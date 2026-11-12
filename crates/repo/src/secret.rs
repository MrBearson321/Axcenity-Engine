//! Secrets (access tokens) that can't end up in a log by accident.
//!
//! A [`Secret`] prints as `***` with `{:?}` and has no `Display`, so a
//! token can only be read on purpose ([`Secret::expose`]). [`redact`]
//! scrubs text that came back from `git` or a web server before it's
//! logged or shown, in case a token or a URL with a password slipped in.

use std::fmt;

/// A token or password. Compare and clone it freely; print it never.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    pub fn new(text: impl Into<String>) -> Secret {
        Secret(text.into())
    }

    /// The secret itself, for the one place that sends it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(***)")
    }
}

/// `text` with every secret in `secrets` and every password in a URL
/// (`https://user:password@host`) replaced by `***`.
pub fn redact(text: &str, secrets: &[&Secret]) -> String {
    let mut out = text.to_owned();
    for secret in secrets {
        // Short strings would scrub ordinary words; real tokens are long.
        if secret.0.len() >= 8 {
            out = out.replace(&secret.0, "***");
        }
    }
    redact_url_passwords(&out)
}

/// Replaces the password part of `scheme://user:password@host` with `***`.
fn redact_url_passwords(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("://") {
        let (head, tail) = rest.split_at(start + 3);
        out.push_str(head);
        // The authority ends at the first character that can't be in it.
        let end = tail
            .find(|c: char| c.is_whitespace() || matches!(c, '/' | '"' | '\'' | '>'))
            .unwrap_or(tail.len());
        let authority = &tail[..end];
        match authority.rfind('@') {
            Some(at) => {
                let user_info = &authority[..at];
                match user_info.find(':') {
                    Some(colon) => {
                        out.push_str(&user_info[..colon]);
                        out.push_str(":***");
                    }
                    // A bare user name before `@` is often the token itself
                    // (`https://<token>@github.com`).
                    None if user_info.len() >= 20 => out.push_str("***"),
                    None => out.push_str(user_info),
                }
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_never_prints() {
        let secret = Secret::new("gho_abcdefghijklmnop");
        assert_eq!(format!("{secret:?}"), "Secret(***)");
        assert_eq!(format!("{:?}", Some(secret.clone())), "Some(Secret(***))");
        assert_eq!(secret.expose(), "gho_abcdefghijklmnop");
    }

    #[test]
    fn redact_scrubs_tokens_and_url_passwords() {
        let token = Secret::new("gho_abcdefghijklmnop");
        let text = "fatal: could not read from https://x-access-token:gho_abcdefghijklmnop@github.com/o/r.git \
                    and gho_abcdefghijklmnop again; https://ghp_0123456789abcdefghij@github.com/o/r; \
                    https://paul@github.com/o/r stays";
        let clean = redact(text, &[&token]);
        assert!(!clean.contains("gho_abcdefghijklmnop"), "{clean}");
        assert!(!clean.contains("ghp_0123456789abcdefghij"), "{clean}");
        assert!(
            clean.contains("https://x-access-token:***@github.com/o/r.git"),
            "{clean}"
        );
        assert!(clean.contains("https://***@github.com/o/r"), "{clean}");
        assert!(
            clean.contains("https://paul@github.com/o/r stays"),
            "{clean}"
        );
    }

    #[test]
    fn short_secrets_dont_scrub_words() {
        let short = Secret::new("main");
        assert_eq!(redact("on branch main", &[&short]), "on branch main");
    }
}
