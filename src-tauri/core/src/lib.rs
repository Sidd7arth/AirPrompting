//! AirPrompt core — everything that is true on both macOS and Windows.
//!
//! This crate has no GUI and no Tauri dependency, so its behaviour is testable
//! with `cargo test -p airprompt-core` on any machine. Anything that touches the
//! operating system arrives through the traits in [`platform`].
//!
//! [`identity`] is the single definition of the product's own names — every
//! other module refers to it rather than repeating a literal.

pub mod audio;
pub mod commands;
pub mod enhancement;
pub mod error;
pub mod formatting;
pub mod identity;
pub mod migration;
pub mod models;
pub mod platform;
pub mod security;
pub mod settings;
pub mod storage;
pub mod transcription;
pub mod vad;
pub mod whisper;

pub use error::{Error, Result};

/// Current wall-clock time in Unix milliseconds.
///
/// Millisecond precision is all history and retention need, and it keeps the
/// core free of a date/time dependency.
pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
