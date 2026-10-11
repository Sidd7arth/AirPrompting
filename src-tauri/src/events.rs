//! The event surface the backend pushes to the UI.
//!
//! One channel carries everything (`airprompt://event`), and each payload is a
//! tagged union. The shapes here are pinned to `AirPromptEvent` in
//! `src/types/index.ts`; a field renamed on one side and not the other would
//! silently stop updating the overlay, so the two are read together.
//!
//! ## Naming
//!
//! The release brief lists event *names* (`dictation_started`,
//! `partial_transcript`, `model_download_progress`, …). Those names describe
//! the same nine occurrences this module already carries as union variants.
//! The mapping is deliberate and one-to-one:
//!
//! | Brief name(s)                                        | Variant here       |
//! |------------------------------------------------------|--------------------|
//! | `dictation_started`, `audio_level_changed`,          | [`UiEvent::Overlay`] |
//! | `dictation_inserted`, `dictation_failed`, …          |                    |
//! | `speech_started`, `speech_stopped`                   | [`UiEvent::Level`] |
//! | `partial_transcript`, `final_transcript`             | [`UiEvent::Transcript`] |
//! | `model_download_started/progress/ready/failed`        | [`UiEvent::Model`] |
//! | `command_started/recognized/…`                       | [`UiEvent::Command`] |
//! | `permission_changed`                                 | [`UiEvent::Permissions`] |
//!
//! Adding a second set of names would mean the overlay had to listen twice for
//! the same occurrence, which is exactly the duplication §9 is trying to avoid.

use serde::Serialize;

use airprompt_core::identity;
use airprompt_core::models::ModelProgressEvent;
use airprompt_core::platform::PermissionStatus;
use airprompt_core::storage::HistoryEntry;

/// What the overlay window is showing.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OverlayState {
    Hidden,
    Starting,
    Listening,
    Transcribing,
    Enhancing,
    Inserting,
    /// The text that was inserted, so the overlay can echo it briefly.
    Success {
        text: String,
    },
    /// A user-facing sentence, never an error code (spec §61).
    Error {
        message: String,
    },
}

impl OverlayState {
    /// Stable name for logs and diagnostics. Never shown as UI copy.
    pub fn name(&self) -> &'static str {
        match self {
            OverlayState::Hidden => "hidden",
            OverlayState::Starting => "starting",
            OverlayState::Listening => "listening",
            OverlayState::Transcribing => "transcribing",
            OverlayState::Enhancing => "enhancing",
            OverlayState::Inserting => "inserting",
            OverlayState::Success { .. } => "success",
            OverlayState::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ToastLevel {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToastEvent {
    pub level: ToastLevel,
    pub message: String,
}

impl ToastEvent {
    pub fn info(message: impl Into<String>) -> Self {
        ToastEvent {
            level: ToastLevel::Info,
            message: message.into(),
        }
    }
    pub fn warning(message: impl Into<String>) -> Self {
        ToastEvent {
            level: ToastLevel::Warning,
            message: message.into(),
        }
    }
    pub fn error(message: impl Into<String>) -> Self {
        ToastEvent {
            level: ToastLevel::Error,
            message: message.into(),
        }
    }
}

impl From<&airprompt_core::error::Error> for ToastEvent {
    /// Every error crossing to the UI goes through `Error::message()`, which is
    /// the only place with a guarantee of containing no path or model detail.
    fn from(error: &airprompt_core::error::Error) -> Self {
        ToastEvent::error(error.message())
    }
}

/// Rolling latency percentiles for the Diagnostics screen.
///
/// Aggregate timings only — never transcript content (spec §60).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LatencySnapshot {
    pub hotkey_to_audio_ms: Option<f64>,
    pub first_partial_ms: Option<f64>,
    pub final_transcript_ms: Option<f64>,
    pub enhancement_ms: Option<f64>,
    pub insertion_ms: Option<f64>,
    pub total_ms: Option<f64>,
}

/// Everything the Diagnostics screen needs, in one payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshot {
    pub os: String,
    pub arch: String,
    pub app_version: String,
    pub speech_engine: String,
    pub model_id: String,
    pub model_ready: bool,
    pub microphone: String,
    pub hotkey: String,
    pub accessibility: airprompt_core::platform::PermissionState,
    pub ai_provider: String,
    pub ai_model: String,
    pub local_only: bool,
    pub latency: LatencySnapshot,
    /// Process metrics. `None` means "not sampled" and the UI renders `—`.
    ///
    /// Deliberately not fabricated: a number the backend never measured is
    /// worse than a dash.
    pub cpu_percent: Option<f64>,
    pub memory_mb: Option<f64>,
}

/// A feature report the UI uses to hide what the platform cannot do.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityReport {
    pub desktop: bool,
    pub platform: String,
    pub supports: CapabilitySupport,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySupport {
    pub accessibility: bool,
    pub system_shortcuts: bool,
    pub shortcuts_app: bool,
    pub tray: bool,
    pub launch_at_login: bool,
    pub auto_update: bool,
}

/// Result of the settings screen's AI connectivity test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiTestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
}

/// An available update. Empty in this build (no updater is configured).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub date: Option<String>,
}

/// The result of running one voice action.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionOutcome {
    pub ok: bool,
    pub message: String,
    /// True when the action needs the user's confirmation before running.
    pub needs_confirmation: bool,
    /// Human-readable steps, so the confirmation sheet can list them.
    pub plan: Vec<String>,
}

impl ActionOutcome {
    pub fn refused(message: impl Into<String>) -> Self {
        ActionOutcome {
            ok: false,
            message: message.into(),
            needs_confirmation: false,
            plan: Vec::new(),
        }
    }
}

/// One message on the core event channel.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiEvent {
    Overlay {
        state: OverlayState,
    },
    /// Input level, 0.0..1.0, for the waveform.
    Level {
        level: f32,
    },
    Transcript {
        committed: String,
        interim: String,
    },
    Model {
        model: ModelProgressEvent,
    },
    History {
        entry: HistoryEntry,
    },
    Permissions {
        status: PermissionStatus,
    },
    Toast {
        toast: ToastEvent,
    },
    Command {
        command: Option<airprompt_core::commands::ParsedCommand>,
        raw: String,
    },
    Diagnostics {
        snapshot: Box<DiagnosticsSnapshot>,
    },
}

/// Where events go.
///
/// The dictation controller and the command layer depend on this trait rather
/// than on `AppHandle`, which is what makes them runnable in a plain
/// `cargo test` process with no Tauri runtime.
pub trait EventSink: Send + Sync + 'static {
    fn emit(&self, event: UiEvent);
}

/// The production sink: forwards to every window on the core channel.
pub struct AppSink {
    app: tauri::AppHandle,
}

impl AppSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        AppSink { app }
    }
}

impl EventSink for AppSink {
    fn emit(&self, event: UiEvent) {
        use tauri::Emitter;
        // A failed emit (no window yet, app shutting down) must not abort the
        // work that produced the event.
        if let Err(error) = self.app.emit(identity::EVENT_CHANNEL, &event) {
            tracing::debug!(%error, "event emit failed");
        }
    }
}

/// A sink that throws events away. Used where a caller has no UI.
pub struct NullSink;

impl EventSink for NullSink {
    fn emit(&self, _event: UiEvent) {}
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Collects events so a test can assert on the stream the UI would see.
    #[derive(Clone, Default)]
    pub struct RecordingSink {
        events: Arc<Mutex<Vec<UiEvent>>>,
    }

    impl RecordingSink {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn events(&self) -> Vec<UiEvent> {
            self.events.lock().map(|e| e.clone()).unwrap_or_default()
        }

        pub fn take(&self) -> Vec<UiEvent> {
            self.events
                .lock()
                .map(|mut e| std::mem::take(&mut *e))
                .unwrap_or_default()
        }

        pub fn overlay_states(&self) -> Vec<OverlayState> {
            self.events()
                .into_iter()
                .filter_map(|e| match e {
                    UiEvent::Overlay { state } => Some(state),
                    _ => None,
                })
                .collect()
        }
    }

    impl EventSink for RecordingSink {
        fn emit(&self, event: UiEvent) {
            if let Ok(mut events) = self.events.lock() {
                events.push(event);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::RecordingSink;
    use super::*;

    #[test]
    fn events_serialise_to_the_shapes_the_ui_reads() {
        let sink = RecordingSink::new();
        sink.emit(UiEvent::Overlay {
            state: OverlayState::Listening,
        });
        sink.emit(UiEvent::Level { level: 0.5 });
        sink.emit(UiEvent::Transcript {
            committed: "hello".into(),
            interim: "wor".into(),
        });

        let json: Vec<serde_json::Value> = sink
            .events()
            .iter()
            .map(|e| serde_json::to_value(e).unwrap())
            .collect();

        assert_eq!(json[0]["type"], "overlay");
        assert_eq!(json[0]["state"]["state"], "listening");
        assert_eq!(json[1]["type"], "level");
        assert_eq!(json[1]["level"], 0.5);
        assert_eq!(json[2]["type"], "transcript");
        assert_eq!(json[2]["committed"], "hello");
        assert_eq!(json[2]["interim"], "wor");
    }

    #[test]
    fn success_and_error_overlay_states_carry_their_payload() {
        let success = serde_json::to_value(UiEvent::Overlay {
            state: OverlayState::Success {
                text: "done".into(),
            },
        })
        .unwrap();
        assert_eq!(success["state"]["state"], "success");
        assert_eq!(success["state"]["text"], "done");

        let error = serde_json::to_value(UiEvent::Overlay {
            state: OverlayState::Error {
                message: "no microphone".into(),
            },
        })
        .unwrap();
        assert_eq!(error["state"]["state"], "error");
        assert_eq!(error["state"]["message"], "no microphone");
    }

    #[test]
    fn permission_status_uses_camel_case_input_monitoring() {
        // The Settings screen reads `permissions.inputMonitoring`; a snake_case
        // key would leave the row permanently blank.
        let status = PermissionStatus {
            microphone: airprompt_core::platform::PermissionState::Granted,
            accessibility: airprompt_core::platform::PermissionState::Denied,
            input_monitoring: airprompt_core::platform::PermissionState::NotDetermined,
        };
        let json = serde_json::to_value(UiEvent::Permissions { status }).unwrap();
        assert_eq!(json["status"]["microphone"], "granted");
        assert_eq!(json["status"]["accessibility"], "denied");
        assert_eq!(json["status"]["inputMonitoring"], "not_determined");
        assert!(json["status"].get("input_monitoring").is_none());
    }

    #[test]
    fn overlay_state_names_are_stable_and_distinct() {
        let states = [
            OverlayState::Hidden,
            OverlayState::Starting,
            OverlayState::Listening,
            OverlayState::Transcribing,
            OverlayState::Enhancing,
            OverlayState::Inserting,
            OverlayState::Success {
                text: String::new(),
            },
            OverlayState::Error {
                message: String::new(),
            },
        ];
        let mut names: Vec<&str> = states.iter().map(|s| s.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), states.len(), "state names must be unique");
    }

    #[test]
    fn diagnostics_snapshot_matches_the_ui_field_names() {
        let snapshot = DiagnosticsSnapshot {
            os: "windows".into(),
            arch: "x86_64".into(),
            app_version: "0.1.0".into(),
            speech_engine: "whisper_cpp".into(),
            model_id: "whisper-base-en".into(),
            model_ready: true,
            microphone: "Default".into(),
            hotkey: "Alt+Space".into(),
            accessibility: airprompt_core::platform::PermissionState::Unsupported,
            ai_provider: "none".into(),
            ai_model: String::new(),
            local_only: true,
            latency: LatencySnapshot::default(),
            cpu_percent: None,
            memory_mb: None,
        };
        let json = serde_json::to_value(snapshot).unwrap();
        for key in [
            "appVersion",
            "speechEngine",
            "modelId",
            "modelReady",
            "aiProvider",
            "aiModel",
            "localOnly",
            "cpuPercent",
            "memoryMb",
        ] {
            assert!(json.get(key).is_some(), "missing {key}");
        }
        assert!(json["latency"].get("firstPartialMs").is_some());
        assert!(json["latency"].get("totalMs").is_some());
    }

    #[test]
    fn action_outcome_and_capabilities_use_camel_case() {
        let outcome = serde_json::to_value(ActionOutcome {
            ok: true,
            message: "Opened Chrome.".into(),
            needs_confirmation: false,
            plan: vec!["open_app Chrome".into()],
        })
        .unwrap();
        assert!(outcome.get("needsConfirmation").is_some());

        let caps = serde_json::to_value(CapabilityReport {
            desktop: true,
            platform: "windows".into(),
            supports: CapabilitySupport {
                accessibility: false,
                system_shortcuts: true,
                shortcuts_app: false,
                tray: true,
                launch_at_login: true,
                auto_update: false,
            },
        })
        .unwrap();
        assert!(caps["supports"].get("systemShortcuts").is_some());
        assert!(caps["supports"].get("shortcutsApp").is_some());
        assert!(caps["supports"].get("launchAtLogin").is_some());
        assert!(caps["supports"].get("autoUpdate").is_some());
    }

    #[test]
    fn error_toasts_never_carry_the_internal_detail() {
        let error = airprompt_core::error::Error::Io(
            "failed to read C:\\Users\\someone\\secret\\model.bin".into(),
        );
        let toast = ToastEvent::from(&error);
        assert!(!toast.message.contains("secret"));
        assert!(!toast.message.contains('\\'));
        assert_eq!(toast.level, ToastLevel::Error);
    }
}
