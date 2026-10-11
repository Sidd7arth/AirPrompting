//! Operating-system adapters.
//!
//! Exactly one of the submodules is compiled, and it is the only place in the
//! project where `cfg(target_os)` selects real behaviour. Everything above this
//! crate talks to `airprompt_core::platform::PlatformIntegration`.

use airprompt_core::platform::{PlatformIntegration, SecretStore};
use std::sync::Arc;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(target_os = "macos")]
pub use macos::{MacOsIntegration, MacOsSecretStore};

#[cfg(target_os = "windows")]
pub use windows::{WindowsIntegration, WindowsSecretStore};

/// The adapter for the platform this build targets.
///
/// Constructed once at startup and shared for the process lifetime: hotkey
/// hooks and accessibility handles are expensive to create and must not be
/// duplicated.
pub fn create() -> Arc<dyn PlatformIntegration> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(MacOsIntegration::new())
    }
    #[cfg(target_os = "windows")]
    {
        Arc::new(WindowsIntegration::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        compile_error!(
            "AirPrompt supports macOS and Windows. Add a platform adapter in \
             src-tauri/platform/src before building for another target."
        )
    }
}

/// The OS credential store for API keys.
///
/// Both implementations use `airprompt_core::identity::SECRET_SERVICE` as the
/// service name, so a credential is written under an AirPrompt namespace. A
/// legacy service name listed in `identity::LEGACY_SECRET_SERVICES` is read once
/// during migration and re-written under the AirPrompt name; the old entry is
/// left in place rather than deleted.
pub fn create_secret_store() -> Arc<dyn SecretStore> {
    #[cfg(target_os = "macos")]
    {
        Arc::new(MacOsSecretStore::new())
    }
    #[cfg(target_os = "windows")]
    {
        Arc::new(WindowsSecretStore::new())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        unreachable!()
    }
}
