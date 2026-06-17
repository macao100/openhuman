//! Fallback backend: fail-closed.
//!
//! Used when no OS-level jail is available (unsupported platform, missing
//! kernel feature, etc.). Previously spawned without enforcement (audit-only),
//! which created a silent security degradation. Now returns an explicit error.
//!
//! Set `DADOU_SANDBOX_PERMISSIVE=1` to override — this restores the old
//! pass-through behaviour for development environments where no sandbox
//! backend exists.

use std::process::Command;

use super::jail::{Jail, JailBackend, JailedChild};

/// Returns `true` when the `DADOU_SANDBOX_PERMISSIVE` env var is set to `1`.
fn is_permissive() -> bool {
    std::env::var("DADOU_SANDBOX_PERMISSIVE")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false)
}

#[derive(Debug, Default)]
pub struct NoopBackend;

impl JailBackend for NoopBackend {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn is_available(&self) -> bool {
        // Always available — it's the fallback backend. Its `spawn` will
        // fail closed unless DADOU_SANDBOX_PERMISSIVE is set.
        true
    }

    fn spawn(&self, _jail: &Jail, mut cmd: Command) -> std::io::Result<JailedChild> {
        if is_permissive() {
            log::warn!("[cwd_jail] DADOU_SANDBOX_PERMISSIVE=1 — spawning WITHOUT sandbox!");
            cmd.spawn().map(JailedChild::Std)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "[fail-closed] No sandbox backend available — refusing to spawn \
                 unrestricted. Install a supported sandbox backend (Windows: \
                 Restricted Token / AppContainer, Linux: Landlock, macOS: \
                 Seatbelt) or set DADOU_SANDBOX_PERMISSIVE=1 to override.",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openhuman::cwd_jail::Jail;

    #[test]
    fn noop_fails_closed() {
        // Without DADOU_SANDBOX_PERMISSIVE, spawn returns
        // Err(Unsupported).
        let backend = NoopBackend;
        let dir = std::env::temp_dir();
        let jail = Jail::new(&dir, "test.fail-closed");
        let cmd = Command::new(if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "true"
        });
        let err = backend.spawn(&jail, cmd).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::Unsupported);
        assert!(
            err.to_string().contains("fail-closed"),
            "error message should mention fail-closed, got: {}",
            err
        );
    }

    #[test]
    fn noop_permissive_with_env() {
        // DADOU_SANDBOX_PERMISSIVE=1 should allow unrestricted spawn.
        // Save and restore the env var.
        let prev = std::env::var("DADOU_SANDBOX_PERMISSIVE").ok();
        std::env::set_var("DADOU_SANDBOX_PERMISSIVE", "1");
        let backend = NoopBackend;
        let dir = std::env::temp_dir();
        let jail = Jail::new(&dir, "test.permissive");
        let mut cmd = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit"]);
            c
        } else {
            Command::new("true")
        };
        // Use Command to avoid borrowing issues - just spawn a
        // placeholder command to verify the path works.
        let result = backend.spawn(&jail, cmd);
        match prev {
            Some(v) => std::env::set_var("DADOU_SANDBOX_PERMISSIVE", v),
            None => std::env::remove_var("DADOU_SANDBOX_PERMISSIVE"),
        }
        // The spawn should succeed because we set the permissive flag.
        assert!(result.is_ok(), "permissive spawn should succeed");
    }

    #[test]
    fn noop_metadata() {
        assert_eq!(NoopBackend.name(), "noop");
        assert!(NoopBackend.is_available());
    }
}
