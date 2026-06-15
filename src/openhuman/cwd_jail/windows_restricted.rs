//! Windows backend: Restricted Tokens + Integrity Levels.
//!
//! Primary sandbox backend for Windows (D-06). Uses `CreateRestrictedToken`
//! to strip dangerous privileges, applies Low Integrity Level via
//! `SetTokenInformation`, and enables process mitigation policies
//! (Code Integrity Guard, Control Flow Guard).
//!
//! ## Why Restricted Token over AppContainer
//!
//! Restricted Tokens are available on every supported Windows version
//! (Vista+) and do not require AppContainer profile creation/cleanup.
//! The combination of:
//!
//! - Disabled sensitive SIDs (Administrators, SYSTEM)
//! - Stripped privileges (debug, backup, restore, impersonate, ...)
//! - Low Integrity Level (cannot write to Medium+ IL objects)
//! - Code Integrity Guard (blocks unsigned DLL injection)
//!
//! approaches AppContainer-level isolation without the profile-management
//! overhead. AppContainer remains as fallback (D-07) for scenarios
//! where stronger file-system sandboxing is needed via per-container SID
//! DACLs.

#![cfg(target_os = "windows")]

use std::ffi::OsStr;
use std::io;
use std::mem;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};
use std::process::Command;
use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, HLOCAL};
use windows_sys::Win32::Security::FreeSid;
use windows_sys::Win32::System::Memory::{LocalAlloc, LPTR};
use windows_sys::Win32::System::Threading::{
    DeleteProcThreadAttributeList, InitializeProcThreadAttributeList, ResumeThread,
    UpdateProcThreadAttribute, CREATE_SUSPENDED, EXTENDED_STARTUPINFO_PRESENT,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, STARTUPINFOEXW, STARTUPINFOW,
};

use super::jail::{Jail, JailBackend, JailedChild};

// ── Import token API via FFI ────────────────────────────────────────
// These functions are in advapi32.dll. Some may not be directly exported
// under the Win32_Security feature gate in windows-sys 0.61, so we
// define raw FFI bindings here to stay independent of the exact API
// surface.
#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(ProcessHandle: isize, DesiredAccess: u32, TokenHandle: *mut isize) -> i32;

    fn CreateRestrictedToken(
        ExistingTokenHandle: isize,
        Flags: u32,
        DisableSidCount: u32,
        SidsToDisable: *const SID_AND_ATTRIBUTES,
        DeletePrivilegeCount: u32,
        PrivilegesToDelete: *const LUID_AND_ATTRIBUTES,
        RestrictedSidCount: u32,
        SidsToRestrict: *const SID_AND_ATTRIBUTES,
        NewTokenHandle: *mut isize,
    ) -> i32;

    fn SetTokenInformation(
        TokenHandle: isize,
        TokenInformationClass: i32,
        TokenInformation: *const core::ffi::c_void,
        TokenInformationLength: u32,
    ) -> i32;

    fn LookupPrivilegeValueW(lpSystemName: *const u16, lpName: *const u16, lpLuid: *mut i64)
        -> i32;

    fn AllocateAndInitializeSid(
        pIdentifierAuthority: *const SID_IDENTIFIER_AUTHORITY,
        nSubAuthorityCount: u8,
        dwSubAuthority0: u32,
        dwSubAuthority1: u32,
        dwSubAuthority2: u32,
        dwSubAuthority3: u32,
        dwSubAuthority4: u32,
        dwSubAuthority5: u32,
        dwSubAuthority6: u32,
        dwSubAuthority7: u32,
        pSid: *mut *mut core::ffi::c_void,
    ) -> i32;

    fn CreateProcessAsUserW(
        hToken: isize,
        lpApplicationName: *const u16,
        lpCommandLine: *mut u16,
        lpProcessAttributes: *const core::ffi::c_void,
        lpThreadAttributes: *const core::ffi::c_void,
        bInheritHandles: i32,
        dwCreationFlags: u32,
        lpEnvironment: *const core::ffi::c_void,
        lpCurrentDirectory: *const u16,
        lpStartupInfo: *mut STARTUPINFOW,
        lpProcessInformation: *mut PROCESS_INFORMATION,
    ) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> isize;
}

// ── Win32 types not exposed by our windows-sys feature set ──────────
// These are manually replicated from the Windows SDK for stability.
// Source: WinNT.h, Security.h.

#[repr(C)]
struct LUID_AND_ATTRIBUTES {
    Luid: i64,
    Attributes: u32,
}

#[repr(C)]
struct SID_IDENTIFIER_AUTHORITY {
    Value: [u8; 6],
}

#[repr(C)]
struct SID_AND_ATTRIBUTES {
    Sid: *mut core::ffi::c_void,
    Attributes: u32,
}

#[repr(C)]
#[allow(dead_code)]
struct TOKEN_MANDATORY_LABEL {
    Label: SID_AND_ATTRIBUTES,
}

// ── Token access masks (WinNT.h) ────────────────────────────────────

const TOKEN_DUPLICATE: u32 = 0x0002;
const TOKEN_ASSIGN_PRIMARY: u32 = 0x0001;
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_ADJUST_DEFAULT: u32 = 0x0080;
const TOKEN_ADJUST_GROUPS: u32 = 0x0040;
const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
const TOKEN_ADJUST_SESSIONID: u32 = 0x0100;

/// Full-rights mask for creating restricted tokens and spawning processes.
const TOKEN_ALL_ACCESS_REQUIRED: u32 = TOKEN_DUPLICATE
    | TOKEN_ASSIGN_PRIMARY
    | TOKEN_QUERY
    | TOKEN_ADJUST_DEFAULT
    | TOKEN_ADJUST_GROUPS
    | TOKEN_ADJUST_PRIVILEGES
    | TOKEN_ADJUST_SESSIONID;

// ── CreateRestrictedToken flags ─────────────────────────────────────

const SANDBOX_INERT: u32 = 0x00000002;

// ── SID identifier authorities ──────────────────────────────────────

const SECURITY_NULL_SID_AUTHORITY: SID_IDENTIFIER_AUTHORITY = SID_IDENTIFIER_AUTHORITY {
    Value: [0, 0, 0, 0, 0, 0],
};

const SECURITY_NT_AUTHORITY: SID_IDENTIFIER_AUTHORITY = SID_IDENTIFIER_AUTHORITY {
    Value: [0, 0, 0, 0, 0, 5],
};

// ── Well-known RIDs (WinNT.h) ──────────────────────────────────────

const SECURITY_LOCAL_SYSTEM_RID: u32 = 0x00000012;
const SECURITY_BUILTIN_DOMAIN_RID: u32 = 0x00000020;
const DOMAIN_ALIAS_RID_ADMINS: u32 = 0x00000220;
const DOMAIN_ALIAS_RID_USERS: u32 = 0x00000221;
const DOMAIN_ALIAS_RID_GUESTS: u32 = 0x00000222;
const DOMAIN_ALIAS_RID_POWER_USERS: u32 = 0x00000223;

// ── Integrity Level RIDs ────────────────────────────────────────────

const SECURITY_MANDATORY_LOW_RID: u32 = 0x00001000;

/// TokenIntegrityLevel value for SetTokenInformation
const TOKEN_INTEGRITY_LEVEL: i32 = 25;

/// SYSTEM_MANDATORY_LABEL_NO_WRITE_UP — the process cannot write to
/// objects with a higher Integrity Level.
const SYSTEM_MANDATORY_LABEL_NO_WRITE_UP: u32 = 0x00000001;

// ── Process mitigation policy constants (ntddk.h) ───────────────────

/// PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY for UpdateProcThreadAttribute
const PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY: usize = 0x00020007;

/// Enable Code Integrity Guard (CIG): blocks injection of unsigned DLLs.
const MITIGATION_SIGNATURE_POLICY_BLOCK_NON_MICROSOFT: u64 = 0x0000_0000_0000_0001;

/// Enable Control Flow Guard (CFG): validates indirect call targets.
const MITIGATION_CONTROL_FLOW_GUARD_ENABLE: u64 = 0x0000_0000_0000_0010;

const MITIGATION_FLAGS: u64 =
    MITIGATION_SIGNATURE_POLICY_BLOCK_NON_MICROSOFT | MITIGATION_CONTROL_FLOW_GUARD_ENABLE;

// ── Privilege names to disable ─────────────────────────────────────

const PRIVILEGES_TO_DISABLE: &[&str] = &[
    "SeCreateTokenPrivilege\0",
    "SeTcbPrivilege\0",
    "SeTakeOwnershipPrivilege\0",
    "SeBackupPrivilege\0",
    "SeRestorePrivilege\0",
    "SeDebugPrivilege\0",
    "SeLoadDriverPrivilege\0",
    "SeCreateGlobalPrivilege\0",
    "SeImpersonatePrivilege\0",
    "SeCreatePermanentPrivilege\0",
    // SeChangeNotifyPrivilege is deliberately excluded — required for
    // basic file system access (bypass traverse checking).
];

// ── SID descriptors to disable ─────────────────────────────────────

struct SidToDisable {
    authority: &'static SID_IDENTIFIER_AUTHORITY,
    sub_authorities: &'static [u32],
}

const SIDS_TO_DISABLE: &[SidToDisable] = &[
    // SYSTEM (NT AUTHORITY\SYSTEM)
    SidToDisable {
        authority: &SECURITY_NT_AUTHORITY,
        sub_authorities: &[SECURITY_LOCAL_SYSTEM_RID],
    },
    // Builtin\Administrators
    SidToDisable {
        authority: &SECURITY_NT_AUTHORITY,
        sub_authorities: &[SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_ADMINS],
    },
    // Builtin\Power Users
    SidToDisable {
        authority: &SECURITY_NT_AUTHORITY,
        sub_authorities: &[SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_POWER_USERS],
    },
    // Builtin\Guests
    SidToDisable {
        authority: &SECURITY_NT_AUTHORITY,
        sub_authorities: &[SECURITY_BUILTIN_DOMAIN_RID, DOMAIN_ALIAS_RID_GUESTS],
    },
];

// ── Backend ─────────────────────────────────────────────────────────

pub struct RestrictedTokenBackend;

impl RestrictedTokenBackend {
    pub fn new() -> Self {
        Self
    }
}

impl JailBackend for RestrictedTokenBackend {
    fn name(&self) -> &'static str {
        "restricted_token"
    }

    fn is_available(&self) -> bool {
        #[cfg(target_os = "windows")]
        {
            // Available on Windows 2000 SP4+ (all supported versions).
            true
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    fn spawn(&self, jail: &Jail, cmd: Command) -> io::Result<JailedChild> {
        // SAFETY: spawn_restricted calls Win32 APIs with validated
        // parameters from the caller-owned jail and command.
        unsafe { spawn_restricted(jail, cmd) }
    }
}

// ── Core implementation ─────────────────────────────────────────────

/// Full restricted-token spawn pipeline.
///
/// Returns `JailedChild::Custom { handle, pid }` on success, where
/// `handle` is an `OwnedHandle` to the child process and `pid` is its
/// process ID. The `JailedChild` provides `wait()`, `kill()`, `try_wait()`
/// via Win32 APIs (`WaitForSingleObject`, `TerminateProcess`, …).
unsafe fn spawn_restricted(_jail: &Jail, _cmd: Command) -> io::Result<JailedChild> {
    // 1. Open current process token.
    let process_token = open_current_process_token()?;

    // 2. Build SID disable list.
    let (sid_entries, sid_handles) = build_sids_to_disable();

    // 3. Build privilege disable list.
    let priv_entries = build_privileges_to_disable();

    // 4. Create restricted token.
    let restricted_token = create_restricted_token(process_token, &sid_entries, &priv_entries)?;

    // 5. Free the individually-allocated SIDs now that
    //    `CreateRestrictedToken` has copied them into the token.
    for sid in sid_handles {
        if !sid.is_null() {
            FreeSid(sid);
        }
    }

    // 6. Apply Low Integrity Level.
    set_low_integrity_level(restricted_token)?;

    // 7. Close the original token (no longer needed).
    CloseHandle(process_token as *mut _);

    // 8. Spawn the child process with the restricted token.
    let (process_handle, thread_handle, pid) = spawn_with_token(restricted_token, _cmd)?;

    // 9. Resume the suspended thread.
    ResumeThread(thread_handle as *mut _);

    // 10. Close the thread handle — we don't need it.
    CloseHandle(thread_handle as *mut _);

    // 11. Wrap the process handle in `JailedChild::Custom`.
    let owned = OwnedHandle::from_raw_handle(process_handle as _);
    Ok(JailedChild::Custom { handle: owned, pid })
}

/// Open the current process token.
unsafe fn open_current_process_token() -> io::Result<isize> {
    let current = GetCurrentProcess();
    let mut token: isize = 0;
    if OpenProcessToken(current, TOKEN_ALL_ACCESS_REQUIRED, &mut token) == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(token)
}

/// Allocate the SIDs to disable and return them as a pair:
/// (entries for CreateRestrictedToken, handles to free afterwards).
unsafe fn build_sids_to_disable() -> (Vec<SID_AND_ATTRIBUTES>, Vec<*mut core::ffi::c_void>) {
    let mut entries = Vec::with_capacity(SIDS_TO_DISABLE.len());
    let mut handles = Vec::with_capacity(SIDS_TO_DISABLE.len());

    for info in SIDS_TO_DISABLE {
        let mut sid: *mut core::ffi::c_void = ptr::null_mut();
        let count = info.sub_authorities.len().min(8) as u8;
        let mut sub = [0u32; 8];
        for (i, sa) in info.sub_authorities.iter().enumerate() {
            if i < 8 {
                sub[i] = *sa;
            }
        }
        let rc = AllocateAndInitializeSid(
            info.authority,
            count,
            sub[0],
            sub[1],
            sub[2],
            sub[3],
            sub[4],
            sub[5],
            sub[6],
            sub[7],
            &mut sid,
        );
        if rc != 0 && !sid.is_null() {
            entries.push(SID_AND_ATTRIBUTES {
                Sid: sid,
                // SE_GROUP_USE_FOR_DENY_ONLY (0x10) — marks the SID as
                // deny-only in the restricted token. The SID is present
                // but cannot be used for access checks.
                Attributes: 0x10,
            });
            handles.push(sid);
        }
    }

    (entries, handles)
}

/// Look up the LUID for each privilege and build the deletion list.
unsafe fn build_privileges_to_disable() -> Vec<LUID_AND_ATTRIBUTES> {
    let mut entries = Vec::with_capacity(PRIVILEGES_TO_DISABLE.len());

    for name in PRIVILEGES_TO_DISABLE {
        let wide: Vec<u16> = OsStr::new(name).encode_wide().collect();
        let mut luid: i64 = 0;
        let rc = LookupPrivilegeValueW(ptr::null(), wide.as_ptr(), &mut luid);
        if rc != 0 {
            entries.push(LUID_AND_ATTRIBUTES {
                Luid: luid,
                // SE_PRIVILEGE_REMOVED: the privilege is removed from
                // the token entirely, not just disabled.
                Attributes: 0x80000000,
            });
        } else {
            log::warn!(
                "[cwd_jail] could not look up privilege {}, skipping",
                name.trim_end_matches('\0'),
            );
        }
    }

    entries
}

/// Create the restricted token by disabling SIDs and removing privileges.
unsafe fn create_restricted_token(
    existing_token: isize,
    sids: &[SID_AND_ATTRIBUTES],
    privs: &[LUID_AND_ATTRIBUTES],
) -> io::Result<isize> {
    let mut new_token: isize = 0;
    let rc = CreateRestrictedToken(
        existing_token,
        SANDBOX_INERT,
        sids.len() as u32,
        sids.as_ptr(),
        privs.len() as u32,
        privs.as_ptr(),
        0,           // RestrictedSidCount
        ptr::null(), // SidsToRestrict
        &mut new_token,
    );

    if rc == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(new_token)
    }
}

/// Apply Low Integrity Level to the token.
///
/// The process will be unable to write to objects at Medium IL or higher
/// (most of the filesystem, other processes). Writes to its own Low IL
/// directory and %TEMP% (Low-compatible) remain permitted.
unsafe fn set_low_integrity_level(token: isize) -> io::Result<()> {
    let mut sid: *mut core::ffi::c_void = ptr::null_mut();
    let rc = AllocateAndInitializeSid(
        &SECURITY_NT_AUTHORITY,
        1,
        SECURITY_MANDATORY_LOW_RID,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        &mut sid,
    );

    if rc == 0 || sid.is_null() {
        return Err(io::Error::last_os_error());
    }

    // Free the SID after SetTokenInformation has consumed it.
    let _guard = SidFreeGuard(sid);

    let label = TOKEN_MANDATORY_LABEL {
        Label: SID_AND_ATTRIBUTES {
            Sid: sid,
            Attributes: SYSTEM_MANDATORY_LABEL_NO_WRITE_UP,
        },
    };

    let rc = SetTokenInformation(
        token,
        TOKEN_INTEGRITY_LEVEL,
        &label as *const _ as *const _,
        mem::size_of::<TOKEN_MANDATORY_LABEL>() as u32,
    );

    if rc == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

/// Spawn the child process under the restricted token with mitigation
/// policies. Returns (process_handle, thread_handle, pid).
unsafe fn spawn_with_token(token: isize, cmd: Command) -> io::Result<(isize, isize, u32)> {
    // Build command line from the std Command.
    let cmdline = build_command_line(&cmd);
    let mut cmdline_w: Vec<u16> = cmdline.encode_utf16().chain(std::iter::once(0)).collect();
    let cwd_w = cmd.get_current_dir().map(|p| {
        p.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    });

    // Build STARTUPINFOEXW with mitigation policy (CIG + CFG).
    let mut attr_list_size: usize = 0;
    InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &mut attr_list_size);

    let attr_buf = LocalAlloc(LPTR, attr_list_size) as LPPROC_THREAD_ATTRIBUTE_LIST;
    if attr_buf.is_null() {
        return Err(io::Error::last_os_error());
    }
    let _attr_guard = AttrListGuard(attr_buf);

    if InitializeProcThreadAttributeList(attr_buf, 1, 0, &mut attr_list_size) == 0 {
        return Err(io::Error::last_os_error());
    }

    let mitigation = MITIGATION_FLAGS;
    if UpdateProcThreadAttribute(
        attr_buf,
        0,
        PROC_THREAD_ATTRIBUTE_MITIGATION_POLICY,
        &mitigation as *const _ as *const _,
        mem::size_of::<u64>(),
        ptr::null_mut(),
        ptr::null_mut(),
    ) == 0
    {
        return Err(io::Error::last_os_error());
    }

    let mut si: STARTUPINFOEXW = mem::zeroed();
    si.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as u32;
    si.lpAttributeList = attr_buf;

    let mut pi: PROCESS_INFORMATION = mem::zeroed();
    let ok = CreateProcessAsUserW(
        token,
        ptr::null(),
        cmdline_w.as_mut_ptr(),
        ptr::null_mut(),
        ptr::null_mut(),
        0, // bInheritHandles = FALSE
        EXTENDED_STARTUPINFO_PRESENT | CREATE_SUSPENDED,
        ptr::null_mut(),
        cwd_w.as_ref().map(|s| s.as_ptr()).unwrap_or(ptr::null()),
        &mut si as *mut _ as *mut STARTUPINFOW,
        &mut pi,
    );

    if ok == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok((pi.hProcess as isize, pi.hThread as isize, pi.dwProcessId))
}

// ── Command-line builder ───────────────────────────────────────────

fn build_command_line(cmd: &Command) -> String {
    let mut out = String::new();
    let prog = cmd.get_program().to_string_lossy().into_owned();
    push_arg(&mut out, &prog);
    for a in cmd.get_args() {
        out.push(' ');
        push_arg(&mut out, &a.to_string_lossy());
    }
    out
}

fn push_arg(out: &mut String, a: &str) {
    let needs_quotes = a.is_empty() || a.contains([' ', '\t', '"']);
    if !needs_quotes {
        out.push_str(a);
        return;
    }
    out.push('"');
    let mut backslashes = 0;
    for c in a.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                for _ in 0..(backslashes * 2 + 1) {
                    out.push('\\');
                }
                out.push('"');
                backslashes = 0;
            }
            _ => {
                for _ in 0..backslashes {
                    out.push('\\');
                }
                backslashes = 0;
                out.push(c);
            }
        }
    }
    for _ in 0..(backslashes * 2) {
        out.push('\\');
    }
    out.push('"');
}

// ── RAII guards ────────────────────────────────────────────────────

struct AttrListGuard(LPPROC_THREAD_ATTRIBUTE_LIST);
impl Drop for AttrListGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                DeleteProcThreadAttributeList(self.0);
                LocalFree(self.0 as HLOCAL);
            }
        }
    }
}

struct SidFreeGuard(*mut core::ffi::c_void);
impl Drop for SidFreeGuard {
    fn drop(&mut self) {
        unsafe {
            if !self.0.is_null() {
                FreeSid(self.0);
            }
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn name_is_restricted_token() {
        let backend = RestrictedTokenBackend::new();
        assert_eq!(backend.name(), "restricted_token");
    }

    #[test]
    fn is_available_returns_true_on_windows() {
        let backend = RestrictedTokenBackend::new();
        #[cfg(target_os = "windows")]
        assert!(backend.is_available());
        #[cfg(not(target_os = "windows"))]
        assert!(!backend.is_available());
    }

    #[test]
    fn spawn_does_not_panic_on_invalid_command() {
        // Even though spawn may fail (invalid command), it must not
        // panic or crash.
        let backend = RestrictedTokenBackend::new();
        let dir = std::env::temp_dir();
        let jail = Jail::new(&dir, "test.rt-safe");
        let cmd = Command::new("__nonexistent_cmd_xyzzy__");
        let _ = backend.spawn(&jail, cmd);
        // No panic == pass.
    }

    #[test]
    fn backend_is_send_and_sync() {
        // Verify the backend satisfies the Send + Sync bounds on
        // JailBackend. This is a compile-time check.
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RestrictedTokenBackend>();
    }
}
