//! Settings model.
//!
//! Defaults follow spec §106: local speech, no cloud, enhancement off, history
//! local-only, overlay on, push-to-talk on. Every struct is `#[serde(default)]`
//! so a settings file written by an older build still loads.

use serde::{Deserialize, Serialize};

/* ------------------------------------------------------------------ enums */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    PushToTalk,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SilencePreset {
    Off,
    Short,
    Balanced,
    Long,
}

impl SilencePreset {
    /// Milliseconds of trailing silence that ends a take. `None` disables VAD.
    pub fn trailing_silence_ms(self) -> Option<u64> {
        match self {
            SilencePreset::Off => None,
            SilencePreset::Short => Some(700),
            SilencePreset::Balanced => Some(1_200),
            SilencePreset::Long => Some(2_000),
        }
    }

    /// Milliseconds of silence that ends an utterance inside a streaming take.
    pub fn utterance_gap_ms(self) -> u64 {
        match self {
            SilencePreset::Off | SilencePreset::Short => 400,
            SilencePreset::Balanced => 600,
            SilencePreset::Long => 900,
        }
    }

    /// RMS threshold below which a frame counts as silence.
    pub fn energy_threshold(self) -> f32 {
        match self {
            SilencePreset::Off => 0.0,
            SilencePreset::Short => 0.004,
            SilencePreset::Balanced => 0.006,
            SilencePreset::Long => 0.009,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeechEngineId {
    /// Bundled whisper.cpp sidecar. The default, available on both platforms.
    WhisperCpp,
    /// Apple Speech (macOS only).
    AppleSpeech,
    /// Parakeet via the FluidAudio CoreML bundle (macOS Apple Silicon only).
    Parakeet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnhancementMode {
    Natural,
    Professional,
    Email,
    Casual,
    Technical,
    Minimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProviderKind {
    None,
    /// A llama.cpp server the user runs locally.
    LlamaCpp,
    /// Ollama's OpenAI-compatible endpoint.
    Ollama,
    /// LM Studio's OpenAI-compatible endpoint.
    LmStudio,
    /// OpenAI, Groq or any other OpenAI-compatible remote endpoint.
    OpenAiCompatible,
}

impl AiProviderKind {
    /// Local providers never leave the machine and stay usable in Local Only mode.
    pub fn is_local(self) -> bool {
        matches!(
            self,
            AiProviderKind::LlamaCpp | AiProviderKind::Ollama | AiProviderKind::LmStudio
        )
    }

    pub fn default_endpoint(self) -> &'static str {
        match self {
            AiProviderKind::None => "",
            AiProviderKind::LlamaCpp => "http://127.0.0.1:8080/v1",
            AiProviderKind::Ollama => "http://127.0.0.1:11434/v1",
            AiProviderKind::LmStudio => "http://127.0.0.1:1234/v1",
            AiProviderKind::OpenAiCompatible => "https://api.openai.com/v1",
        }
    }

    /// Key under which an API key is stored in the OS credential store.
    pub fn secret_key(self) -> &'static str {
        match self {
            AiProviderKind::None => "none",
            AiProviderKind::LlamaCpp => "llama_cpp",
            AiProviderKind::Ollama => "ollama",
            AiProviderKind::LmStudio => "lm_studio",
            AiProviderKind::OpenAiCompatible => "openai_compatible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeName {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayAnchor {
    Top,
    Center,
    Bottom,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/* --------------------------------------------------------------- sections */

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GeneralSettings {
    pub launch_at_login: bool,
    pub start_minimized: bool,
    pub show_tray_icon: bool,
    pub auto_copy_result: bool,
    pub show_model_progress_in_overlay: bool,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        GeneralSettings {
            // Asked during onboarding rather than assumed (spec §106).
            launch_at_login: false,
            start_minimized: true,
            show_tray_icon: true,
            auto_copy_result: false,
            show_model_progress_in_overlay: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DictationSettings {
    pub activation: ActivationMode,
    pub insert_mode: crate::platform::InsertMode,
    pub continuous_dictation: bool,
    pub spoken_punctuation: bool,
    pub trailing_space: bool,
    pub gaav_mode: bool,
    pub restore_focus: bool,
}

impl Default for DictationSettings {
    fn default() -> Self {
        DictationSettings {
            activation: ActivationMode::PushToTalk,
            insert_mode: crate::platform::InsertMode::Paste,
            continuous_dictation: true,
            spoken_punctuation: true,
            trailing_space: true,
            gaav_mode: false,
            restore_focus: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpeechSettings {
    pub engine: SpeechEngineId,
    /// Empty until the user (or onboarding) picks one.
    pub model_id: String,
    pub language: String,
    pub threads: u32,
    pub use_gpu: bool,
    pub vad_enabled: bool,
    pub silence: SilencePreset,
    pub input_device: Option<String>,
}

impl Default for SpeechSettings {
    fn default() -> Self {
        SpeechSettings {
            engine: SpeechEngineId::WhisperCpp,
            model_id: String::new(),
            language: "en".into(),
            threads: 0,
            use_gpu: true,
            vad_enabled: true,
            silence: SilencePreset::Balanced,
            input_device: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EnhancementSettings {
    pub enabled: bool,
    pub provider: AiProviderKind,
    pub endpoint: String,
    pub model: String,
    pub mode: EnhancementMode,
    pub custom_prompt: Option<String>,
    pub remove_fillers: bool,
    pub auto_punctuation: bool,
    pub auto_capitalization: bool,
    pub timeout_secs: u64,
}

impl Default for EnhancementSettings {
    fn default() -> Self {
        EnhancementSettings {
            // Off until a provider is explicitly configured and tested (§106).
            enabled: false,
            provider: AiProviderKind::None,
            endpoint: String::new(),
            model: String::new(),
            mode: EnhancementMode::Natural,
            custom_prompt: None,
            remove_fillers: true,
            auto_punctuation: true,
            auto_capitalization: true,
            timeout_secs: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PrivacySettings {
    pub local_only: bool,
    pub save_history: bool,
    pub history_retention_days: u32,
    pub save_recordings: bool,
    pub telemetry: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        PrivacySettings {
            // Off by default so cloud providers remain opt-in, but dictation is
            // local anyway; toggling this also blocks remote enhancement.
            local_only: false,
            save_history: true,
            history_retention_days: 0,
            save_recordings: false,
            telemetry: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppearanceSettings {
    pub theme: ThemeName,
    pub overlay_enabled: bool,
    pub overlay_position: OverlayAnchor,
    pub overlay_opacity: f32,
    pub show_waveform: bool,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        AppearanceSettings {
            theme: ThemeName::System,
            overlay_enabled: true,
            overlay_position: OverlayAnchor::Bottom,
            overlay_opacity: 0.97,
            show_waveform: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
#[derive(Default)]
pub struct ShortcutBinding {
    pub accelerator: String,
    pub enabled: bool,
}

impl ShortcutBinding {
    pub fn new(accelerator: impl Into<String>) -> Self {
        ShortcutBinding {
            accelerator: accelerator.into(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Shortcuts {
    /// The primary dictation shortcut, honouring `activation` for its behaviour.
    pub dictation: ShortcutBinding,
    /// Voice Actions / command mode.
    pub voice_command: ShortcutBinding,
    /// Optional explicit push-to-talk binding when `activation` is `toggle`.
    pub push_to_talk: ShortcutBinding,
}

impl Default for Shortcuts {
    fn default() -> Self {
        // Platform-appropriate defaults, not hard-coded constants (spec §5).
        let (dictation, command, ptt) = if cfg!(target_os = "macos") {
            ("Alt+Space", "Alt+Shift+Space", "Alt+Space")
        } else {
            ("Alt+Space", "Ctrl+Shift+Space", "Alt+Space")
        };
        Shortcuts {
            dictation: ShortcutBinding::new(dictation),
            voice_command: ShortcutBinding::new(command),
            push_to_talk: ShortcutBinding {
                accelerator: ptt.into(),
                enabled: false,
            },
        }
    }
}

/* -------------------------------------------------------------- aggregate */

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub general: GeneralSettings,
    pub dictation: DictationSettings,
    pub speech: SpeechSettings,
    pub enhancement: EnhancementSettings,
    pub privacy: PrivacySettings,
    pub appearance: AppearanceSettings,
    pub shortcuts: Shortcuts,
    pub log_level: LogLevel,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            general: GeneralSettings::default(),
            dictation: DictationSettings::default(),
            speech: SpeechSettings::default(),
            enhancement: EnhancementSettings::default(),
            privacy: PrivacySettings::default(),
            appearance: AppearanceSettings::default(),
            shortcuts: Shortcuts::default(),
            log_level: LogLevel::Info,
        }
    }
}

impl Settings {
    /// Clamp user input into supported ranges. Never fails; the UI should not be
    /// able to wedge the backend with an out-of-range value.
    pub fn normalized(mut self) -> Self {
        self.appearance.overlay_opacity = self.appearance.overlay_opacity.clamp(0.5, 1.0);
        self.enhancement.timeout_secs = self.enhancement.timeout_secs.clamp(5, 120);
        self.speech.threads = self.speech.threads.min(64);
        self.privacy.history_retention_days = match self.privacy.history_retention_days {
            0 | 1 | 7 | 30 => self.privacy.history_retention_days,
            // Snap anything unexpected to the nearest supported preset.
            n if n < 4 => 1,
            n if n < 15 => 7,
            _ => 30,
        };

        // A provider with no endpoint is unusable; fall back to its default.
        if self.enhancement.provider != AiProviderKind::None
            && self.enhancement.endpoint.trim().is_empty()
        {
            self.enhancement.endpoint = self.enhancement.provider.default_endpoint().into();
        }

        // Enhancement cannot be on without a provider.
        if self.enhancement.provider == AiProviderKind::None {
            self.enhancement.enabled = false;
        }

        // Local Only mode forbids the only provider that leaves the machine.
        if self.privacy.local_only && self.enhancement.provider == AiProviderKind::OpenAiCompatible
        {
            self.enhancement.enabled = false;
        }

        // Trim whitespace that would silently break matching.
        self.speech.language = self.speech.language.trim().to_lowercase();
        if self.speech.language.is_empty() {
            self.speech.language = "en".into();
        }
        self.enhancement.endpoint = self
            .enhancement
            .endpoint
            .trim()
            .trim_end_matches('/')
            .to_string();
        self.enhancement.model = self.enhancement.model.trim().to_string();
        self.enhancement.custom_prompt = self
            .enhancement
            .custom_prompt
            .take()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty());

        self
    }

    /// True when enhancement would send the transcript off the machine.
    /// The UI uses this to show the "Cloud AI Enabled" badge (spec §13).
    pub fn uses_cloud(&self) -> bool {
        self.enhancement.enabled && !self.enhancement.provider.is_local()
    }

    /// Why enhancement is not currently usable, if that is the case.
    pub fn enhancement_blocker(&self) -> Option<&'static str> {
        if !self.enhancement.enabled {
            return Some("AI enhancement is off.");
        }
        if self.enhancement.provider == AiProviderKind::None {
            return Some("Choose an AI provider first.");
        }
        if self.privacy.local_only && !self.enhancement.provider.is_local() {
            return Some("Local Only mode blocks online providers.");
        }
        if self.enhancement.endpoint.is_empty() {
            return Some("Set an API endpoint.");
        }
        None
    }

    /// All binding slots, so the shortcut manager can register them in one pass.
    pub fn enabled_bindings(&self) -> Vec<(&'static str, &ShortcutBinding)> {
        let mut out = Vec::new();
        if self.shortcuts.dictation.enabled && !self.shortcuts.dictation.accelerator.is_empty() {
            out.push(("dictation", &self.shortcuts.dictation));
        }
        if self.shortcuts.voice_command.enabled
            && !self.shortcuts.voice_command.accelerator.is_empty()
        {
            out.push(("voice_command", &self.shortcuts.voice_command));
        }
        if self.shortcuts.push_to_talk.enabled
            && !self.shortcuts.push_to_talk.accelerator.is_empty()
            && self.shortcuts.push_to_talk.accelerator != self.shortcuts.dictation.accelerator
        {
            out.push(("push_to_talk", &self.shortcuts.push_to_talk));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_privacy_first() {
        let s = Settings::default();
        assert!(!s.privacy.local_only || s.enhancement.provider.is_local());
        assert!(
            !s.privacy.save_recordings,
            "raw audio must never be saved by default"
        );
        assert!(!s.privacy.telemetry);
        assert!(!s.enhancement.enabled, "enhancement is opt-in");
        assert_eq!(s.enhancement.provider, AiProviderKind::None);
        assert!(s.privacy.save_history, "history is on and local");
        assert!(s.appearance.overlay_enabled);
        assert_eq!(s.dictation.activation, ActivationMode::PushToTalk);
    }

    #[test]
    fn normalization_clamps_ranges() {
        let mut s = Settings::default();
        s.appearance.overlay_opacity = 5.0;
        s.enhancement.timeout_secs = 10_000;
        s.speech.threads = 1_000;
        s.privacy.history_retention_days = 12;
        let n = s.normalized();
        assert_eq!(n.appearance.overlay_opacity, 1.0);
        assert_eq!(n.enhancement.timeout_secs, 120);
        assert_eq!(n.speech.threads, 64);
        assert_eq!(n.privacy.history_retention_days, 7);
    }

    #[test]
    fn local_only_blocks_cloud_provider() {
        let mut s = Settings::default();
        s.privacy.local_only = true;
        s.enhancement.enabled = true;
        s.enhancement.provider = AiProviderKind::OpenAiCompatible;
        s.enhancement.endpoint = "https://api.openai.com/v1".into();
        let n = s.normalized();
        assert!(!n.enhancement.enabled);
        assert!(!n.uses_cloud());
        assert_eq!(n.enhancement_blocker(), Some("AI enhancement is off."));
    }

    #[test]
    fn local_provider_survives_local_only() {
        let mut s = Settings::default();
        s.privacy.local_only = true;
        s.enhancement.enabled = true;
        s.enhancement.provider = AiProviderKind::Ollama;
        let n = s.normalized();
        assert!(n.enhancement.enabled);
        assert!(!n.uses_cloud());
        assert_eq!(n.enhancement.endpoint, "http://127.0.0.1:11434/v1");
    }

    #[test]
    fn enhancement_off_without_provider() {
        let mut s = Settings::default();
        s.enhancement.enabled = true;
        let n = s.normalized();
        assert!(!n.enhancement.enabled);
        assert_eq!(n.enhancement_blocker(), Some("AI enhancement is off."));
    }

    #[test]
    fn endpoint_trailing_slash_removed() {
        let mut s = Settings::default();
        s.enhancement.provider = AiProviderKind::OpenAiCompatible;
        s.enhancement.endpoint = "  https://example.test/v1/  ".into();
        assert_eq!(
            s.normalized().enhancement.endpoint,
            "https://example.test/v1"
        );
    }

    #[test]
    fn round_trips_through_json() {
        let s = Settings::default();
        let json = serde_json::to_string(&s).unwrap();
        let back: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn older_settings_files_still_load() {
        // An incomplete object must fill in defaults rather than erroring.
        let partial = serde_json::json!({ "general": { "startMinimized": false } });
        let s: Settings = serde_json::from_value(partial).unwrap();
        assert!(!s.general.start_minimized);
        assert!(s.general.show_tray_icon);
        assert_eq!(s.speech.silence, SilencePreset::Balanced);
    }

    #[test]
    fn enabled_bindings_skips_disabled_and_duplicates() {
        let mut s = Settings::default();
        s.shortcuts.push_to_talk.enabled = true;
        s.shortcuts.push_to_talk.accelerator = s.shortcuts.dictation.accelerator.clone();
        let names: Vec<_> = s.enabled_bindings().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["dictation", "voice_command"]);

        s.shortcuts.voice_command.enabled = false;
        let names: Vec<_> = s.enabled_bindings().into_iter().map(|(n, _)| n).collect();
        assert_eq!(names, vec!["dictation"]);
    }

    #[test]
    fn silence_presets_are_ordered() {
        assert!(SilencePreset::Off.trailing_silence_ms().is_none());
        let short = SilencePreset::Short.trailing_silence_ms().unwrap();
        let balanced = SilencePreset::Balanced.trailing_silence_ms().unwrap();
        let long = SilencePreset::Long.trailing_silence_ms().unwrap();
        assert!(short < balanced && balanced < long);
        assert!(SilencePreset::Short.energy_threshold() < SilencePreset::Long.energy_threshold());
    }
}
