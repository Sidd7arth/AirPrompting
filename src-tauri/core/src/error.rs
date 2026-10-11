//! One error type for the whole core.
//!
//! Every variant carries a `message()` that is safe to show in the UI: no paths,
//! no stack traces, no model internals (spec §61).

use std::fmt;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("unable to access microphone: {0}")]
    Microphone(String),

    #[error("speech model failed to load: {0}")]
    ModelLoad(String),

    #[error("speech model is not ready")]
    ModelNotReady,

    #[error("no speech model is installed")]
    NoModelInstalled,

    #[error("speech recognition failed: {0}")]
    Transcription(String),

    #[error("could not insert text: {0}")]
    Insertion(String),

    #[error("the target app closed before insertion")]
    TargetGone,

    #[error("secure input field is active; insertion skipped")]
    SecureInput,

    #[error("missing permission: {0}")]
    Permission(String),

    #[error("shortcut is already in use: {0}")]
    ShortcutConflict(String),

    #[error("clipboard operation failed: {0}")]
    Clipboard(String),

    #[error("AI enhancement unavailable: {0}")]
    Enhancement(String),

    #[error("refused: {0}")]
    Refused(String),

    #[error("network is disabled while Local Only mode is on")]
    LocalOnly,

    #[error("model integrity check failed")]
    Integrity,

    #[error("cancelled")]
    Cancelled,

    #[error("not supported on this platform: {0}")]
    Unsupported(String),

    #[error("{0}")]
    Io(String),

    #[error("{0}")]
    Store(String),
}

impl Error {
    /// A short, actionable sentence for the user.
    pub fn message(&self) -> String {
        match self {
            Error::Microphone(_) => {
                "Unable to access the microphone. Check your system privacy settings.".into()
            }
            Error::ModelLoad(_) => "The speech model failed to load. Try re-downloading it.".into(),
            Error::ModelNotReady => {
                "The speech model is still preparing. Try again shortly.".into()
            }
            Error::NoModelInstalled => {
                "No speech model is installed yet. Choose one in Settings → Speech Models.".into()
            }
            Error::Transcription(_) => "Speech recognition failed. Please try again.".into(),
            Error::Insertion(_) => {
                "Text could not be inserted automatically. It has been copied to the clipboard."
                    .into()
            }
            Error::TargetGone => {
                "The original app is no longer available. The transcript is on your clipboard."
                    .into()
            }
            Error::SecureInput => {
                "A password field was focused. Your transcript is on the clipboard instead.".into()
            }
            Error::Permission(p) => format!("{p} permission is required for this feature."),
            Error::ShortcutConflict(s) => {
                format!("The shortcut {s} is already in use by another app.")
            }
            Error::Clipboard(_) => "The clipboard could not be updated. Please try again.".into(),
            Error::Enhancement(_) => {
                "AI enhancement is unavailable. Using the original transcript.".into()
            }
            Error::Refused(reason) => format!("That action was blocked: {reason}"),
            Error::LocalOnly => "Local Only mode is on, so network features are disabled.".into(),
            Error::Integrity => {
                "The downloaded model failed its integrity check. Please retry.".into()
            }
            Error::Cancelled => "Cancelled.".into(),
            Error::Unsupported(what) => format!("{what} is not available on this platform."),
            Error::Io(_) => "A file operation failed. Check available disk space.".into(),
            Error::Store(_) => "Local storage is unavailable. Restart the app to recover.".into(),
        }
    }

    /// Stable identifier for logs and diagnostics (never shown as UI copy).
    pub fn code(&self) -> &'static str {
        match self {
            Error::Microphone(_) => "microphone",
            Error::ModelLoad(_) => "model_load",
            Error::ModelNotReady => "model_not_ready",
            Error::NoModelInstalled => "no_model",
            Error::Transcription(_) => "transcription",
            Error::Insertion(_) => "insertion",
            Error::TargetGone => "target_gone",
            Error::SecureInput => "secure_input",
            Error::Permission(_) => "permission",
            Error::ShortcutConflict(_) => "shortcut_conflict",
            Error::Clipboard(_) => "clipboard",
            Error::Enhancement(_) => "enhancement",
            Error::Refused(_) => "refused",
            Error::LocalOnly => "local_only",
            Error::Integrity => "integrity",
            Error::Cancelled => "cancelled",
            Error::Unsupported(_) => "unsupported",
            Error::Io(_) => "io",
            Error::Store(_) => "store",
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(value.to_string())
    }
}

impl From<rusqlite::Error> for Error {
    fn from(value: rusqlite::Error) -> Self {
        Error::Store(value.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        Error::Store(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Wrapper so IPC boundaries can serialise errors.
#[derive(Debug, serde::Serialize)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
}

impl From<Error> for ErrorPayload {
    fn from(value: Error) -> Self {
        ErrorPayload {
            code: value.code(),
            message: value.message(),
        }
    }
}

impl fmt::Display for ErrorPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}
