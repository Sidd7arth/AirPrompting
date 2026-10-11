//! The product's identity, defined once (spec §19).
//!
//! Every name AirPrompt uses to identify itself lives here and nowhere else:
//! the display name, the bundle/application identifier, the data and cache
//! directory names, the database file, the credential-store service, the log
//! namespace, the event channel carrying core state to the UI, and the
//! environment variables the product reads.
//!
//! The point of this module is that a future rename is one edit, not a
//! repository-wide search. Nothing that is genuine third-party identity belongs
//! here: `whisper.cpp`, `FluidAudio`, SQLite and Tauri are the names of other
//! projects, and they keep them (spec §26).

/// Product name, as shown to the user in every surface.
pub const APP_NAME: &str = "AirPrompt";

/// Reverse-DNS identifier. macOS `CFBundleIdentifier`, the Tauri application
/// identifier and the Windows application identity all use this one value.
pub const APP_IDENTIFIER: &str = "com.airprompt.app";

/// Directory under the per-user application data directory
/// (`~/Library/Application Support` on macOS, `%APPDATA%` on Windows).
pub const DATA_DIR_NAME: &str = "AirPrompt";

/// Directory under the per-user cache directory, which the OS is free to purge.
pub const CACHE_DIR_NAME: &str = "com.airprompt.app";

/// The single SQLite database file. One database, not one per feature.
pub const DATABASE_FILE_NAME: &str = "airprompt.sqlite3";

/// Service name for provider API keys in the OS credential store.
pub const SECRET_SERVICE: &str = "com.airprompt.app.provider-api-keys";

/// Log namespace and diagnostic subsystem label.
pub const LOG_NAMESPACE: &str = "airprompt";

/// Channel carrying core events to the UI. The TypeScript side mirrors this in
/// `src/identity.ts`; the two must stay equal.
pub const EVENT_CHANNEL: &str = "airprompt://event";

/// Product-owned file-name prefix for generated artefacts (exports, backups,
/// audio captures), e.g. `AirPrompt_Export_2026-10-07.txt`.
pub const FILE_PREFIX: &str = "AirPrompt";

/// Explicit path to the whisper.cpp CLI, for development.
pub const ENV_WHISPER_BIN: &str = "AIRPROMPT_WHISPER_BIN";

/// Where downloaded speech models live, overriding the per-user default.
pub const ENV_MODELS_DIR: &str = "AIRPROMPT_MODELS_DIR";

/// Prefix for the product's own environment variables.
pub const ENV_PREFIX: &str = "AIRPROMPT_";

/* ----------------------------------------------------------- migration --- */

// The constants below are *migration compatibility identifiers* (spec §33,
// §51). They exist so that a user upgrading from an earlier identity keeps
// their data. They are read-only: nothing in AirPrompt ever writes data under
// one of these names. They are the only place in the product where the earlier
// names appear, and each is justified in BRANDING_AUDIT.md.

/// Directory names earlier builds may have used under the application data
/// directory. Ordered most-recent first, so the newest install wins when a
/// machine has more than one.
pub const LEGACY_DATA_DIR_NAMES: &[&str] = &[
    "Jarvis",
    "com.jarvis.app",
    "FluidVoice",
    "com.fluidvoice.app",
];

/// Database file names earlier builds may have used. A file with one of these
/// names is migrated *to* [`DATABASE_FILE_NAME`], so an upgrade never leaves two
/// competing databases behind.
pub const LEGACY_DATABASE_FILE_NAMES: &[&str] = &["TranscriptionHistory.sqlite3", "jarvis.db"];

/// Credential-store service names earlier builds may have used. The platform
/// adapter reads these once during migration and writes the values into
/// [`SECRET_SERVICE`]; the old entries are left in place.
pub const LEGACY_SECRET_SERVICES: &[&str] = &["com.fluidvoice.provider-api-keys", "com.jarvis.app"];

/// Explicit-path override under the name an earlier build used.
pub const LEGACY_ENV_WHISPER_BIN: &str = "JARVIS_WHISPER_BIN";

/// Model-directory override under the name an earlier build used.
pub const LEGACY_ENV_MODELS_DIR: &str = "JARVIS_MODELS_DIR";

/// Environment variables earlier builds read. They are still honoured as a
/// fallback so a developer's existing shell configuration keeps working.
pub const LEGACY_ENV_VARS: &[&str] = &[LEGACY_ENV_WHISPER_BIN, LEGACY_ENV_MODELS_DIR];

/// Read one of the product's own environment variables, falling back to the
/// name an earlier version used.
///
/// Only the product's own variables are read this way; nothing here touches a
/// third-party variable such as `RUST_LOG` or a provider's `*_API_KEY`.
pub fn env(name: &str, legacy: &[&str]) -> Option<std::ffi::OsString> {
    std::env::var_os(name).or_else(|| legacy.iter().find_map(std::env::var_os))
}

/// Prefix of the browser storage keys earlier builds used for UI preferences.
pub const LEGACY_WEB_STORAGE_PREFIX: &str = "jarvis.";

/// Files whose presence in a candidate directory proves the data belongs to an
/// earlier identity of this product rather than to an unrelated application
/// that happens to have a similar name.
pub const RECOGNISED_DATA_FILES: &[&str] = &[
    "airprompt.sqlite3",
    "jarvis.db",
    "TranscriptionHistory.sqlite3",
    "settings.json",
];
