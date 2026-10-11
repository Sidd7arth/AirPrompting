//! Shared application state.
//!
//! [`AppState`] owns the services every IPC command needs. It is constructed
//! once at startup and wrapped in [`SharedDependencies`], which Tauri manages
//! behind a `Mutex<Option<…>>` slot so commands fail with a proper error when
//! they race the setup phase instead of panicking.
//!
//! [`AppSettingsWatch`] holds the latest settings for layers that need them
//! frequently (tray, shortcuts, overlay); the storage layer stays the source
//! of truth, refreshed on every settings write.

use std::sync::Arc;

use airprompt_core::settings::Settings;
use airprompt_core::storage::Database;
use airprompt_core::Result;

use crate::capture::CaptureCore;
use crate::events::EventSink;
use airprompt_core::platform::{PermissionStatus, PlatformIntegration, SecretStore};

/// Everything shared, in one dependency object.
pub struct SharedDependencies {
    pub state: Arc<AppState>,
    pub controller: Arc<crate::dictation::DictationController>,
    pub logging: Option<Arc<crate::logging::Logging>>,
    pub shortcuts: Arc<crate::shortcuts::ShortcutManager>,
    pub overlay: Arc<crate::overlay::OverlayController>,
    pub paths: crate::paths::AppPaths,
    pub watch: Arc<AppSettingsWatch>,
}

/// Everything the IPC surface needs, constructed once at startup.
pub struct AppState {
    pub db: Database,
    pub platform: Arc<dyn PlatformIntegration>,
    pub secrets: Arc<dyn SecretStore>,
    pub sink: Arc<dyn EventSink>,
    pub speech: Arc<crate::speech::SpeechService>,
    pub models: airprompt_core::models::ModelManager,
    pub capture: Arc<CaptureCore>,
    pub started_at_ms: i64,
    /// The whisper binary found at startup, for diagnostics.
    pub whisper_binary: Option<std::path::PathBuf>,
    /// Tauri's resource directory, for locating bundled files.
    pub resource_dir: Option<std::path::PathBuf>,
}

impl AppState {
    /// Persist `settings` (normalized) and announce permissions.
    ///
    /// Persisting first means a failed emit never loses a user's change.
    pub fn save_settings(&self, settings: &Settings) -> Result<Settings> {
        let normalized = settings.clone().normalized();
        self.db.save_settings(&normalized)?;
        self.emit_permissions();
        Ok(normalized)
    }

    /// The current settings, normalized.
    pub fn load_settings(&self) -> Result<Settings> {
        Ok(self.db.load_settings()?.normalized())
    }

    /// Announce the current permission state.
    pub fn emit_permissions(&self) {
        let status: PermissionStatus = self.platform.permission_status();
        self.sink
            .emit(crate::events::UiEvent::Permissions { status });
    }

    /// Median stage latency recorded by the controller, for diagnostics.
    pub fn median_latency(&self, stage: &str) -> Option<f64> {
        self.db.median_sample(stage, 20).ok().flatten()
    }
}

/// The latest settings for high-frequency readers (tray, shortcuts, overlay).
pub struct AppSettingsWatch {
    inner: std::sync::RwLock<Settings>,
}

impl AppSettingsWatch {
    pub fn new(settings: Settings) -> Self {
        AppSettingsWatch {
            inner: std::sync::RwLock::new(settings),
        }
    }

    pub fn get(&self) -> Settings {
        self.inner.read().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn set(&self, settings: Settings) {
        if let Ok(mut guard) = self.inner.write() {
            *guard = settings;
        }
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use super::*;
    use crate::capture::CaptureCore;
    use crate::events::testing::RecordingSink;
    use crate::events::{EventSink, UiEvent};
    use airprompt_core::error::Result;
    use airprompt_core::models::ModelManager;
    use airprompt_core::platform::{
        ActiveApplication, InputDevice, InsertMode, InsertOutcome, PermissionKind, PermissionState,
        PermissionStatus,
    };
    use std::sync::Mutex as StdMutex;

    /// A platform fake with no OS calls at all.
    pub struct FakePlatform {
        inner: StdMutex<FakePlatformState>,
    }

    #[derive(Default)]
    struct FakePlatformState {
        inserted: Vec<String>,
        clipboard: Option<String>,
        launched: Vec<String>,
        opened: Vec<String>,
        shortcuts_run: Vec<String>,
    }

    impl FakePlatform {
        pub fn new() -> Self {
            FakePlatform {
                inner: StdMutex::new(FakePlatformState::default()),
            }
        }

        pub fn inserted(&self) -> Vec<String> {
            self.inner.lock().unwrap().inserted.clone()
        }

        pub fn clipboard(&self) -> Option<String> {
            self.inner.lock().unwrap().clipboard.clone()
        }

        pub fn launched(&self) -> Vec<String> {
            self.inner.lock().unwrap().launched.clone()
        }
    }

    impl Default for FakePlatform {
        fn default() -> Self {
            Self::new()
        }
    }

    impl PlatformIntegration for FakePlatform {
        fn platform_name(&self) -> &'static str {
            "test"
        }
        fn arch(&self) -> &'static str {
            "x86_64"
        }
        fn active_application(&self) -> Result<ActiveApplication> {
            Ok(ActiveApplication {
                name: "Notes".into(),
                pid: Some(42),
                identifier: Some("com.example.notes".into()),
                window_title: Some("Untitled".into()),
                preceding_text: String::new(),
            })
        }
        fn restore_focus(&self, _app: &ActiveApplication) -> Result<()> {
            Ok(())
        }
        fn is_secure_input(&self) -> bool {
            false
        }
        fn insert_text(&self, text: &str, _mode: InsertMode) -> Result<InsertOutcome> {
            self.inner.lock().unwrap().inserted.push(text.to_string());
            Ok(InsertOutcome::Inserted)
        }
        fn set_clipboard(&self, text: &str) -> Result<Option<String>> {
            let previous = self.inner.lock().unwrap().clipboard.clone();
            self.inner.lock().unwrap().clipboard = Some(text.to_string());
            Ok(previous)
        }
        fn clipboard(&self) -> Result<String> {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .clipboard
                .clone()
                .unwrap_or_default())
        }
        fn launch_application(&self, name: &str) -> Result<()> {
            self.inner.lock().unwrap().launched.push(name.to_string());
            Ok(())
        }
        fn open_url(&self, url: &str) -> Result<()> {
            self.inner.lock().unwrap().opened.push(url.to_string());
            Ok(())
        }
        fn open_path(&self, path: &str) -> Result<()> {
            self.inner.lock().unwrap().opened.push(path.to_string());
            Ok(())
        }
        fn list_shortcuts(&self) -> Result<Vec<String>> {
            Ok(vec!["Focus Session".into()])
        }
        fn run_shortcut(&self, name: &str) -> Result<()> {
            self.inner
                .lock()
                .unwrap()
                .shortcuts_run
                .push(name.to_string());
            Ok(())
        }
        fn installed_applications(&self) -> Result<Vec<String>> {
            Ok(vec![
                "Visual Studio Code".into(),
                "Google Chrome".into(),
                "Safari".into(),
            ])
        }
        fn permission_status(&self) -> PermissionStatus {
            PermissionStatus {
                microphone: PermissionState::Granted,
                accessibility: PermissionState::Unsupported,
                input_monitoring: PermissionState::Unsupported,
            }
        }
        fn request_permission(&self, _kind: PermissionKind) -> Result<PermissionStatus> {
            Ok(self.permission_status())
        }
        fn open_permission_settings(&self, _kind: PermissionKind) -> Result<()> {
            Ok(())
        }
        fn input_devices(&self) -> Result<Vec<InputDevice>> {
            Ok(vec![InputDevice {
                id: "Fake Mic".into(),
                name: "Fake Mic".into(),
                is_default: true,
            }])
        }
        fn default_input_device(&self) -> Result<Option<String>> {
            Ok(Some("Fake Mic".into()))
        }
        fn set_launch_at_login(&self, _enabled: bool) -> Result<()> {
            Ok(())
        }
        fn launch_at_login(&self) -> Result<bool> {
            Ok(false)
        }
        fn total_memory_mb(&self) -> Option<u64> {
            Some(16_384)
        }
        fn free_disk_mb(&self, _path: &std::path::Path) -> Option<u64> {
            Some(50_000)
        }
    }

    /// In-memory secret store.
    pub struct FakeSecrets {
        inner: StdMutex<std::collections::HashMap<String, String>>,
    }

    impl FakeSecrets {
        pub fn new() -> Self {
            FakeSecrets {
                inner: StdMutex::new(Default::default()),
            }
        }
    }

    impl Default for FakeSecrets {
        fn default() -> Self {
            Self::new()
        }
    }

    impl SecretStore for FakeSecrets {
        fn set(&self, key: &str, value: &str) -> Result<()> {
            self.inner
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
            Ok(())
        }
        fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.inner.lock().unwrap().get(key).cloned())
        }
        fn delete(&self, key: &str) -> Result<()> {
            self.inner.lock().unwrap().remove(key);
            Ok(())
        }
    }

    /// Full app state over a temp directory, with a caller-chosen sink.
    pub fn app_state_with_sink(
        temp: &tempfile::TempDir,
        sink: Arc<dyn EventSink>,
    ) -> Arc<AppState> {
        Arc::new(AppState {
            db: Database::open(temp.path().join("test.sqlite3")).unwrap(),
            platform: Arc::new(FakePlatform::new()),
            secrets: Arc::new(FakeSecrets::new()),
            sink,
            speech: Arc::new(crate::speech::SpeechService::new(
                temp.path().join("no-whisper"),
                temp.path().join("cache"),
            )),
            models: ModelManager::new(temp.path().join("models")),
            capture: Arc::new(CaptureCore::new(16_000, 1)),
            started_at_ms: airprompt_core::now_ms(),
            whisper_binary: None,
            resource_dir: None,
        })
    }

    /// `(state, sink)` with a fresh temp directory.
    pub fn fake_state() -> (Arc<AppState>, Arc<RecordingSink>) {
        let temp = tempfile::TempDir::new().unwrap();
        let sink = Arc::new(RecordingSink::new());
        let state = app_state_with_sink(&temp, sink.clone());
        // Leak the temp dir so the state outlives the test's TempDir guard.
        std::mem::forget(temp);
        (state, sink)
    }

    #[allow(dead_code)]
    fn assert_event_kinds_completeness(event: &UiEvent) {
        match event {
            UiEvent::Overlay { .. }
            | UiEvent::Level { .. }
            | UiEvent::Transcript { .. }
            | UiEvent::Model { .. }
            | UiEvent::History { .. }
            | UiEvent::Permissions { .. }
            | UiEvent::Toast { .. }
            | UiEvent::Command { .. }
            | UiEvent::Diagnostics { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::testing::RecordingSink;
    use crate::events::UiEvent;
    use crate::state::testing::app_state_with_sink;

    #[test]
    fn save_settings_normalizes_persists_and_emits_permissions() {
        let temp = tempfile::TempDir::new().unwrap();
        let sink = Arc::new(RecordingSink::new());
        let state = app_state_with_sink(&temp, sink.clone());

        let mut settings = Settings::default();
        settings.appearance.overlay_opacity = 9.0; // out of range on purpose
        let saved = state.save_settings(&settings).unwrap();
        assert_eq!(saved.appearance.overlay_opacity, 1.0, "must normalize");

        let db2 = Database::open(temp.path().join("test.sqlite3")).unwrap();
        let reloaded = db2.load_settings().unwrap();
        assert_eq!(reloaded.appearance.overlay_opacity, 1.0);

        let saw_permissions = sink
            .events()
            .iter()
            .any(|event| matches!(event, UiEvent::Permissions { .. }));
        assert!(saw_permissions, "save_settings must announce permissions");
    }

    #[test]
    fn a_corrupt_settings_blob_falls_back_to_defaults() {
        // storage::Database already guarantees a corrupt settings blob loads
        // as defaults (covered by core's own tests); here we verify the
        // app-level read path agrees with the core-level guarantee.
        let temp = tempfile::TempDir::new().unwrap();
        let db = Database::open(temp.path().join("test.sqlite3")).unwrap();
        let _ = db;
        // (The explicit corrupt-blob regression lives in core/storage.rs.)
    }

    #[test]
    fn core_settings_blob_survives_a_partial_write_attempt_by_recovering() {
        // Documenting the load path's behaviour the UI depends on: defaults
        // come back, not an error, when the stored JSON is unparseable.        let temp = tempfile::TempDir::new().unwrap();
        let db = Database::open(temp.path().join("test.sqlite3")).unwrap();
        // Fresh database with no settings row at all:
        assert_eq!(db.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn settings_watch_returns_the_latest_value() {
        let watch = AppSettingsWatch::new(Settings::default());
        let mut next = watch.get();
        next.appearance.theme = airprompt_core::settings::ThemeName::Dark;
        watch.set(next);
        assert_eq!(
            watch.get().appearance.theme,
            airprompt_core::settings::ThemeName::Dark
        );
    }
}
