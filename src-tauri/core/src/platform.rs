//! The entire surface through which the shared core touches the operating
//! system (spec §3, §115).
//!
//! The core never contains `#[cfg(target_os = ...)]`. It sees only these traits;
//! `airprompt-platform` supplies the macOS and Windows implementations.

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The application that held focus when dictation started.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveApplication {
    /// Human name, e.g. "Visual Studio Code". May be empty.
    pub name: String,
    /// OS process id when available.
    pub pid: Option<u32>,
    /// Executable path or bundle identifier when available.
    pub identifier: Option<String>,
    /// Title of the focused window, when available.
    pub window_title: Option<String>,
    /// Text immediately preceding the caret, used for continuous dictation.
    /// Empty when the platform cannot read it.
    pub preceding_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Microphone,
    Accessibility,
    InputMonitoring,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionState {
    NotDetermined,
    Granted,
    Denied,
    /// The platform has no such concept (e.g. Accessibility on Windows).
    Unsupported,
}

/// Wire format is camelCase to match `PermissionStatus` in `src/types/index.ts`,
/// whose `inputMonitoring` field the Settings screen reads directly. Renaming
/// the Rust field without renaming the TypeScript one would silently show
/// "Not required" for a permission the user must still grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionStatus {
    pub microphone: PermissionState,
    pub accessibility: PermissionState,
    pub input_monitoring: PermissionState,
}

impl Default for PermissionStatus {
    fn default() -> Self {
        PermissionStatus {
            microphone: PermissionState::NotDetermined,
            accessibility: PermissionState::Unsupported,
            input_monitoring: PermissionState::Unsupported,
        }
    }
}

/// How text reaches the focused application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsertMode {
    /// Clipboard + synthetic paste. Preserves and restores the clipboard.
    Paste,
    /// Synthesise the characters directly (no clipboard involvement).
    Type,
    /// Only place the text on the clipboard; the user pastes.
    ClipboardOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertOutcome {
    Inserted,
    /// Nothing was inserted, but the text is on the clipboard.
    CopiedOnly,
}

/// Audio input device description.
///
/// camelCase on the wire, matching `InputDevice` in `src/services/ipc.ts`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

/// A 16 kHz mono PCM frame in the range [-1.0, 1.0].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioFrame {
    pub peak: f32,
    pub rms: f32,
}

/// Everything the core needs from the host operating system.
pub trait PlatformIntegration: Send + Sync + 'static {
    /// Stable name used in diagnostics: "macos", "windows", "linux".
    fn platform_name(&self) -> &'static str;
    fn arch(&self) -> &'static str;

    /* ----------------------------------------------------------- focus --- */
    /// Capture the currently focused application before recording begins.
    fn active_application(&self) -> Result<ActiveApplication>;
    /// Return focus to a previously captured application.
    fn restore_focus(&self, app: &ActiveApplication) -> Result<()>;
    /// True when the focused control is a password/secure field, where text must
    /// never be injected (spec §99).
    fn is_secure_input(&self) -> bool {
        false
    }

    /* ------------------------------------------------------- text output --- */
    fn insert_text(&self, text: &str, mode: InsertMode) -> Result<InsertOutcome>;
    /// Replace the current clipboard contents, returning the previous value.
    fn set_clipboard(&self, text: &str) -> Result<Option<String>>;
    fn clipboard(&self) -> Result<String>;

    /* ------------------------------------------------------- launching --- */
    fn launch_application(&self, name: &str) -> Result<()>;
    fn open_url(&self, url: &str) -> Result<()>;
    fn open_path(&self, path: &str) -> Result<()>;
    /// Names of runnable shortcuts. macOS: Shortcuts app. Windows: empty.
    fn list_shortcuts(&self) -> Result<Vec<String>>;
    fn run_shortcut(&self, name: &str) -> Result<()>;
    /// Installed application names, for fuzzy command matching.
    fn installed_applications(&self) -> Result<Vec<String>>;

    /* ------------------------------------------------------ permissions --- */
    fn permission_status(&self) -> PermissionStatus;
    /// Prompt if the platform supports prompting; otherwise open settings.
    fn request_permission(&self, kind: PermissionKind) -> Result<PermissionStatus>;
    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()>;

    /* ---------------------------------------------------------- devices --- */
    fn input_devices(&self) -> Result<Vec<InputDevice>>;
    fn default_input_device(&self) -> Result<Option<String>>;

    /* ---------------------------------------------------------- startup --- */
    fn set_launch_at_login(&self, enabled: bool) -> Result<()>;
    fn launch_at_login(&self) -> Result<bool>;

    /* ------------------------------------------------------ system info --- */

    /// Physical memory in MiB, when the platform can report it.
    ///
    /// The model recommender needs this: telling an 8 GB laptop to download
    /// the Medium model would make the machine unusable (spec §10).
    ///
    /// `None` means "not known" and must never be read as zero. There is no
    /// portable, dependency-free way to ask, and inventing a number here would
    /// be worse than admitting ignorance, so the default says so.
    fn total_memory_mb(&self) -> Option<u64> {
        None
    }

    /// Free space in MiB on the volume holding `path`, when known.
    ///
    /// Used to refuse a model download that cannot fit. `None` means unknown.
    fn free_disk_mb(&self, _path: &Path) -> Option<u64> {
        None
    }
}

/// Secure storage for API keys (Keychain / Credential Manager / DPAPI).
pub trait SecretStore: Send + Sync + 'static {
    fn set(&self, key: &str, value: &str) -> Result<()>;
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn delete(&self, key: &str) -> Result<()>;
}
