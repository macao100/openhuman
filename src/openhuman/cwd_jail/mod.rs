//! Directory jail (cwd_jail): jail an agent/tool into a single workspace.
//!
//! ## Why this exists
//!
//! `src/openhuman/security/` already has a `Sandbox` trait that wraps
//! `Command`s (Landlock / Firejail / Bubblewrap / Docker). It works well
//! for Linux but the macOS branch is a stub (`bwrap` doesn't exist there)
//! and there is no Windows backend at all. Callers also have to thread
//! `SecurityConfig` through every call site.
//!
//! `cwd_jail` is the user-facing facade. Callers describe *what* the
//! jail looks like ([`Jail`]) and the module picks the right OS backend:
//!
//! | OS      | Backend            | Mechanism                                  |
//! |---------|--------------------|--------------------------------------------|
//! | Linux   | landlock           | Kernel 5.13+ LSM, applied in `pre_exec`    |
//! | macOS   | seatbelt           | `sandbox-exec -p '<profile>' …`            |
//! | Windows | restricted_token   | `CreateRestrictedToken` + Low IL + CIG/CFG |
//! | Windows | appcontainer       | `CreateAppContainerProfile` (fallback)     |
//! | other   | noop               | fail-closed (returns error)                |
//!
//! ## Quick start
//!
//! ```ignore
//! use openhuman::openhuman::cwd_jail::{spawn, Jail};
//! use std::process::Command;
//!
//! let mut jail = Jail::new("/Users/x/work/proj", "agent.delegate")
//!     .add_read_only("/usr/lib")
//!     .deny_subprocess();
//! jail.canonicalize_or_log();
//!
//! let mut cmd = Command::new("node");
//! cmd.arg("script.js");
//! let mut child = spawn(&jail, cmd)?;
//! let status = child.wait()?;
//! ```
//!
//! ## What this does *not* do
//!
//! - It does not jail the current process. Backends spawn a child. The core
//!   itself is trusted; only the things it shells out to are caged.
//! - It does not replace `security::SecurityPolicy`. The autonomy gate
//!   still decides *whether* a command may run; this module decides
//!   *what filesystem* it sees once approved.
//! - It does not encrypt files. ACLs / Landlock rules / Seatbelt profiles
//!   are the wall — anything inside `root` is fully visible to the child.

pub mod detect;
pub mod jail;
pub mod noop;
pub mod registry;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;
#[cfg(target_os = "windows")]
pub mod windows_restricted;

pub use jail::{Jail, JailBackend, JailedChild};
pub use noop::NoopBackend;
pub use registry::{JailRecord, JailRegistry};

use std::process::Command;
use std::sync::{Arc, OnceLock};

/// Cached default backend for the current platform.
static DEFAULT_BACKEND: OnceLock<Arc<dyn JailBackend>> = OnceLock::new();

/// Returns the process-wide default backend, lazily auto-detected.
pub fn default_backend() -> Arc<dyn JailBackend> {
    DEFAULT_BACKEND.get_or_init(detect::pick_backend).clone()
}

/// Spawn `cmd` inside the jail described by `spawn`, using the default backend.
///
/// `jail.canonicalize()` is called once here so the backends never see
/// `..` or symlinks. If the root does not exist, the spawn fails with
/// `NotFound` (canonicalize bubbles it up) — callers should create the
/// workspace before encapsulating.
pub fn spawn(jail: &Jail, cmd: Command) -> std::io::Result<JailedChild> {
    let mut jail = jail.clone();
    jail.canonicalize()?;
    default_backend().spawn(&jail, cmd)
}

/// Same as [`spawn`] but with a caller-supplied backend. Useful in
/// tests and for callers that want to opt into a weaker backend
/// explicitly (e.g. forcing [`NoopBackend`] during local dev).
pub fn spawn_with(
    backend: &dyn JailBackend,
    jail: &Jail,
    cmd: Command,
) -> std::io::Result<JailedChild> {
    let mut jail = jail.clone();
    jail.canonicalize()?;
    backend.spawn(&jail, cmd)
}

impl Jail {
    /// Best-effort canonicalize that swallows errors and logs them. Most
    /// callers should use the validating [`Jail::canonicalize`] path that
    /// [`spawn`] runs automatically.
    pub fn canonicalize_or_log(&mut self) {
        if let Err(e) = self.canonicalize() {
            log::warn!(
                "[cwd_jail] failed to canonicalize jail root {}: {}",
                self.root.display(),
                e
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jail_builder_chains() {
        let j = Jail::new("/tmp", "x")
            .add_read_only("/usr/lib")
            .deny_net()
            .deny_subprocess();
        assert_eq!(j.read_only.len(), 1);
        assert!(!j.allow_net);
        assert!(!j.allow_subprocess);
    }

    #[test]
    fn missing_root_errors() {
        let jail = Jail::new("/this/does/not/exist/ever", "x");
        let err = spawn_with(&NoopBackend, &jail, Command::new("true")).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn default_backend_returns_something() {
        let b = default_backend();
        assert!(!b.name().is_empty());
    }

    #[test]
    fn default_backend_is_cached() {
        // OnceLock guarantees the same Arc on every call.
        let a = default_backend();
        let b = default_backend();
        assert!(Arc::ptr_eq(&a, &b));
    }

    #[test]
    #[cfg_attr(windows, ignore = "Windows sandboxing not available in test context")]
    fn spawn_uses_default_backend() {
        let dir = std::env::temp_dir();
        let jail = Jail::new(&dir, "default-spawn");
        let cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit"]);
            c
        } else {
            Command::new("true")
        };
        // Must succeed via whichever platform backend is detected (or
        // noop). The point of the test is that we go through the public
        // `spawn` entry rather than `spawn_with`.
        let mut child = spawn(&jail, cmd).expect("spawn");
        let _ = child.wait().expect("wait");
    }

    #[test]
    fn canonicalize_or_log_does_not_panic_on_missing() {
        // The lossy helper is supposed to log + continue rather than
        // propagate. Verify it doesn't panic for the missing-root case.
        let mut jail = Jail::new("/no/such/place", "lossy");
        jail.canonicalize_or_log();
        // root stays as-is on failure.
        assert_eq!(jail.root, std::path::PathBuf::from("/no/such/place"));
    }

    #[test]
    fn noop_backend_metadata() {
        assert_eq!(NoopBackend.name(), "noop");
        assert!(NoopBackend.is_available());
    }
}
