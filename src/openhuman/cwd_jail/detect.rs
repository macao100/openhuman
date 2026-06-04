//! Platform auto-detection. Picks the strongest available backend.
//!
//! Detection order per platform:
//!
//! | Platform | Priority                        | Mechanism                     |
//! |----------|---------------------------------|------------------------------|
//! | Windows  | 1. RestrictedToken (D-06)      | CreateRestrictedToken + Low IL |
//! | Windows  | 2. AppContainer (D-07)         | CreateAppContainerProfile    |
//! | Linux    | 1. Landlock                    | LSM (kernel 5.13+)           |
//! | macOS    | 1. Seatbelt                    | sandbox-exec                  |
//! | Any      | ❌ Noop (fail-closed)          | Returns error on spawn       |

use std::sync::Arc;

use super::jail::JailBackend;
use super::noop::NoopBackend;

pub fn pick_backend() -> Arc<dyn JailBackend> {
    #[cfg(target_os = "windows")]
    {
        // D-06: Restricted Token is the primary backend.
        let rt = super::windows_restricted::RestrictedTokenBackend::new();
        if rt.is_available() {
            log::info!("[cwd_jail] backend=restricted_token");
            return Arc::new(rt);
        }
        // D-07: AppContainer is the fallback.
        let ac = super::windows::AppContainerBackend::new();
        if ac.is_available() {
            log::info!("[cwd_jail] backend=appcontainer");
            return Arc::new(ac);
        }
    }
    #[cfg(target_os = "linux")]
    {
        let lb = super::linux::LandlockBackend::new();
        if lb.is_available() {
            log::info!("[cwd_jail] backend=landlock");
            return Arc::new(lb);
        }
    }
    #[cfg(target_os = "macos")]
    {
        let sb = super::macos::SeatbeltBackend::new();
        if sb.is_available() {
            log::info!("[cwd_jail] backend=seatbelt");
            return Arc::new(sb);
        }
    }
    // D-08: No silent degradation. NoopBackend will fail-closed on spawn.
    log::error!(
        "[cwd_jail] CRITICAL: no sandbox backend available on this platform! \
         All jailed process spawns will FAIL closed. \
         Install a supported backend or set DADOU_SANDBOX_PERMISSIVE=1 to override."
    );
    Arc::new(NoopBackend)
}
