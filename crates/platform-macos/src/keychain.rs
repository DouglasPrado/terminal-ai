//! macOS Keychain access for the few secrets the app must hold.
//!
//! Terminal AI's rule is that credentials live in the Keychain or the provider CLIs' own files, and
//! never in `app.db` or `config.toml`. Until now the repo only ever *read* a Keychain item (the
//! Claude credential lookup in `usage-core`); the memory kernel's optional bearer token is the
//! first secret the app may need to write.
//!
//! The write path deliberately feeds the secret on **stdin**. `security add-generic-password -w
//! <secret>` puts it on the command line, where any `ps` on the machine can read it for the
//! lifetime of the call — a small window, but a needless one.

use crate::PlatformError;
use std::io::Write;
use std::process::{Command, Stdio};

/// The Keychain service Terminal AI stores its own items under.
pub const SERVICE: &str = "dev.terminal-ai.app";

/// Read a generic password. `Ok(None)` means "no such item", which is not an error.
pub fn get(service: &str, account: &str) -> Result<Option<String>, PlatformError> {
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", service, "-a", account, "-w"])
        .output()?;
    if !output.status.success() {
        // `security` exits non-zero for a missing item exactly as it does for a real failure, so
        // absence is the only sane interpretation of "we asked and got nothing".
        return Ok(None);
    }
    let value = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    Ok(if value.is_empty() { None } else { Some(value) })
}

/// Create or replace a generic password.
pub fn set(service: &str, account: &str, secret: &str) -> Result<(), PlatformError> {
    let mut child = Command::new("/usr/bin/security")
        .args([
            "add-generic-password",
            "-s",
            service,
            "-a",
            account,
            "-U", // update an existing item instead of failing
            "-w", // read the secret from stdin, NOT from argv
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(secret.as_bytes())?;
        stdin.write_all(b"\n")?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(PlatformError::Keychain(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(())
}

/// Remove a generic password. Removing something that is not there succeeds.
pub fn delete(service: &str, account: &str) -> Result<(), PlatformError> {
    let output = Command::new("/usr/bin/security")
        .args(["delete-generic-password", "-s", service, "-a", account])
        .output()?;
    let _ = output.status;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Touches the real login Keychain, so it is opt-in rather than part of `cargo test`.
    #[test]
    #[ignore = "writes to the real macOS Keychain; run with --ignored"]
    fn round_trips_a_secret() {
        let account = "terminal-ai-keychain-test";
        set(SERVICE, account, "s3cret-value").expect("set");
        assert_eq!(
            get(SERVICE, account).expect("get").as_deref(),
            Some("s3cret-value")
        );
        delete(SERVICE, account).expect("delete");
        assert_eq!(get(SERVICE, account).expect("get after delete"), None);
    }
}
