//! Cross-platform directory-jail facade.
//!
//! A [`Jail`] describes *what* the agent is allowed to see; a [`JailBackend`]
//! enforces it on a specific OS. Callers only interact with [`Jail`] and the
//! top-level [`crate::openhuman::cwd_jail::spawn`] function — they
//! never pick a backend by name.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};

#[cfg(target_os = "windows")]
use std::os::windows::io::{AsRawHandle, OwnedHandle};

/// Declarative description of a directory jail.
///
/// One `root` (read/write), zero or more `read_only` paths, an optional
/// allow-list of extra paths the child *may* read, and a network toggle.
/// Backends translate this into Landlock rules, a Seatbelt profile, or an
/// AppContainer ACL.
#[derive(Debug, Clone)]
pub struct Jail {
    /// Primary read/write root. The child cannot escape this directory for
    /// writes. Must be an existing, canonicalizable directory.
    pub root: PathBuf,
    /// Extra paths the child may read (e.g. `/usr/lib`, the runtime-node
    /// install). Writes are still denied.
    pub read_only: Vec<PathBuf>,
    /// Allow outbound network. Most agent tools need this; some risky tools
    /// (untrusted code execution) should disable it.
    pub allow_net: bool,
    /// Allow the child to spawn further child processes. AppContainer and
    /// Seatbelt can deny this; Landlock cannot.
    pub allow_subprocess: bool,
    /// Free-form label used by audit logs and (on Windows) as the basis for
    /// the AppContainer profile name. Keep it short and ASCII.
    pub label: String,
}

impl Jail {
    /// Convenience: read/write jail rooted at `root` with networking enabled
    /// and no additional read-only mounts.
    pub fn new(root: impl AsRef<Path>, label: impl Into<String>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            read_only: Vec::new(),
            allow_net: true,
            allow_subprocess: true,
            label: label.into(),
        }
    }

    pub fn add_read_only(mut self, path: impl AsRef<Path>) -> Self {
        self.read_only.push(path.as_ref().to_path_buf());
        self
    }

    pub fn deny_net(mut self) -> Self {
        self.allow_net = false;
        self
    }

    pub fn deny_subprocess(mut self) -> Self {
        self.allow_subprocess = false;
        self
    }

    /// Canonicalize `root` and `read_only` so backends never see `..` or
    /// symlink trickery. Returns an error if `root` does not exist.
    pub fn canonicalize(&mut self) -> std::io::Result<()> {
        self.root = self.root.canonicalize()?;
        for p in self.read_only.iter_mut() {
            if let Ok(c) = p.canonicalize() {
                *p = c;
            }
        }
        Ok(())
    }
}

// ── JailedChild ─────────────────────────────────────────────────────

/// A process handle returned by a [`JailBackend`].
///
/// Wraps either a `std::process::Child` (for backends that can return one
/// directly via `Command::spawn`) or a custom Windows process handle
/// (for backends that manage their own process lifecycle via raw Win32 APIs).
///
/// Backends that use `Command::spawn` return `JailedChild::Std(child)`.
/// Windows backends (RestrictedToken, AppContainer) return
/// `JailedChild::Custom { handle, pid }`.
#[derive(Debug)]
pub enum JailedChild {
    /// Standard child process returned by `std::process::Command::spawn`.
    Std(Child),
    /// Custom process handle on Windows (RestrictedToken / AppContainer).
    /// Owns the process handle and remembers the PID.
    #[cfg(target_os = "windows")]
    Custom {
        /// Owned process handle. Closed on drop (after waiting for the
        /// process to exit to avoid zombies).
        handle: OwnedHandle,
        /// Process ID.
        pid: u32,
    },
}

impl JailedChild {
    /// The process ID of the child.
    pub fn id(&self) -> u32 {
        match self {
            JailedChild::Std(c) => c.id(),
            #[cfg(target_os = "windows")]
            JailedChild::Custom { pid, .. } => *pid,
        }
    }

    /// Wait for the process to exit and return its exit status.
    ///
    /// On the `Custom` variant the handle is not consumed — subsequent
    /// calls to `try_wait` return `Some(status)` immediately.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        match self {
            JailedChild::Std(c) => c.wait(),
            #[cfg(target_os = "windows")]
            JailedChild::Custom { handle, .. } => {
                // SAFETY: handle is a valid process handle from
                // CreateProcessAsUserW / CreateProcessW.
                let status = unsafe { wait_for_process(handle.as_raw_handle())? };
                Ok(status)
            }
        }
    }

    /// Kill the process.
    pub fn kill(&mut self) -> io::Result<()> {
        match self {
            JailedChild::Std(c) => c.kill(),
            #[cfg(target_os = "windows")]
            JailedChild::Custom { handle, .. } => {
                // SAFETY: handle is a valid process handle.
                unsafe { terminate_process(handle.as_raw_handle()) }
            }
        }
    }

    /// Try to wait without blocking. Returns `Ok(None)` if the process is
    /// still running, `Ok(Some(status))` if it has exited.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        match self {
            JailedChild::Std(c) => c.try_wait(),
            #[cfg(target_os = "windows")]
            JailedChild::Custom { handle, .. } => {
                // SAFETY: handle is a valid process handle.
                let status = unsafe { try_wait_for_process(handle.as_raw_handle())? };
                Ok(status)
            }
        }
    }
}

impl Drop for JailedChild {
    fn drop(&mut self) {
        // Std variant: Child's Drop handles waiting + cleanup.
        // Custom variant: wait for the process to exit before closing
        // the handle, preventing a zombie.
        #[cfg(target_os = "windows")]
        if let JailedChild::Custom { handle, .. } = self {
            unsafe {
                let raw = handle.as_raw_handle();
                if !raw.is_null() {
                    // INFINITE wait ensures the process has exited before
                    // the handle is closed, avoiding a zombie.
                    let _ = win::WaitForSingleObject(raw as isize, win::INFINITE);
                }
            }
        }
    }
}

// ── Windows-specific helpers ────────────────────────────────────────
// These are kept in a separate module so the cfg gate is contained.

#[cfg(target_os = "windows")]
mod win {
    use std::io;
    use std::os::windows::process::ExitStatusExt;

    pub const INFINITE: u32 = 0xFFFF_FFFF;
    pub const WAIT_OBJECT_0: u32 = 0;
    pub const WAIT_FAILED: u32 = 0xFFFF_FFFF;

    #[link(name = "kernel32")]
    extern "system" {
        pub fn WaitForSingleObject(hHandle: isize, dwMilliseconds: u32) -> u32;
        pub fn GetExitCodeProcess(hProcess: isize, lpExitCode: *mut u32) -> i32;
        pub fn TerminateProcess(hProcess: isize, uExitCode: u32) -> i32;
    }

    /// Wait until the process exits and return its exit status.
    ///
    /// # Safety
    ///
    /// `handle` must be a valid handle to a process, opened with
    /// `PROCESS_QUERY_INFORMATION | SYNCHRONIZE`.
    pub unsafe fn wait_for_process(
        handle: *mut core::ffi::c_void,
    ) -> io::Result<std::process::ExitStatus> {
        let rc = WaitForSingleObject(handle as isize, INFINITE);
        if rc == WAIT_FAILED {
            return Err(io::Error::last_os_error());
        }
        let mut exit_code: u32 = 0;
        if GetExitCodeProcess(handle as isize, &mut exit_code) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(std::process::ExitStatus::from_raw(exit_code))
    }

    /// Try to wait without blocking.
    ///
    /// # Safety
    ///
    /// `handle` must be a valid process handle.
    pub unsafe fn try_wait_for_process(
        handle: *mut core::ffi::c_void,
    ) -> io::Result<Option<std::process::ExitStatus>> {
        let rc = WaitForSingleObject(handle as isize, 0); // 0 = no wait
        if rc == WAIT_FAILED {
            return Err(io::Error::last_os_error());
        }
        if rc != WAIT_OBJECT_0 {
            return Ok(None);
        }
        let mut exit_code: u32 = 0;
        if GetExitCodeProcess(handle as isize, &mut exit_code) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(std::process::ExitStatus::from_raw(exit_code)))
    }

    /// Kill the process.
    ///
    /// # Safety
    ///
    /// `handle` must be a valid process handle with `PROCESS_TERMINATE`
    /// access.
    pub unsafe fn terminate_process(handle: *mut core::ffi::c_void) -> io::Result<()> {
        if TerminateProcess(handle as isize, 1) == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
use win::*;

// ── JailBackend trait ───────────────────────────────────────────────

/// OS-specific enforcement of a [`Jail`].
///
/// We model spawning rather than `Command` mutation because Windows
/// AppContainer requires custom `CreateProcess` flags that `std`'s
/// `Command::spawn` does not expose.
pub trait JailBackend: Send + Sync {
    /// Stable identifier, used in logs / audit ("landlock", "seatbelt",
    /// "appcontroller", "restricted_token", "noop").
    fn name(&self) -> &'static str;

    /// Whether the backend can actually enforce the jail in this process /
    /// on this kernel build. Auto-detection consults this before returning
    /// a backend.
    fn is_available(&self) -> bool;

    /// Spawn `cmd` under the jail described by `jail`. Backends own how the
    /// jail is materialized (Landlock ruleset, sandbox-exec wrapper,
    /// AppContainer profile + restricted token).
    ///
    /// Returns a [`JailedChild`] that wraps either a `std::process::Child`
    /// or a platform-specific process handle.
    fn spawn(&self, jail: &Jail, cmd: Command) -> io::Result<JailedChild>;
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_permissive() {
        let j = Jail::new("/tmp", "x");
        assert!(j.allow_net);
        assert!(j.allow_subprocess);
        assert_eq!(j.label, "x");
        assert!(j.read_only.is_empty());
    }

    #[test]
    fn deny_net_is_idempotent() {
        let j = Jail::new("/tmp", "x").deny_net().deny_net();
        assert!(!j.allow_net);
    }

    #[test]
    fn deny_subprocess_is_idempotent() {
        let j = Jail::new("/tmp", "x").deny_subprocess().deny_subprocess();
        assert!(!j.allow_subprocess);
    }

    #[test]
    fn add_read_only_appends_in_order() {
        let j = Jail::new("/tmp", "x")
            .add_read_only("/a")
            .add_read_only("/b")
            .add_read_only("/c");
        assert_eq!(j.read_only.len(), 3);
        assert_eq!(j.read_only[0], PathBuf::from("/a"));
        assert_eq!(j.read_only[2], PathBuf::from("/c"));
    }

    #[test]
    fn canonicalize_resolves_real_path() {
        let dir = std::env::temp_dir();
        let mut j = Jail::new(&dir, "x");
        j.canonicalize().unwrap();
        // After canonicalize, root has no `..` and resolves to a real path.
        assert!(j.root.is_absolute());
        assert!(j.root.exists());
    }

    #[test]
    fn canonicalize_swallows_missing_read_only() {
        // read_only entries that don't exist are silently dropped from
        // canonicalization (they stay as-is). Verify no panic.
        let dir = std::env::temp_dir();
        let mut j = Jail::new(&dir, "x").add_read_only("/this/never/existed");
        j.canonicalize().unwrap();
        assert_eq!(j.read_only.len(), 1);
    }

    #[test]
    fn canonicalize_errors_on_missing_root() {
        let mut j = Jail::new("/no/such/root/here", "x");
        let err = j.canonicalize().unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[test]
    fn jailed_child_std_id_and_wait() {
        let mut child = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", "exit", "42"]);
            JailedChild::Std(c.spawn().expect("spawn cmd"))
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg("exit 42");
            JailedChild::Std(c.spawn().expect("spawn sh"))
        };
        let id = child.id();
        assert!(id > 0);
        let status = child.wait().expect("wait");
        assert_eq!(status.code(), Some(42));
    }

    #[test]
    fn jailed_child_std_kill() {
        let mut child = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.args(["/C", "ping", "127.0.0.1", "-n", "100"]);
            JailedChild::Std(c.spawn().expect("spawn cmd"))
        } else {
            let mut c = Command::new("sleep");
            c.arg("100");
            JailedChild::Std(c.spawn().expect("spawn sleep"))
        };
        child.kill().expect("kill");
        let status = child.wait().expect("wait");
        assert!(!status.success());
    }

    #[test]
    fn jailed_child_std_try_wait() {
        let mut child = {
            let mut c = Command::new(if cfg!(target_os = "windows") {
                "cmd"
            } else {
                "true"
            });
            if cfg!(target_os = "windows") {
                c.args(["/C", "exit"]);
            }
            JailedChild::Std(c.spawn().expect("spawn"))
        };
        // After wait(), try_wait() should return Some immediately.
        let _ = child.wait().expect("wait");
        let result = child.try_wait().expect("try_wait");
        assert!(result.is_some());
    }
}
