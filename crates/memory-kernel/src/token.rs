//! A bearer token that will not leak through the two channels secrets usually escape by:
//! a stray `{:?}` and a `tracing` field.
//!
//! On loopback the kernel needs no token at all, which is the app's default. This type exists for
//! the case where the user attaches to a server that does have one.

use std::fmt;

/// A kernel bearer token. Its `Debug` and `Display` are deliberately useless.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthToken(String);

impl AuthToken {
    #[must_use]
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The only way to see the secret. Named so that a reviewer notices it at a call site.
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn header_value(&self) -> String {
        format!("Bearer {}", self.0)
    }
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthToken(<redacted>)")
    }
}

impl fmt::Display for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secret_never_appears_in_debug_or_display_output() {
        let token = AuthToken::new("aim_super_secret_value");

        assert!(!format!("{token:?}").contains("super_secret"));
        assert!(!format!("{token}").contains("super_secret"));
        // The common accident: a struct that derives Debug and happens to hold a token.
        #[derive(Debug)]
        struct Config {
            #[allow(dead_code)]
            url: String,
            #[allow(dead_code)]
            token: Option<AuthToken>,
        }
        let config = Config {
            url: "http://127.0.0.1:49374".into(),
            token: Some(token),
        };
        assert!(!format!("{config:?}").contains("super_secret"));
    }

    #[test]
    fn exposing_is_explicit_and_builds_the_header() {
        let token = AuthToken::new("abc123");
        assert_eq!(token.expose(), "abc123");
        assert_eq!(token.header_value(), "Bearer abc123");
    }
}
