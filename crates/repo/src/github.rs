//! GitHub: signing in with the OAuth device flow, and finding or making a
//! repository through the REST API (P1 of
//! `docs/research/collaborate-persistent.md`).
//!
//! **Device flow:** Axcenity asks GitHub for a short code, the person
//! opens <https://github.com/login/device> and types it, and Axcenity
//! polls until GitHub hands over a token. No web server, no redirect, no
//! password typed into Axcenity. The scope is `repo`, the narrowest OAuth
//! scope that covers private repositories.
//!
//! Every request goes through [`Http`], so tests answer with recorded
//! JSON. [`UreqHttp`] is the real one.

use std::time::Duration;

use serde::Deserialize;

use crate::secret::{Secret, redact};

/// GitHub's web address.
pub const WEB: &str = "https://github.com";
/// GitHub's REST API.
pub const API: &str = "https://api.github.com";
/// The OAuth scope Axcenity asks for.
pub const SCOPE: &str = "repo";
/// Where the person types the code.
pub const DEVICE_PAGE: &str = "https://github.com/login/device";
/// The environment variable (at build time, or when running) naming the
/// OAuth App's client ID. Official builds set it.
pub const CLIENT_ID_VAR: &str = "AXCENITY_GITHUB_CLIENT_ID";

/// The OAuth App client ID this build signs in with, if any: from the
/// environment when running, else from when it was built.
pub fn client_id() -> Option<String> {
    std::env::var(CLIENT_ID_VAR)
        .ok()
        .or_else(|| option_env!("AXCENITY_GITHUB_CLIENT_ID").map(str::to_owned))
        .filter(|id| !id.trim().is_empty())
}

/// An HTTP answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
}

/// One HTTP request: `method` to `url` with `headers` and an optional
/// body. Errors are for when no answer came at all.
pub trait Http: Send + Sync {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, String)],
        body: Option<String>,
    ) -> Result<Response, String>;
}

/// HTTPS with `ureq`. Every status is an answer, not an error.
pub struct UreqHttp {
    agent: ureq::Agent,
}

impl Default for UreqHttp {
    fn default() -> UreqHttp {
        let agent = ureq::Agent::config_builder()
            .user_agent(concat!("Axcenity/", env!("CARGO_PKG_VERSION")))
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .into();
        UreqHttp { agent }
    }
}

impl Http for UreqHttp {
    fn request(
        &self,
        method: &str,
        url: &str,
        headers: &[(&str, String)],
        body: Option<String>,
    ) -> Result<Response, String> {
        if !url.starts_with("https://") {
            return Err(format!("{url} isn't https"));
        }
        let response = match method {
            "GET" | "DELETE" => {
                let mut request = if method == "GET" {
                    self.agent.get(url)
                } else {
                    self.agent.delete(url)
                };
                for (name, value) in headers {
                    request = request.header(*name, value.as_str());
                }
                request.call()
            }
            _ => {
                let mut request = self.agent.post(url);
                for (name, value) in headers {
                    request = request.header(*name, value.as_str());
                }
                request.send(body.unwrap_or_default())
            }
        }
        .map_err(|e| e.to_string())?;
        let status = response.status().as_u16();
        let body = response
            .into_body()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok(Response { status, body })
    }
}

/// Why a GitHub step failed, in words for the person.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum GitHubError {
    #[error("couldn't reach GitHub: {0}")]
    Offline(String),
    #[error(
        "GitHub sign-in isn't set up in this build of Axcenity (no OAuth client ID). Sign in with the GitHub CLI (`gh auth login`) or paste a token instead."
    )]
    NoClientId,
    #[error("you're not signed in to GitHub, or the sign-in has expired. Sign in again.")]
    Unauthorized,
    #[error("GitHub says you don't have access to that: {0}")]
    Forbidden(String),
    #[error("there's no repository {0} on GitHub (or you can't see it)")]
    NotFound(String),
    #[error("{0}")]
    Refused(String),
}

/// What the person is shown while signing in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    /// The code to type, `WDJB-MJHT`.
    pub user_code: String,
    /// Where to type it.
    pub verification_uri: String,
    /// What Axcenity polls with. Not shown.
    pub device_code: Secret,
    /// The least time between two polls.
    pub interval: Duration,
    /// How long the code works.
    pub expires_in: Duration,
}

/// One poll of a device sign-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll {
    /// Not yet: poll again after the interval.
    Pending,
    /// Polled too often: wait this long between polls from now on.
    SlowDown(Duration),
    /// Signed in.
    Token(Secret),
    /// The person said no on GitHub's page.
    Denied,
    /// The code ran out; start again.
    Expired,
}

/// A repository on GitHub.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct RepoInfo {
    /// `owner/name`.
    pub full_name: String,
    /// The HTTPS URL to clone.
    pub clone_url: String,
    pub html_url: String,
    pub default_branch: String,
    pub private: bool,
    /// Whether the signed-in person may push. GitHub leaves it out for
    /// people who can't see the permissions.
    #[serde(default)]
    pub permissions: Option<Permissions>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Permissions {
    #[serde(default)]
    pub push: bool,
    #[serde(default)]
    pub admin: bool,
}

impl RepoInfo {
    pub fn can_push(&self) -> bool {
        self.permissions.as_ref().is_some_and(|p| p.push || p.admin)
    }
}

/// The signed-in person.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct User {
    pub login: String,
    pub id: u64,
    #[serde(default)]
    pub name: Option<String>,
}

impl User {
    /// The address GitHub keeps private for commits.
    pub fn noreply_email(&self) -> String {
        format!("{}+{}@users.noreply.github.com", self.id, self.login)
    }
}

/// GitHub, through an [`Http`].
pub struct GitHub {
    http: Box<dyn Http>,
    web: String,
    api: String,
}

impl Default for GitHub {
    fn default() -> GitHub {
        GitHub::new(Box::new(UreqHttp::default()))
    }
}

impl GitHub {
    pub fn new(http: Box<dyn Http>) -> GitHub {
        GitHub {
            http,
            web: WEB.to_owned(),
            api: API.to_owned(),
        }
    }

    /// Starts a device sign-in for the OAuth App `client_id`.
    pub fn start_sign_in(&self, client_id: &str) -> Result<DeviceCode, GitHubError> {
        if client_id.trim().is_empty() {
            return Err(GitHubError::NoClientId);
        }
        let body = form(&[("client_id", client_id), ("scope", SCOPE)]);
        let response = self.post_form(&format!("{}/login/device/code", self.web), body)?;
        #[derive(Deserialize)]
        struct Answer {
            device_code: String,
            user_code: String,
            verification_uri: String,
            expires_in: u64,
            interval: u64,
        }
        if let Some(error) = oauth_error(&response.body) {
            return Err(match error.as_str() {
                "device_flow_disabled" | "incorrect_client_credentials" => GitHubError::NoClientId,
                other => GitHubError::Refused(format!("GitHub refused the sign-in ({other})")),
            });
        }
        let answer: Answer = parse(&response)?;
        Ok(DeviceCode {
            user_code: answer.user_code,
            verification_uri: answer.verification_uri,
            device_code: Secret::new(answer.device_code),
            interval: Duration::from_secs(answer.interval.max(1)),
            expires_in: Duration::from_secs(answer.expires_in),
        })
    }

    /// Asks once whether the person has typed the code yet.
    pub fn poll_sign_in(&self, client_id: &str, code: &DeviceCode) -> Result<Poll, GitHubError> {
        let body = form(&[
            ("client_id", client_id),
            ("device_code", code.device_code.expose()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ]);
        let response = self.post_form(&format!("{}/login/oauth/access_token", self.web), body)?;
        #[derive(Deserialize)]
        struct Answer {
            #[serde(default)]
            access_token: Option<String>,
            #[serde(default)]
            error: Option<String>,
            #[serde(default)]
            interval: Option<u64>,
        }
        let answer: Answer = parse(&response)?;
        if let Some(token) = answer.access_token.filter(|t| !t.is_empty()) {
            return Ok(Poll::Token(Secret::new(token)));
        }
        match answer.error.as_deref() {
            Some("authorization_pending") => Ok(Poll::Pending),
            Some("slow_down") => Ok(Poll::SlowDown(Duration::from_secs(
                answer.interval.unwrap_or(code.interval.as_secs() + 5),
            ))),
            Some("access_denied") => Ok(Poll::Denied),
            Some("expired_token") | Some("token_expired") => Ok(Poll::Expired),
            Some("device_flow_disabled") | Some("incorrect_client_credentials") => {
                Err(GitHubError::NoClientId)
            }
            Some(other) => Err(GitHubError::Refused(format!(
                "GitHub refused the sign-in ({other})"
            ))),
            None => Err(GitHubError::Refused("GitHub's answer had no token".into())),
        }
    }

    /// Who `token` belongs to (also checks it still works).
    pub fn user(&self, token: &Secret) -> Result<User, GitHubError> {
        let response = self.api_call("GET", "/user", token, None)?;
        check(&response, token, "your account")?;
        parse(&response)
    }

    /// The repository `owner/name`.
    pub fn repository(
        &self,
        token: &Secret,
        owner: &str,
        name: &str,
    ) -> Result<RepoInfo, GitHubError> {
        let response = self.api_call("GET", &format!("/repos/{owner}/{name}"), token, None)?;
        check(&response, token, &format!("{owner}/{name}"))?;
        parse(&response)
    }

    /// Makes a repository called `name` for the signed-in person, private
    /// unless `private` is false, with no commits.
    pub fn create_repository(
        &self,
        token: &Secret,
        name: &str,
        description: &str,
        private: bool,
    ) -> Result<RepoInfo, GitHubError> {
        let body = serde_json::json!({
            "name": name,
            "description": description,
            "private": private,
            "auto_init": false,
        })
        .to_string();
        let response = self.api_call("POST", "/user/repos", token, Some(body))?;
        if response.status == 422 {
            return Err(GitHubError::Refused(format!(
                "GitHub couldn't make `{name}`: {}",
                api_message(&response.body)
                    .unwrap_or_else(|| "a repository with that name may already exist".into())
            )));
        }
        check(&response, token, name)?;
        parse(&response)
    }

    fn post_form(&self, url: &str, body: String) -> Result<Response, GitHubError> {
        self.http
            .request(
                "POST",
                url,
                &[
                    ("Accept", "application/json".into()),
                    ("Content-Type", "application/x-www-form-urlencoded".into()),
                ],
                Some(body),
            )
            .map_err(GitHubError::Offline)
    }

    fn api_call(
        &self,
        method: &str,
        path: &str,
        token: &Secret,
        body: Option<String>,
    ) -> Result<Response, GitHubError> {
        let mut headers = vec![
            ("Accept", "application/vnd.github+json".to_owned()),
            ("Authorization", format!("Bearer {}", token.expose())),
            ("X-GitHub-Api-Version", "2022-11-28".to_owned()),
        ];
        if body.is_some() {
            headers.push(("Content-Type", "application/json".to_owned()));
        }
        self.http
            .request(method, &format!("{}{path}", self.api), &headers, body)
            .map_err(|e| GitHubError::Offline(redact(&e, &[token])))
    }
}

/// Splits `owner/name`, `https://github.com/owner/name(.git)` or
/// `git@github.com:owner/name.git` into owner and name.
pub fn parse_repo_ref(text: &str) -> Option<(String, String)> {
    let text = text.trim().trim_end_matches('/');
    let rest = if let Some(rest) = text.strip_prefix("git@github.com:") {
        rest
    } else if let Some(at) = text.find("github.com/") {
        &text[at + "github.com/".len()..]
    } else if text.contains("://") || text.contains('@') {
        return None;
    } else {
        text
    };
    let rest = rest.trim_end_matches(".git");
    let mut parts = rest.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    let valid = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    (parts.next().is_none() && valid(owner) && valid(name) && name != "." && name != "..")
        .then(|| (owner.to_owned(), name.to_owned()))
}

/// The HTTPS clone URL of `owner/name` on GitHub.
pub fn clone_url(owner: &str, name: &str) -> String {
    format!("{WEB}/{owner}/{name}.git")
}

/// A name GitHub takes for a repository, made from a project's name:
/// letters, digits, `-`, `_` and `.`; anything else becomes `-`.
pub fn repository_name(project: &str) -> String {
    let mut name: String = project
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    while name.contains("--") {
        name = name.replace("--", "-");
    }
    let name = name.trim_matches(|c| c == '-' || c == '.').to_owned();
    if name.is_empty() {
        "axcenity-project".to_owned()
    } else {
        name.chars().take(100).collect()
    }
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn encode(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn oauth_error(body: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()?
        .get("error")?
        .as_str()
        .map(str::to_owned)
}

fn api_message(body: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let message = value.get("message")?.as_str()?.to_owned();
    let detail = value
        .get("errors")
        .and_then(|e| e.as_array())
        .and_then(|e| e.first())
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str());
    Some(match detail {
        Some(detail) => format!("{message} ({detail})"),
        None => message,
    })
}

fn check(response: &Response, token: &Secret, what: &str) -> Result<(), GitHubError> {
    match response.status {
        200..=299 => Ok(()),
        401 => Err(GitHubError::Unauthorized),
        403 => Err(GitHubError::Forbidden(redact(
            &api_message(&response.body).unwrap_or_else(|| what.to_owned()),
            &[token],
        ))),
        404 => Err(GitHubError::NotFound(what.to_owned())),
        status => Err(GitHubError::Refused(format!(
            "GitHub answered {status}: {}",
            redact(&api_message(&response.body).unwrap_or_default(), &[token])
        ))),
    }
}

fn parse<T: for<'de> Deserialize<'de>>(response: &Response) -> Result<T, GitHubError> {
    serde_json::from_str(&response.body)
        .map_err(|e| GitHubError::Refused(format!("GitHub's answer didn't make sense ({e})")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    type Seen = Arc<Mutex<Vec<(String, String, Vec<(String, String)>, Option<String>)>>>;

    /// Answers each request with the next recorded response.
    struct Recorded {
        answers: Mutex<VecDeque<Result<Response, String>>>,
        seen: Seen,
    }

    fn github(answers: Vec<Result<(u16, &str), &str>>) -> (GitHub, Seen) {
        let seen: Seen = Arc::default();
        let http = Recorded {
            answers: Mutex::new(
                answers
                    .into_iter()
                    .map(|a| {
                        a.map(|(status, body)| Response {
                            status,
                            body: body.to_owned(),
                        })
                        .map_err(str::to_owned)
                    })
                    .collect(),
            ),
            seen: seen.clone(),
        };
        (GitHub::new(Box::new(http)), seen)
    }

    impl Http for Recorded {
        fn request(
            &self,
            method: &str,
            url: &str,
            headers: &[(&str, String)],
            body: Option<String>,
        ) -> Result<Response, String> {
            self.seen.lock().unwrap().push((
                method.to_owned(),
                url.to_owned(),
                headers
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
                body,
            ));
            self.answers.lock().unwrap().pop_front().unwrap()
        }
    }

    const CODE: &str = r#"{"device_code":"3584d83530557fdd1f46af8289938c8ef79f9dc5","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#;

    #[test]
    fn a_device_sign_in_waits_slows_down_and_gets_a_token() {
        let (github, seen) = github(vec![
            Ok((200, CODE)),
            Ok((200, r#"{"error":"authorization_pending"}"#)),
            Ok((200, r#"{"error":"slow_down","interval":10}"#)),
            Ok((
                200,
                r#"{"access_token":"gho_16C7e42F292c6912E7710c838347Ae178B4a","token_type":"bearer","scope":"repo"}"#,
            )),
        ]);
        let code = github.start_sign_in("Iv1.abc").unwrap();
        assert_eq!(code.user_code, "WDJB-MJHT");
        assert_eq!(code.interval, Duration::from_secs(5));
        assert_eq!(code.expires_in, Duration::from_secs(900));
        assert_eq!(
            github.poll_sign_in("Iv1.abc", &code).unwrap(),
            Poll::Pending
        );
        assert_eq!(
            github.poll_sign_in("Iv1.abc", &code).unwrap(),
            Poll::SlowDown(Duration::from_secs(10))
        );
        assert_eq!(
            github.poll_sign_in("Iv1.abc", &code).unwrap(),
            Poll::Token(Secret::new("gho_16C7e42F292c6912E7710c838347Ae178B4a"))
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].1, "https://github.com/login/device/code");
        assert_eq!(seen[0].3.as_deref(), Some("client_id=Iv1.abc&scope=repo"));
        assert_eq!(seen[1].1, "https://github.com/login/oauth/access_token");
        assert!(
            seen[1]
                .3
                .as_ref()
                .unwrap()
                .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code")
        );
    }

    #[test]
    fn a_device_sign_in_can_be_denied_or_expire() {
        let (github, _) = github(vec![
            Ok((200, CODE)),
            Ok((200, r#"{"error":"access_denied"}"#)),
            Ok((200, r#"{"error":"expired_token"}"#)),
        ]);
        let code = github.start_sign_in("id").unwrap();
        assert_eq!(github.poll_sign_in("id", &code).unwrap(), Poll::Denied);
        assert_eq!(github.poll_sign_in("id", &code).unwrap(), Poll::Expired);
    }

    #[test]
    fn without_a_client_id_or_device_flow_it_says_so() {
        let (github, _) = github(vec![Ok((200, r#"{"error":"device_flow_disabled"}"#))]);
        assert_eq!(github.start_sign_in(""), Err(GitHubError::NoClientId));
        assert_eq!(github.start_sign_in("id"), Err(GitHubError::NoClientId));
    }

    #[test]
    fn no_network_is_offline_not_a_refusal() {
        let (github, _) = github(vec![Err("dns failed")]);
        assert!(matches!(
            github.start_sign_in("id"),
            Err(GitHubError::Offline(_))
        ));
    }

    const REPO: &str = r#"{"full_name":"paul/castle","clone_url":"https://github.com/paul/castle.git","html_url":"https://github.com/paul/castle","default_branch":"trunk","private":true,"permissions":{"admin":false,"push":true,"pull":true}}"#;

    #[test]
    fn a_repository_is_looked_up_with_its_default_branch() {
        let token = Secret::new("gho_testtoken1234567890");
        let (github, seen) = github(vec![
            Ok((200, REPO)),
            Ok((404, r#"{"message":"Not Found"}"#)),
        ]);
        let repo = github.repository(&token, "paul", "castle").unwrap();
        assert_eq!(repo.default_branch, "trunk");
        assert!(repo.private && repo.can_push());
        assert_eq!(
            github.repository(&token, "paul", "nope"),
            Err(GitHubError::NotFound("paul/nope".into()))
        );
        let seen = seen.lock().unwrap();
        assert_eq!(seen[0].1, "https://api.github.com/repos/paul/castle");
        assert!(seen[0].2.contains(&(
            "Authorization".into(),
            "Bearer gho_testtoken1234567890".into()
        )));
    }

    #[test]
    fn a_new_repository_is_private_and_empty() {
        let token = Secret::new("gho_testtoken1234567890");
        let (github, seen) = github(vec![
            Ok((201, REPO)),
            Ok((
                422,
                r#"{"message":"Repository creation failed.","errors":[{"message":"name already exists on this account"}]}"#,
            )),
        ]);
        github
            .create_repository(&token, "castle", "An Axcenity project", true)
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_str(seen.lock().unwrap()[0].3.as_ref().unwrap()).unwrap();
        assert_eq!(body["private"], true);
        assert_eq!(body["auto_init"], false);
        let error = github
            .create_repository(&token, "castle", "", true)
            .unwrap_err();
        assert!(error.to_string().contains("name already exists"), "{error}");
    }

    #[test]
    fn an_expired_token_asks_to_sign_in_again() {
        let token = Secret::new("gho_testtoken1234567890");
        let (github, _) = github(vec![Ok((401, r#"{"message":"Bad credentials"}"#))]);
        assert_eq!(github.user(&token), Err(GitHubError::Unauthorized));
    }

    #[test]
    fn repository_references_parse() {
        let pair = |a: &str, b: &str| Some((a.to_owned(), b.to_owned()));
        assert_eq!(parse_repo_ref("paul/castle"), pair("paul", "castle"));
        assert_eq!(
            parse_repo_ref("https://github.com/paul/castle.git"),
            pair("paul", "castle")
        );
        assert_eq!(
            parse_repo_ref("https://github.com/paul/castle/"),
            pair("paul", "castle")
        );
        assert_eq!(
            parse_repo_ref("git@github.com:paul/my.game.git"),
            pair("paul", "my.game")
        );
        assert_eq!(parse_repo_ref("paul"), None);
        assert_eq!(parse_repo_ref("paul/castle/tree/main"), None);
        assert_eq!(parse_repo_ref("https://gitlab.com/paul/castle"), None);
        assert_eq!(parse_repo_ref("paul/../etc"), None);
    }

    #[test]
    fn project_names_become_repository_names() {
        assert_eq!(repository_name("My Castle Game!"), "My-Castle-Game");
        assert_eq!(repository_name("  "), "axcenity-project");
        assert_eq!(repository_name("Röck"), "R-ck");
    }

    #[test]
    fn user_emails_stay_private() {
        let user = User {
            login: "MrBearson321".into(),
            id: 106480040,
            name: None,
        };
        assert_eq!(
            user.noreply_email(),
            "106480040+MrBearson321@users.noreply.github.com"
        );
    }
}
