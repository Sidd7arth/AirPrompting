//! Logging.
//!
//! Two sinks: a rolling file under the app's `logs` directory, which the
//! Diagnostics screen links to, and stderr in debug builds so `cargo run` stays
//! useful.
//!
//! **Never log transcript content.** Log lines carry identifiers, timings and
//! error codes only (spec §60, §94). The level filter can be changed at runtime
//! through [`Logging::set_level`], which is what the Settings screen's log-level
//! control calls.

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::reload;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Registry;

use airprompt_core::settings::LogLevel;

/// Holds the pieces that must outlive `init`.
pub struct Logging {
    reload: reload::Handle<EnvFilter, Registry>,
    /// The non-blocking writer's worker. Dropping it would stop flushing, so it
    /// is owned here for the process lifetime.
    _guard: Option<tracing_appender::non_blocking::WorkerGuard>,
}

impl Logging {
    /// Change the active level without restarting.
    pub fn set_level(&self, level: LogLevel) -> bool {
        let filter = EnvFilter::new(directive(level));
        match self.reload.modify(|f| *f = filter) {
            Ok(()) => {
                tracing::info!(level = level_name(level), "log level changed");
                true
            }
            Err(error) => {
                // The subscriber is gone; nothing to reload.
                eprintln!("airprompt: could not change log level: {error}");
                false
            }
        }
    }
}

/// Initialise logging. Safe to call once per process.
pub fn init(log_dir: &std::path::Path, level: LogLevel) -> Logging {
    let filter = EnvFilter::new(directive(level));
    let (filter_layer, reload) = reload::Layer::new(filter);

    // Daily rotation with a `airprompt.log.YYYY-MM-DD` suffix. The appender
    // creates the directory if it is missing.
    let appender = tracing_appender::rolling::daily(log_dir, "airprompt.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(false)
        .with_target(true);

    let registry = tracing_subscriber::registry()
        .with(filter_layer)
        .with(file_layer);

    // In development, mirror to stderr as well. Release builds are
    // `windows_subsystem = "windows"` with no console to write to.
    #[cfg(debug_assertions)]
    let registry = registry.with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    if let Err(error) = registry.try_init() {
        // A second init (tests, or a plugin that installs its own subscriber)
        // must not abort startup.
        eprintln!("airprompt: logging already initialised ({error})");
    }

    Logging {
        reload,
        _guard: Some(guard),
    }
}

/// The `RUST_LOG`-style directive string for a level.
///
/// The bare level sets the default for every crate; the explicit targets make
/// sure AirPrompt's own crates honour the user's choice even after a noisy
/// dependency would otherwise flood the file. `RUST_LOG`, when present, wins —
/// that is the debugging escape hatch for anyone building from source.
pub fn directive(level: LogLevel) -> String {
    if let Ok(explicit) = std::env::var("RUST_LOG") {
        if !explicit.trim().is_empty() {
            return explicit;
        }
    }
    let name = level_name(level);
    format!("warn,airprompt={name},airprompt_lib={name},airprompt_core={name},airprompt_platform={name}")
}

/// Lowercase name matching the settings wire format.
pub fn level_name(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    }
}

/// Parse the string the UI sends in `set_log_level`.
pub fn parse_level(raw: &str) -> Option<LogLevel> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "error" => Some(LogLevel::Error),
        "warn" | "warning" => Some(LogLevel::Warn),
        "info" => Some(LogLevel::Info),
        "debug" => Some(LogLevel::Debug),
        "trace" => Some(LogLevel::Trace),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_round_trips_through_its_name() {
        for level in [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ] {
            assert_eq!(parse_level(level_name(level)), Some(level));
        }
    }

    #[test]
    fn level_names_match_the_settings_wire_format() {
        // These strings are what the UI sends; changing them breaks the control.
        assert_eq!(level_name(LogLevel::Error), "error");
        assert_eq!(level_name(LogLevel::Info), "info");
        assert_eq!(level_name(LogLevel::Trace), "trace");
    }

    #[test]
    fn unknown_levels_are_rejected_rather_than_defaulted() {
        assert_eq!(parse_level("verbose"), None);
        assert_eq!(parse_level(""), None);
        assert_eq!(parse_level("  "), None);
        // Case and the common alias are tolerated.
        assert_eq!(parse_level("INFO"), Some(LogLevel::Info));
        assert_eq!(parse_level(" warning "), Some(LogLevel::Warn));
    }

    #[test]
    fn directive_quotes_the_airprompt_crates_at_the_chosen_level() {
        let directive = directive(LogLevel::Debug);
        assert!(directive.contains("airprompt=debug"));
        assert!(directive.contains("airprompt_core=debug"));
        // Dependencies stay quiet unless the user asks otherwise.
        assert!(directive.starts_with("warn,"));
    }
}
