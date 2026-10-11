//! The speech-engine service.
//!
//! Owns the one [`SpeechEngine`] instance and the active model. Decoding is a
//! process-wide bottleneck — whisper.cpp spawns threads per decode — so a single
//! engine behind a lock is the honest shape, and [`SpeechEngine::transcribe`]
//! already takes `&self`.
//!
//! Model changes take effect without a restart: `apply_settings` re-prepares
//! when the model, language or thread count changed, and the next take uses the
//! new model. That is the behaviour §17 requires ("changing the model in
//! settings must affect the runtime").

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use airprompt_core::error::{Error, Result};
use airprompt_core::models::ModelManager;
use airprompt_core::settings::{Settings, SpeechEngineId};
use airprompt_core::transcription::{EngineOptions, SpeechEngine};
use airprompt_core::whisper::WhisperCliEngine;

/// The engines this build actually ships.
///
/// `AppleSpeech` and `Parakeet` remain selectable in settings but are reported
/// unavailable, with the reason the settings screen shows. Wiring Apple's
/// Speech framework through objc2 is real work and was not done for this
/// release; saying so beats a menu item that fails at record time.
pub struct SpeechService {
    engine: Box<dyn SpeechEngine>,
    state: Mutex<SpeechState>,
}

#[derive(Debug, Clone, Default)]
struct SpeechState {
    model_path: Option<PathBuf>,
    model_id: String,
    options: EngineOptions,
}

impl SpeechState {
    fn matches(&self, model_path: &Path, options: &EngineOptions) -> bool {
        self.model_path.as_deref() == Some(model_path)
            && self.options.language == options.language
            && self.options.threads == options.threads
            && self.options.use_gpu == options.use_gpu
    }
}

impl SpeechService {
    /// Build the service around `binary` (a `whisper-cli` executable).
    ///
    /// The engine is created even when the binary is missing: `is_available`
    /// reports the fact and every entry point surfaces the same message,
    /// instead of the service silently existing in two shapes.
    pub fn new(binary: PathBuf, cache_dir: PathBuf) -> Self {
        SpeechService {
            engine: Box::new(WhisperCliEngine::new(binary, cache_dir)),
            state: Mutex::new(SpeechState::default()),
        }
    }

    /// The engine id this build runs.
    pub fn engine_id(&self) -> &'static str {
        self.engine.id()
    }

    /// True when the engine backend is usable at all.
    pub fn is_available(&self) -> bool {
        self.engine.is_available()
    }

    /// Why the engine is unusable, for settings and diagnostics.
    pub fn unavailable_reason(&self) -> Option<String> {
        self.engine.unavailable_reason()
    }

    /// True once a model is loaded and a take can be transcribed.
    pub fn is_ready(&self) -> bool {
        self.engine.is_ready()
    }

    /// The currently loaded model id, for diagnostics.
    pub fn loaded_model_id(&self) -> Option<String> {
        self.state.lock().ok().and_then(|s| {
            s.model_id
                .is_empty()
                .then_none()
                .or_else(|| Some(s.model_id.clone()))
        })
    }

    /// Adopt settings. Loads or switches the model when needed.
    ///
    /// Loading a model decodes one 250 ms probe, so this is deliberately
    /// synchronous and called from setup paths, never from a hotkey handler.
    pub fn apply_settings(
        &self,
        settings: &Settings,
        manager: &ModelManager,
    ) -> Result<Option<String>> {
        let speech = &settings.speech;

        // Only the bundled engine exists in this build; anything else is an
        // explicit unavailability, not a silent fallback to whisper.
        if speech.engine != SpeechEngineId::WhisperCpp {
            return Err(Error::Unsupported(match speech.engine {
                SpeechEngineId::AppleSpeech => {
                    "Apple Speech is not available in this build".to_string()
                }
                SpeechEngineId::Parakeet => "Parakeet is not available in this build".to_string(),
                SpeechEngineId::WhisperCpp => unreachable!(),
            }));
        }
        if !self.is_available() {
            return Err(Error::Unsupported(
                self.unavailable_reason()
                    .unwrap_or_else(|| "The speech engine is not installed".to_string()),
            ));
        }

        // Empty means "not chosen yet": skip loading rather than guessing a
        // model, and let start_dictation report NoModelInstalled with its
        // actionable message.
        if speech.model_id.trim().is_empty() {
            return Ok(None);
        }

        let model_path = manager.path_for(speech.model_id.trim())?;
        if !model_path.is_file() {
            // An id that is not on disk must not poison the engine state.
            return Err(Error::NoModelInstalled);
        }

        let options = EngineOptions {
            language: speech.language.clone(),
            threads: speech.threads,
            use_gpu: speech.use_gpu,
        };

        {
            let state = self
                .state
                .lock()
                .map_err(|_| Error::ModelLoad("speech engine state is unavailable".into()))?;
            if state.matches(&model_path, &options) && self.engine.is_ready() {
                return Ok(None);
            }
        }

        self.engine.prepare(&model_path, &options)?;

        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::ModelLoad("speech engine state is unavailable".into()))?;
        let previous = state.model_id.clone();
        state.model_path = Some(model_path);
        state.model_id = speech.model_id.trim().to_string();
        state.options = options;

        if previous.is_empty() || previous != state.model_id {
            Ok(Some(state.model_id.clone()))
        } else {
            Ok(None)
        }
    }

    /// Transcribe a take. The samples must be 16 kHz mono (spec §8).
    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        if samples.len() < airprompt_core::transcription::MIN_AUDIO_SAMPLES {
            // Too short to decode; an empty result is the honest answer.
            return Ok(String::new());
        }
        airprompt_core::transcription::ensure_audio_is_decodable(samples)?;
        self.engine.transcribe(samples)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn service() -> (SpeechService, TempDir) {
        let temp = TempDir::new().unwrap();
        let service = SpeechService::new(
            temp.path().join("missing-whisper-cli"),
            temp.path().join("cache"),
        );
        (service, temp)
    }

    #[test]
    fn a_missing_sidecar_is_reported_not_hidden() {
        let (service, _temp) = service();
        assert!(!service.is_available());
        assert!(service.unavailable_reason().is_some());
        assert!(!service.is_ready());
    }

    #[test]
    fn prepare_without_a_model_reports_no_model_installed() {
        let (service, temp) = service();
        let manager = ModelManager::new(temp.path().join("models"));
        let mut settings = Settings::default();
        settings.speech.model_id = "whisper-tiny-en".into();

        let error = service
            .apply_settings(&settings, &manager)
            .err()
            .expect("missing model file must fail");
        assert_eq!(error.code(), "no_model");
        assert!(!service.is_ready());
    }

    #[test]
    fn an_empty_model_id_is_not_an_error() {
        let (service, temp) = service();
        let manager = ModelManager::new(temp.path().join("models"));
        let settings = Settings::default();
        assert_eq!(service.apply_settings(&settings, &manager).unwrap(), None);
    }

    #[test]
    fn other_engines_are_reported_unavailable_not_silently_swapped() {
        let (service, temp) = service();
        let manager = ModelManager::new(temp.path().join("models"));
        let mut settings = Settings::default();
        settings.speech.engine = SpeechEngineId::AppleSpeech;

        let error = service
            .apply_settings(&settings, &manager)
            .err()
            .expect("unwired engine must fail");
        assert_eq!(error.code(), "unsupported");
    }

    #[test]
    fn short_takes_return_empty_without_touching_the_engine() {
        let (service, _temp) = service();
        // 100 ms of samples: below MIN_AUDIO_SAMPLES (250 ms).
        let samples = vec![0.0f32; 1_600];
        assert_eq!(service.transcribe(&samples).unwrap(), "");
    }

    #[test]
    fn non_finite_audio_is_refused_before_a_decode() {
        let (service, _temp) = service();
        let mut samples = vec![0.0f32; 16_000];
        samples[10] = f32::NAN;
        assert_eq!(
            service.transcribe(&samples).unwrap_err().code(),
            "transcription"
        );
    }
}
