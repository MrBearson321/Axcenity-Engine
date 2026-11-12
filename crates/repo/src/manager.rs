//! [`RepositoryManager`]: the brief's repository integration manager, one
//! object the editor keeps that puts GitHub, the stored token and Git
//! together: sign in and out, connect an existing repository, create a
//! new (private) one, disconnect, and read the status.
//!
//! It never writes `collaborate.json`: the caller records the
//! [`Connected`] it returns (the editor, through `axcenity_project`).

use std::path::Path;
use std::sync::Arc;

use crate::credentials::{Credential, CredentialStore, GITHUB_HOST, Stored};
use crate::git::{Author, Git, GitError};
use crate::github::{self, DeviceCode, GitHub, GitHubError, Poll, RepoInfo, User};
use crate::repository::{self, LinkError, Linked, RemoteSpec, Status};
use crate::secret::Secret;

/// A project's link, to record in `collaborate.json`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connected {
    /// `github` or `git`.
    pub provider: String,
    /// The HTTPS URL (no user name or token).
    pub url: String,
    pub branch: String,
    /// What linking did to the folder.
    pub linked: Linked,
}

/// Why connecting failed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConnectError {
    #[error(transparent)]
    GitHub(#[from] GitHubError),
    #[error(transparent)]
    Link(#[from] LinkError),
    #[error("that isn't a GitHub repository. Use owner/name or its github.com address.")]
    NotARepository,
    #[error(
        "you can see {0} but can't upload to it. Ask its owner to add you, or make a new repository."
    )]
    ReadOnly(String),
}

impl From<GitError> for ConnectError {
    fn from(error: GitError) -> ConnectError {
        ConnectError::Link(LinkError::Git(error))
    }
}

/// GitHub, the token and Git together.
pub struct RepositoryManager {
    pub git: Git,
    pub github: GitHub,
    credentials: Arc<dyn CredentialStore>,
}

impl RepositoryManager {
    pub fn new(
        git: Git,
        github: GitHub,
        credentials: Arc<dyn CredentialStore>,
    ) -> RepositoryManager {
        RepositoryManager {
            git,
            github,
            credentials,
        }
    }

    /// The real thing: the installed `git`, github.com, Git's credential
    /// store.
    pub fn system() -> RepositoryManager {
        let git = Git::system();
        let credentials = Arc::new(crate::credentials::GitCredentials::new(git.clone()));
        RepositoryManager::new(git, GitHub::default(), credentials)
    }

    /// The stored GitHub token, if any.
    pub fn token(&self) -> Option<Secret> {
        self.credentials.get(GITHUB_HOST).map(|c| c.secret)
    }

    /// `git` with the stored token, for remote steps.
    pub fn git_with_token(&self) -> Git {
        self.git.with_token(self.token())
    }

    /// Who's signed in (checks the token still works).
    pub fn signed_in(&self) -> Result<Option<User>, GitHubError> {
        match self.token() {
            None => Ok(None),
            Some(token) => self.github.user(&token).map(Some),
        }
    }

    /// Starts signing in with the device flow (the person types the code
    /// at [`github::DEVICE_PAGE`]).
    pub fn start_sign_in(&self, client_id: &str) -> Result<DeviceCode, GitHubError> {
        self.github.start_sign_in(client_id)
    }

    /// Polls the sign-in; on success keeps the token and says who it is.
    pub fn poll_sign_in(&self, client_id: &str, code: &DeviceCode) -> Result<SignIn, GitHubError> {
        match self.github.poll_sign_in(client_id, code)? {
            Poll::Token(token) => self.keep_token(token),
            Poll::Pending => Ok(SignIn::Waiting(None)),
            Poll::SlowDown(interval) => Ok(SignIn::Waiting(Some(interval))),
            Poll::Denied => Ok(SignIn::Denied),
            Poll::Expired => Ok(SignIn::Expired),
        }
    }

    /// Signs in with a token from elsewhere (the GitHub CLI, or one the
    /// person made on github.com): checks it, then keeps it.
    pub fn keep_token(&self, token: Secret) -> Result<SignIn, GitHubError> {
        let user = self.github.user(&token)?;
        let stored = self.credentials.put(
            GITHUB_HOST,
            &Credential {
                username: user.login.clone(),
                secret: token,
            },
        );
        Ok(SignIn::Done { user, stored })
    }

    /// Forgets the token here. GitHub's own page revokes it everywhere:
    /// <https://github.com/settings/applications>.
    pub fn sign_out(&self) {
        self.credentials.remove(GITHUB_HOST);
    }

    /// Links `dir` to the existing GitHub repository `reference`
    /// (`owner/name` or its URL).
    pub fn connect_github(
        &self,
        dir: &Path,
        reference: &str,
        author: &Author,
        replace_remote: bool,
    ) -> Result<Connected, ConnectError> {
        let (owner, name) =
            github::parse_repo_ref(reference).ok_or(ConnectError::NotARepository)?;
        let token = self.token().ok_or(GitHubError::Unauthorized)?;
        let info = self.github.repository(&token, &owner, &name)?;
        if !info.can_push() {
            return Err(ConnectError::ReadOnly(info.full_name));
        }
        self.link(dir, &info, author, replace_remote)
    }

    /// Makes a new GitHub repository `name` (private unless `private` is
    /// false) and links `dir` to it.
    pub fn create_github(
        &self,
        dir: &Path,
        name: &str,
        private: bool,
        author: &Author,
    ) -> Result<Connected, ConnectError> {
        let token = self.token().ok_or(GitHubError::Unauthorized)?;
        let info = self.github.create_repository(
            &token,
            &github::repository_name(name),
            "An Axcenity project",
            private,
        )?;
        self.link(dir, &info, author, false)
    }

    fn link(
        &self,
        dir: &Path,
        info: &RepoInfo,
        author: &Author,
        replace_remote: bool,
    ) -> Result<Connected, ConnectError> {
        let spec = RemoteSpec {
            url: info.clone_url.clone(),
            branch: Some(info.default_branch.clone()),
            replace_remote,
        };
        let (branch, linked) = repository::link(&self.git_with_token(), dir, &spec, author)?;
        Ok(Connected {
            provider: "github".into(),
            url: info.clone_url.clone(),
            branch,
            linked,
        })
    }

    /// Links `dir` to any Git remote by URL (another host than GitHub; its
    /// credentials come from the person's own Git setup).
    pub fn connect_url(
        &self,
        dir: &Path,
        url: &str,
        branch: Option<&str>,
        author: &Author,
        replace_remote: bool,
    ) -> Result<Connected, ConnectError> {
        if url.contains('@') && url.contains("://") {
            return Err(ConnectError::NotARepository);
        }
        let spec = RemoteSpec {
            url: url.to_owned(),
            branch: branch.map(str::to_owned),
            replace_remote,
        };
        let git = if url.starts_with(github::WEB) {
            self.git_with_token()
        } else {
            self.git.clone()
        };
        let (branch, linked) = repository::link(&git, dir, &spec, author)?;
        Ok(Connected {
            provider: if url.starts_with(github::WEB) {
                "github"
            } else {
                "git"
            }
            .into(),
            url: url.to_owned(),
            branch,
            linked,
        })
    }

    /// Unlinks `dir` (the remote only; files and history stay).
    pub fn disconnect(&self, dir: &Path) -> Result<(), GitError> {
        repository::unlink(&self.git, dir)
    }

    /// Where `dir` stands (no network).
    pub fn status(&self, dir: &Path, branch: &str) -> Status {
        repository::status(&self.git, dir, branch)
    }
}

/// Where a sign-in stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignIn {
    /// Not yet; poll again (after this interval, if GitHub asked for a
    /// slower pace).
    Waiting(Option<std::time::Duration>),
    Done {
        user: User,
        stored: Stored,
    },
    Denied,
    Expired,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryCredentials;
    use crate::github::{Http, Response};
    use crate::testing::{author, bare_remote, git, temp, write};
    use std::sync::Mutex;

    /// GitHub answering by URL, with `clone_url` pointing at a local
    /// bare repository.
    struct FakeGitHub {
        clone_url: String,
        push: bool,
        made: Mutex<Vec<String>>,
    }

    impl Http for FakeGitHub {
        fn request(
            &self,
            method: &str,
            url: &str,
            headers: &[(&str, String)],
            body: Option<String>,
        ) -> Result<Response, String> {
            let authorized = headers
                .iter()
                .any(|(k, v)| *k == "Authorization" && v == "Bearer gho_goodtoken123456789");
            let repo = serde_json::json!({
                "full_name": "paul/castle",
                "clone_url": self.clone_url,
                "html_url": "https://github.com/paul/castle",
                "default_branch": "main",
                "private": true,
                "permissions": { "push": self.push, "admin": false },
            })
            .to_string();
            let answer = |status: u16, body: String| Ok(Response { status, body });
            if url.ends_with("/login/oauth/access_token") {
                return answer(200, r#"{"access_token":"gho_goodtoken123456789"}"#.into());
            }
            if !authorized {
                return answer(401, r#"{"message":"Bad credentials"}"#.into());
            }
            match (method, url) {
                ("GET", u) if u.ends_with("/user") => {
                    answer(200, r#"{"login":"paul","id":7,"name":"Paul"}"#.into())
                }
                ("GET", u) if u.ends_with("/repos/paul/castle") => answer(200, repo),
                ("POST", u) if u.ends_with("/user/repos") => {
                    self.made.lock().unwrap().push(body.unwrap_or_default());
                    answer(201, repo)
                }
                _ => answer(404, r#"{"message":"Not Found"}"#.into()),
            }
        }
    }

    fn manager(clone_url: &str, push: bool) -> (RepositoryManager, MemoryCredentials) {
        let credentials = MemoryCredentials::default();
        let github = GitHub::new(Box::new(FakeGitHub {
            clone_url: clone_url.to_owned(),
            push,
            made: Mutex::new(Vec::new()),
        }));
        (
            RepositoryManager::new(git(), github, Arc::new(credentials.clone())),
            credentials,
        )
    }

    fn code() -> DeviceCode {
        DeviceCode {
            user_code: "ABCD-EFGH".into(),
            verification_uri: github::DEVICE_PAGE.into(),
            device_code: Secret::new("device"),
            interval: std::time::Duration::from_secs(5),
            expires_in: std::time::Duration::from_secs(900),
        }
    }

    #[test]
    fn signing_in_keeps_the_token_and_signing_out_forgets_it() {
        let (manager, credentials) = manager("file:///nowhere", true);
        assert_eq!(manager.signed_in(), Ok(None));
        let SignIn::Done { user, .. } = manager.poll_sign_in("id", &code()).unwrap() else {
            panic!()
        };
        assert_eq!(user.login, "paul");
        assert_eq!(manager.signed_in().unwrap().unwrap().login, "paul");
        assert!(credentials.get(GITHUB_HOST).is_some());
        manager.sign_out();
        assert_eq!(manager.signed_in(), Ok(None));
    }

    #[test]
    fn a_bad_token_isnt_kept() {
        let (manager, credentials) = manager("file:///nowhere", true);
        assert_eq!(
            manager.keep_token(Secret::new("gho_wrongtoken")),
            Err(GitHubError::Unauthorized)
        );
        assert!(credentials.get(GITHUB_HOST).is_none());
    }

    #[test]
    fn connecting_an_existing_repository_and_creating_a_new_one() {
        let (_remote, url) = bare_remote(&git());
        let (manager, _) = manager(&url, true);
        let project = temp("project");
        write(&project, "project.json", "{}");
        // Not signed in yet.
        assert_eq!(
            manager.connect_github(&project, "paul/castle", &author("Paul"), false),
            Err(ConnectError::GitHub(GitHubError::Unauthorized))
        );
        manager
            .keep_token(Secret::new("gho_goodtoken123456789"))
            .unwrap();
        let connected = manager
            .connect_github(
                &project,
                "https://github.com/paul/castle",
                &author("Paul"),
                false,
            )
            .unwrap();
        assert_eq!(connected.branch, "main");
        assert!(matches!(connected.linked, Linked::Uploaded { .. }));
        let status = manager.status(&project, "main");
        assert_eq!(status.remote.as_deref(), Some(url.as_str()));
        assert_eq!((status.ahead, status.behind), (0, 0));
        // The token is nowhere in the folder.
        assert!(!contains_in_folder(&project, "gho_goodtoken123456789"));

        // A new repository is private by default.
        let other = temp("other");
        write(&other, "project.json", "{}");
        let (_remote2, url2) = bare_remote(&git());
        let (manager2, _) = self::manager(&url2, true);
        manager2
            .keep_token(Secret::new("gho_goodtoken123456789"))
            .unwrap();
        manager2
            .create_github(&other, "My Castle", true, &author("Paul"))
            .unwrap();
    }

    #[test]
    fn a_repository_you_cant_push_to_is_refused() {
        let (_remote, url) = bare_remote(&git());
        let (manager, _) = manager(&url, false);
        manager
            .keep_token(Secret::new("gho_goodtoken123456789"))
            .unwrap();
        let project = temp("project");
        assert_eq!(
            manager.connect_github(&project, "paul/castle", &author("Paul"), false),
            Err(ConnectError::ReadOnly("paul/castle".into()))
        );
        assert!(!project.join(".git").exists());
    }

    fn contains_in_folder(dir: &Path, needle: &str) -> bool {
        let mut stack = vec![dir.to_owned()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if std::fs::read(&path)
                    .is_ok_and(|b| String::from_utf8_lossy(&b).contains(needle))
                {
                    return true;
                }
            }
        }
        false
    }
}
