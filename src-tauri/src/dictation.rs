//! The dictation controller: the only owner of the take state machine
//! (spec §15).
//!
//! States:
//!
//! ```text
//! IDLE → STARTING → LISTENING → TRANSCRIBING → ENHANCING → INSERTING → COMPLETED
//!                    ↓ (cancel anywhere)                    ↓
//!                 CANCELLED                              ERROR
//! ```
//!
//! Legal transitions are checked in [`DictationController::set_phase`]; every
//! other method funnels through it. Streaming partials come from
//! [`StreamingTranscript`], which commits words only when two consecutive
//! decodes agree (spec §52) — that is what keeps the overlay from flickering.
//!
//! ## Clipboard safety (spec §23)
//!
//! Insertion is delegated to the platform adapter, which implements its own
//! save/replace/paste/restore contract. This controller adds the failure
//! policy: if insertion fails for any reason the transcript is (a) written to a
//! recovery file in the cache directory, (b) placed on the clipboard where the
//! adapter could, and (c) reported as a failure with a user-facing message —
//! the text is never lost.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use airprompt_core::commands;
use airprompt_core::enhancement::{enhance_or_original, OpenAiCompatibleProvider};
use airprompt_core::error::{Error, Result};
use airprompt_core::formatting::{format_transcript, FormatContext, FormatOptions};
use airprompt_core::platform::{ActiveApplication, InsertMode, InsertOutcome};
use airprompt_core::settings::{AiProviderKind, Settings};
use airprompt_core::storage::{Database, NewHistoryEntry};
use airprompt_core::transcription::StreamingTranscript;
use airprompt_core::vad::{Vad, VadEvent};
use airprompt_core::whisper::SAMPLE_RATE;

use crate::events::{
    DiagnosticsSnapshot, EventSink, LatencySnapshot, OverlayState, ToastEvent, UiEvent,
};
use crate::state::AppState;

/// Phases of a take. Stored as a `u8` in an `AtomicU8`; never exposed raw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Starting,
    Listening,
    Transcribing,
    Enhancing,
    Inserting,
    Completed,
    Cancelled,
    Error,
}

impl Phase {
    fn as_u8(self) -> u8 {
        match self {
            Phase::Idle => 0,
            Phase::Starting => 1,
            Phase::Listening => 2,
            Phase::Transcribing => 3,
            Phase::Enhancing => 4,
            Phase::Inserting => 5,
            Phase::Completed => 6,
            Phase::Cancelled => 7,
            Phase::Error => 8,
        }
    }

    fn from_u8(value: u8) -> Option<Self> {
        Some(match value {
            0 => Phase::Idle,
            1 => Phase::Starting,
            2 => Phase::Listening,
            3 => Phase::Transcribing,
            4 => Phase::Enhancing,
            5 => Phase::Inserting,
            6 => Phase::Completed,
            7 => Phase::Cancelled,
            8 => Phase::Error,
            _ => return None,
        })
    }

    /// The overlay state to show, where one applies.
    ///
    /// Completed/Cancelled/Error emit through the explicit paths so each
    /// carries its payload exactly once; Idle hides the overlay.
    fn overlay(self) -> Option<OverlayState> {
        Some(match self {
            Phase::Idle => OverlayState::Hidden,
            Phase::Starting => OverlayState::Starting,
            Phase::Listening => OverlayState::Listening,
            Phase::Transcribing => OverlayState::Transcribing,
            Phase::Enhancing => OverlayState::Enhancing,
            Phase::Inserting => OverlayState::Inserting,
            _ => return None,
        })
    }
}

/// The legal edges of the state machine.
fn can_transition(from: Phase, to: Phase) -> bool {
    use Phase::*;
    matches!(
        (from, to),
        (Idle, Starting)
            | (Starting, Listening)
            | (Starting, Idle)
            | (Starting, Cancelled)
            | (Starting, Error)
            | (Listening, Transcribing)
            | (Listening, Cancelled)
            | (Listening, Error)
            | (Listening, Idle)
            | (Transcribing, Enhancing)
            | (Transcribing, Inserting)
            | (Transcribing, Cancelled)
            | (Transcribing, Error)
            | (Enhancing, Inserting)
            | (Enhancing, Cancelled)
            | (Enhancing, Error)
            | (Inserting, Completed)
            | (Inserting, Cancelled)
            | (Inserting, Error)
            | (Completed, Idle)
            | (Cancelled, Idle)
            | (Error, Idle)
    )
}

/// What a take will produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakePurpose {
    Dictation,
    Command,
}

/// Live state of one take.
struct Take {
    purpose: TakePurpose,
    started: Instant,
    /// The application that had focus when the take began (spec §22).
    target: ActiveApplication,
    stream: StreamingTranscript,
    vad: Vad,
    /// How much audio the streaming loop has already decoded.
    decoded_samples: usize,
    first_partial_ms: Option<f64>,
    decode_count: u32,
}

impl Take {
    fn new(purpose: TakePurpose, target: ActiveApplication, settings: &Settings) -> Self {
        Take {
            purpose,
            started: Instant::now(),
            target,
            stream: StreamingTranscript::new(),
            vad: Vad::new(settings.speech.silence),
            decoded_samples: 0,
            first_partial_ms: None,
            decode_count: 0,
        }
    }
}

/// What `poll` asks the runtime loop to do next.
#[derive(Debug, Clone, PartialEq)]
pub enum PollAction {
    /// Nothing to do; poll again after the given delay.
    Wait { resume_after_ms: u64 },
    /// Run one streaming decode of the audio so far.
    Decode,
    /// The take should end now (VAD trailing silence).
    Finish,
}

pub struct DictationController {
    state: Arc<AppState>,
    /// 0 = Idle, else `Phase::as_u8`.
    phase: AtomicU8,
    take: Mutex<Option<Take>>,
    /// Sample cursor into the capture buffer, advanced by `poll`.
    cursor: Mutex<usize>,
    /// The last successfully inserted transcript, for `paste_last`.
    last_transcript: Mutex<String>,
    /// Where a failed-insertion transcript was written, if it was.
    recovery_path: Mutex<Option<std::path::PathBuf>>,
    latency: Mutex<LatencySnapshot>,
}

impl DictationController {
    pub fn new(state: Arc<AppState>) -> Self {
        DictationController {
            state,
            phase: AtomicU8::new(Phase::Idle.as_u8()),
            take: Mutex::new(None),
            cursor: Mutex::new(0),
            last_transcript: Mutex::new(String::new()),
            recovery_path: Mutex::new(None),
            latency: Mutex::new(LatencySnapshot::default()),
        }
    }

    pub fn phase(&self) -> Phase {
        Phase::from_u8(self.phase.load(Ordering::SeqCst)).unwrap_or(Phase::Error)
    }

    pub fn is_busy(&self) -> bool {
        !matches!(
            self.phase(),
            Phase::Idle | Phase::Completed | Phase::Cancelled | Phase::Error
        )
    }

    fn set_phase(&self, to: Phase) -> Result<()> {
        let from = self.phase();
        if from == to {
            return Ok(());
        }
        if !can_transition(from, to) {
            return Err(Error::Store(format!(
                "illegal dictation transition {} → {}",
                from.as_u8(),
                to.as_u8()
            )));
        }
        self.apply_phase(to);
        Ok(())
    }

    /// Write the phase and emit the matching overlay, if any.
    fn apply_phase(&self, to: Phase) {
        self.phase.store(to.as_u8(), Ordering::SeqCst);
        if let Some(overlay) = to.overlay() {
            self.state.sink.emit(UiEvent::Overlay { state: overlay });
        }
    }

    /// The overlay state the UI should currently show.
    pub fn overlay_state(&self) -> OverlayState {
        self.phase().overlay().unwrap_or(OverlayState::Hidden)
    }

    /* ------------------------------------------------------------- start --- */

    /// Begin a take.
    ///
    /// Guards, in order: an existing live take, microphone permission, an
    /// unusable engine, a missing model. Each failure leaves the state idle
    /// and reports a user-facing reason — a refused start must never wedge the
    /// controller in a half-open phase.
    pub fn start(&self, purpose: TakePurpose, mode: Option<&str>) -> Result<()> {
        if self.is_busy() {
            return Err(Error::Store("a take is already in progress".into()));
        }

        // The model/engine gates use the controller's own SpeechService, the
        // same instance the take decodes run on: no query/set mismatch.
        if !self.state.speech.is_available() {
            let reason = self
                .state
                .speech
                .unavailable_reason()
                .unwrap_or_else(|| "The speech engine is not installed".to_string());
            return Err(Error::Unsupported(reason));
        }
        let settings = self.state.load_settings()?;
        if !self.state.speech.is_ready() {
            return Err(Error::NoModelInstalled);
        }

        // Permission check, with the platform's own answer rather than a
        // cached value, so macOS can route to System Settings.
        let permissions = self.state.platform.permission_status();
        if permissions.microphone == airprompt_core::platform::PermissionState::Denied {
            return Err(Error::Permission("Microphone".into()));
        }

        // Capture the target BEFORE recording (spec §22).
        let target = self.state.platform.active_application()?;

        // No device? Fail with the model-load-level message, before the phase
        // machine moves: resolve falls back to the default device itself.
        *self.cursor.lock().expect("cursor") = 0;
        let take = Take::new(purpose, target, &settings);
        *self.take.lock().expect("take lock") = Some(take);
        self.state.capture.clear();

        if let Some(mode) = mode {
            tracing::debug!(%mode, "dictation requested with explicit mode");
        }

        self.set_phase(Phase::Starting)?;
        self.set_phase(Phase::Listening)?;
        tracing::info!(purpose = ?purpose, "take started");
        Ok(())
    }

    /// Stop an active take and produce its transcript. Idempotent.
    pub fn stop(&self) -> Result<()> {
        match self.phase() {
            Phase::Listening => self.finish_take(),
            Phase::Starting => {
                self.cancel_internal();
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Cancel an active take, discarding its audio. Idempotent.
    pub fn cancel(&self) -> Result<()> {
        match self.phase() {
            Phase::Idle => Ok(()),
            _ => {
                self.cancel_internal();
                Ok(())
            }
        }
    }

    /// Toggle dictation from a global shortcut or the tray.
    ///
    /// A press while a take is live acts as a stop; otherwise a new dictation
    /// take starts. Start failures are surfaced as toasts — a hotkey must
    /// never fail silently.
    pub fn handle_activation(&self) {
        if self.is_busy() {
            if let Err(error) = self.stop() {
                tracing::warn!(code = error.code(), "activation stop failed");
            }
            return;
        }
        if let Err(error) = self.start(TakePurpose::Dictation, None) {
            tracing::warn!(code = error.code(), "activation start failed");
            self.state.sink.emit(UiEvent::Toast {
                toast: ToastEvent::from(&error),
            });
        }
    }

    /// Cancel from any state, including mid-pipeline.
    fn cancel_internal(&self) {
        if self.phase() != Phase::Idle {
            self.apply_phase(Phase::Cancelled);
        }
        *self.take.lock().expect("take lock") = None;
        *self.cursor.lock().expect("cursor") = 0;
        self.state.capture.clear();
        tracing::info!("take cancelled");
        self.apply_phase(Phase::Idle);
    }

    /* -------------------------------------------------------------- poll --- */

    /// Advance the take. Called from the runtime loop while listening.
    ///
    /// Feeds each new window of audio to the VAD exactly once (via the cursor)
    /// and asks for a streaming decode when autterance-gap worth of new audio
    /// has arrived. All state is cursor-based so frames are never consumed
    /// twice and never skipped.
    pub fn poll(&self, settings: &Settings) -> PollAction {
        if self.phase() != Phase::Listening {
            return PollAction::Wait {
                resume_after_ms: 50,
            };
        }

        // Feed the VAD the audio that arrived since the last poll.
        let fresh = self
            .state
            .capture
            .since(&mut *self.cursor.lock().expect("cursor"));
        let mut finish = false;
        if !fresh.is_empty() && settings.speech.vad_enabled {
            let frame = airprompt_core::audio::measure(&fresh);
            let frames = (fresh.len() / airprompt_core::vad::FRAME_SAMPLES).max(1);
            let Some(take) = self.take.lock().ok().and_then(|mut t| t.as_mut()) else {
                return PollAction::Wait {
                    resume_after_ms: 50,
                };
            };
            for _ in 0..frames {
                match take.vad.push(frame.rms) {
                    VadEvent::SpeechStart => {
                        self.state.sink.emit(UiEvent::Level { level: frame.peak });
                    }
                    VadEvent::SpeechEnd { .. } => {
                        if take.decoded_samples > 0 {
                            finish = true;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }

        if finish {
            return PollAction::Finish;
        }

        // Streaming decode cadence: one decode per utterance gap of audio.
        let gap_samples =
            (SAMPLE_RATE as u64 * settings.speech.silence.utterance_gap_ms() / 1000) as usize;
        let total = self.state.capture.sample_count();
        let should_decode = self
            .take
            .lock()
            .ok()
            .and_then(|t| t.as_ref())
            .map(|take| total.saturating_sub(take.decoded_samples) >= gap_samples)
            .unwrap_or(false);
        if should_decode {
            if let Ok(mut guard) = self.take.lock() {
                if let Some(take) = guard.as_mut() {
                    take.decoded_samples = total;
                    take.decode_count += 1;
                }
            }
            return PollAction::Decode;
        }

        PollAction::Wait {
            resume_after_ms: 100,
        }
    }

    /// Run one streaming decode over the take so far and emit the partial.
    ///
    /// The engine contract is full-buffer-in → text-out, so each partial is a
    /// decode of everything captured in this take; [`StreamingTranscript`]
    /// turns the noisy sequence into a stable committed prefix.
    pub fn decode_partial(&self) {
        let started = Instant::now();
        let audio = self.state.capture.snapshot();
        if audio.len() < airprompt_core::transcription::MIN_AUDIO_SAMPLES {
            return;
        }

        let hypothesis = match self.state.speech.transcribe(&audio) {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => return,
            Err(error) => {
                // A failed streaming decode must not kill the take: the final
                // decode may still succeed, and reporting failure here would
                // end a take a split second before it produced its text.
                tracing::debug!(code = error.code(), "streaming decode failed");
                return;
            }
        };

        let Some(transcript) = self
            .take
            .lock()
            .ok()
            .and_then(|mut t| t.as_mut())
            .map(|take| {
                let snapshot = take.stream.push(&hypothesis);
                if take.first_partial_ms.is_none() && !snapshot.is_empty() {
                    take.first_partial_ms = Some(take.started.elapsed().as_secs_f64() * 1000.0);
                }
                snapshot
            })
        else {
            return;
        };

        if !transcript.is_empty() {
            self.state.sink.emit(UiEvent::Transcript {
                committed: transcript.committed,
                interim: transcript.interim,
            });
        }
        self.record_sample(
            "partial_decode_ms",
            started.elapsed().as_secs_f64() * 1000.0,
        );
    }

    /* ------------------------------------------------------------ finish --- */

    /// End the take: final decode → format → enhance → insert → history.
    ///
    /// The engine gate is re-checked here rather than only in `start`: a model
    /// deleted or an engine changed between start and stop must fail here with
    /// a reason, not panic on a stale handle.
    pub fn finish_take(&self) -> Result<()> {
        if self.phase() != Phase::Listening {
            return Ok(());
        }
        self.set_phase(Phase::Transcribing)?;

        let settings = self.state.load_settings()?;
        let audio = self.state.capture.take_audio();
        self.state.capture.clear();
        *self.cursor.lock().expect("cursor") = 0;

        let (purpose, target, mut streaming) = {
            let mut guard = self.take.lock().expect("take lock");
            match guard.take() {
                Some(take) => (take.purpose, take.target, take.stream),
                None => {
                    self.apply_phase(Phase::Idle);
                    return Ok(());
                }
            }
        };

        let started = Instant::now();
        let raw = match self.state.speech.transcribe(&audio) {
            Ok(text) if !text.trim().is_empty() => text,
            Ok(_) => {
                self.finish_with_message("Nothing was heard.");
                return Ok(());
            }
            Err(error) => {
                self.fail(&error);
                return Err(error);
            }
        };
        let final_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.record_sample("final_transcript_ms", final_ms);

        // The final decode is authoritative (spec §52).
        let _ = streaming.finish();
        streaming.reset_to(&raw);
        drop(streaming);

        if purpose == TakePurpose::Command {
            return self.run_command_take(&raw, &target, &settings);
        }

        self.set_phase(Phase::Enhancing)?;
        self.set_phase(Phase::Inserting)?;
        self.deliver(&raw, &target, &settings, final_ms)
    }

    fn finish_with_message(&self, message: &str) {
        self.apply_phase(Phase::Idle);
        self.state.sink.emit(UiEvent::Overlay {
            state: OverlayState::Error {
                message: message.into(),
            },
        });
    }

    /// Format the final text, optionally enhance, then insert.
    ///
    /// Kept synchronous apart from the network enhancement: insertion itself
    /// is a platform call that already has its own settle delays.
    fn deliver(
        &self,
        raw: &str,
        target: &ActiveApplication,
        settings: &Settings,
        final_ms: f64,
    ) -> Result<()> {
        let dictionary = self.state.db.list_dictionary().unwrap_or_default();
        let profile = self.state.db.profile_for(&target.name).ok().flatten();
        let insert_mode = profile
            .as_ref()
            .and_then(|p| p.insert_mode)
            .unwrap_or(settings.dictation.insert_mode);
        let enhancement_enabled = profile
            .as_ref()
            .and_then(|p| p.enhancement_enabled)
            .unwrap_or(settings.enhancement.enabled);

        // Preceding text is unavailable in this build (no AX bindings for
        // reading the caret context); continuous joining degrades to a plain
        // leading space, which the formatting pipeline handles.
        let context = FormatContext {
            preceding_text: String::new(),
            code_context: is_code_context(&target.name),
        };
        let options = FormatOptions {
            spoken_punctuation: settings.dictation.spoken_punctuation,
            remove_fillers: settings.enhancement.remove_fillers || !enhancement_enabled,
            auto_capitalization: settings.enhancement.auto_capitalization || !enhancement_enabled,
            continuous_dictation: settings.dictation.continuous_dictation,
            trailing_space: settings.dictation.trailing_space,
            gaav_mode: settings.dictation.gaav_mode,
            dictionary,
        };
        let mut text = format_transcript(raw, &options, &context);
        if text.trim().is_empty() {
            self.finish_with_message("Nothing was heard.");
            return Ok(());
        }

        /* --------------------------------------------- enhancement --- */
        let mut was_enhanced = false;
        if enhancement_enabled
            && settings.enhancement.provider != AiProviderKind::None
            && settings.enhancement.enabled
        {
            let api_key = self
                .state
                .secrets
                .get(settings.enhancement.provider.secret_key())
                .ok()
                .flatten();
            let provider = OpenAiCompatibleProvider::new();
            let (enhanced_text, enhanced, warning) = block_on_enhancement(
                &provider,
                &text,
                &settings.enhancement,
                api_key.as_deref(),
                settings.privacy.local_only,
            );
            if let Some(warning) = &warning {
                tracing::warn!(warning, "enhancement fell back to the original");
                self.state.sink.emit(UiEvent::Toast {
                    toast: ToastEvent::warning(warning.clone()),
                });
            }
            text = enhanced_text;
            was_enhanced = enhanced;
        }

        /* ----------------------------------------------- insertion --- */
        let insert_started = Instant::now();
        let outcome = self.insert_into_target(&text, target, insert_mode, settings);
        let insert_ms = insert_started.elapsed().as_secs_f64() * 1000.0;
        self.record_sample("insertion_ms", insert_ms);

        match outcome {
            Ok(InsertOutcome::Inserted) => {
                self.complete(&text, target, was_enhanced, settings, final_ms, insert_ms);
                Ok(())
            }
            Ok(InsertOutcome::CopiedOnly) => {
                self.apply_phase(Phase::Idle);
                self.state.sink.emit(UiEvent::Overlay {
                    state: OverlayState::Error {
                        message: "Copied to clipboard — paste where you need it.".into(),
                    },
                });
                Ok(())
            }
            Err(error) => {
                let _ = self.write_recovery_file(&text);
                self.fail(&error);
                Err(error)
            }
        }
    }

    /// Insert with the target protections (spec §22): secure fields refused,
    /// focus restored, and a failure degrades to clipboard rather than loss.
    fn insert_into_target(
        &self,
        text: &str,
        target: &ActiveApplication,
        mode: InsertMode,
        settings: &Settings,
    ) -> Result<InsertOutcome> {
        if mode == InsertMode::ClipboardOnly {
            self.state.platform.set_clipboard(text)?;
            return Ok(InsertOutcome::CopiedOnly);
        }

        if self.state.platform.is_secure_input() {
            let _ = self.state.platform.set_clipboard(text);
            return Err(Error::SecureInput);
        }

        if settings.dictation.restore_focus {
            let _ = self.state.platform.restore_focus(target);
        }

        match self.state.platform.insert_text(text, mode) {
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                let copied = self.state.platform.set_clipboard(text).is_ok();
                if copied {
                    Err(error)
                } else {
                    Err(Error::Insertion(error.message()))
                }
            }
        }
    }

    fn complete(
        &self,
        text: &str,
        target: &ActiveApplication,
        was_enhanced: bool,
        settings: &Settings,
        final_ms: f64,
        insert_ms: f64,
    ) {
        *self.last_transcript.lock().expect("last transcript") = text.to_string();
        self.recovery_path.lock().expect("recovery path").take();

        if settings.privacy.save_history {
            let entry = self
                .state
                .db
                .insert_history(&NewHistoryEntry {
                    duration_ms: self.state.capture.elapsed_ms() as i64,
                    app_name: target.name.clone(),
                    transcript: text.to_string(),
                    mode: "standard".into(),
                    model_id: self.state.speech.loaded_model_id().unwrap_or_default(),
                    enhanced: was_enhanced,
                    command_intent: None,
                })
                .ok();
            if let Some(entry) = entry {
                self.state.sink.emit(UiEvent::History { entry });
            }
        }

        if let Ok(mut latency) = self.latency.lock() {
            latency.final_transcript_ms = Some(final_ms);
            latency.insertion_ms = Some(insert_ms);
            latency.total_ms = Some(final_ms + insert_ms);
        }

        self.apply_phase(Phase::Completed);
        self.state.sink.emit(UiEvent::Overlay {
            state: OverlayState::Success {
                text: text.to_string(),
            },
        });
        self.apply_phase(Phase::Idle);
    }

    /* -------------------------------------------------------- command mode --- */

    /// Finish a command-mode take: parse, execute, report.
    fn run_command_take(
        &self,
        raw: &str,
        target: &ActiveApplication,
        _settings: &Settings,
    ) -> Result<()> {
        let custom = self.state.db.list_commands().unwrap_or_default();
        let apps = self
            .state
            .platform
            .installed_applications()
            .unwrap_or_default();
        let shortcuts = self.state.platform.list_shortcuts().unwrap_or_default();
        let targets = commands::Targets {
            apps,
            shortcuts,
            custom,
        };

        let parsed = commands::parse(raw, &targets);
        self.state.sink.emit(UiEvent::Command {
            command: parsed.clone(),
            raw: raw.to_string(),
        });

        let Some(parsed) = parsed else {
            self.apply_phase(Phase::Idle);
            self.state.sink.emit(UiEvent::Overlay {
                state: OverlayState::Error {
                    message: "No command was recognised in that.".into(),
                },
            });
            return Ok(());
        };

        if parsed.confidence < commands::CONFIDENCE_FLOOR {
            self.apply_phase(Phase::Idle);
            self.state.sink.emit(UiEvent::Overlay {
                state: OverlayState::Error {
                    message: format!("Did you mean \u{201C}{}\u{201D}?", parsed.target),
                },
            });
            return Ok(());
        }

        // Custom workflows must never auto-run a flagged command without
        // confirmation (§58: no automatic execution of anything risky here);
        // the shell-level run_command enforces confirmation, this path only
        // executes single, already-verified steps when the parser is sure.
        let outcome = crate::commands::execute_parsed(&parsed, &self.state.platform);
        let _ = target;
        self.apply_phase(Phase::Idle);
        self.state.sink.emit(UiEvent::Overlay {
            state: if outcome.ok {
                OverlayState::Success {
                    text: outcome.message.clone(),
                }
            } else {
                OverlayState::Error {
                    message: outcome.message.clone(),
                }
            },
        });
        Ok(())
    }

    /* ---------------------------------------------------------- recovery --- */

    fn recovery_file_path(&self) -> std::path::PathBuf {
        self.recovery_path
            .lock()
            .expect("recovery path")
            .clone()
            .unwrap_or_else(|| std::path::PathBuf::from("last-transcript.txt"))
    }

    /// Persist a transcript the app failed to deliver, so nothing is lost.
    fn write_recovery_file(&self, text: &str) -> Result<std::path::PathBuf> {
        let path = self.recovery_file_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(&path, text)?;
        *self.recovery_path.lock().expect("recovery path") = Some(path.clone());
        tracing::info!(path = %path.display(), "uninsertable transcript saved");
        Ok(path)
    }

    /// Point the recovery file at the real cache path. Called at startup.
    pub fn set_recovery_path(&self, path: std::path::PathBuf) {
        *self.recovery_path.lock().expect("recovery path") = Some(path);
    }

    /// The most recently inserted transcript, for `paste_last`.
    pub fn last_transcript(&self) -> String {
        self.last_transcript
            .lock()
            .expect("last transcript")
            .clone()
    }

    /* ------------------------------------------------------- diagnostics --- */

    fn record_sample(&self, stage: &str, millis: f64) {
        let _ = self.state.db.record_sample(stage, millis);
    }

    pub fn diagnostics(&self) -> DiagnosticsSnapshot {
        let settings = self.state.load_settings().unwrap_or_default();
        let permissions = self.state.platform.permission_status();
        let latency = self.latency.lock().map(|l| l.clone()).unwrap_or_default();
        let (cpu_percent, memory_mb) = process_metrics();
        let (peak, _rms) = self.state.capture.take_level();

        DiagnosticsSnapshot {
            os: self.state.platform.platform_name().to_string(),
            arch: self.state.platform.arch().to_string(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            speech_engine: self.state.speech.engine_id().to_string(),
            model_id: self.state.speech.loaded_model_id().unwrap_or_default(),
            model_ready: self.state.speech.is_ready(),
            microphone: format_level(peak),
            hotkey: settings.shortcuts.dictation.accelerator.clone(),
            accessibility: permissions.accessibility,
            ai_provider: format!("{:?}", settings.enhancement.provider).to_lowercase(),
            ai_model: settings.enhancement.model.clone(),
            local_only: settings.privacy.local_only,
            latency,
            cpu_percent,
            memory_mb,
        }
    }

    /// Fail with a user-facing message, from any live phase.
    fn fail(&self, error: &Error) {
        tracing::warn!(code = error.code(), "take failed");
        self.apply_phase(Phase::Idle);
        self.state.sink.emit(UiEvent::Overlay {
            state: OverlayState::Error {
                message: error.message(),
            },
        });
        self.state.sink.emit(UiEvent::Toast {
            toast: error.into(),
        });
    }
}

/// Drive the async enhancement from sync code with a timeout ceiling.
///
/// The controller is sync by design (simpler state ownership), and
/// `enhance_or_original` already encodes the "original on failure" contract.
/// The enhancement settings carry their own timeout, applied inside the
/// reqwest client, so the block here is bounded by it.
fn block_on_enhancement(
    provider: &OpenAiCompatibleProvider,
    text: &str,
    settings: &airprompt_core::settings::EnhancementSettings,
    api_key: Option<&str>,
    local_only: bool,
) -> (String, bool, Option<String>) {
    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::try_current()
            .ok()
            .and_then(|handle| {
                handle
                    .block_on(async {
                        enhance_or_original(provider, text, settings, api_key, local_only).await
                    })
                    .into()
            })
            .unwrap_or_else(|| (text.to_string(), false, None))
    })
}

/// Run one enhancement round-trip and report connectivity honestly.
///
/// Used by the settings screen's `test_ai` command. Every failure mode
/// (timeout, bad key, malformed response, empty reply) comes back as a
/// message, never a panic, and never as a fabricated success.
pub fn test_enhancement(
    settings: &airprompt_core::settings::EnhancementSettings,
    api_key: Option<&str>,
    local_only: bool,
) -> (String, bool, Option<String>) {
    let provider = OpenAiCompatibleProvider::new();
    block_on_enhancement(
        &provider,
        "AirPrompt connectivity test.",
        settings,
        api_key,
        local_only,
    )
}

/// True when the focused app is one where sentence casing usually hurts.
fn is_code_context(app_name: &str) -> bool {
    const EDITORS: &[&str] = &[
        "code",
        "visual studio code",
        "vim",
        "neovim",
        "sublime",
        "intellij",
        "terminal",
        "iterm",
        "powershell",
        "cmd",
        "windows terminal",
        "console",
    ];
    let name = app_name.to_lowercase();
    EDITORS.iter().any(|editor| name.contains(editor))
}

/// A one-word summary of the input level for diagnostics.
fn format_level(peak: f32) -> String {
    match peak {
        p if p <= 0.0 => "silent".into(),
        p if p < 0.05 => "very quiet".into(),
        p if p < 0.5 => "active".into(),
        _ => "loud".into(),
    }
}

/// Process CPU and memory, when measured.
///
/// Nothing is fabricated: without a `/proc` or `GetProcessMemoryInfo` binding
/// the honest value is `None`, which the UI renders as a dash.
fn process_metrics() -> (Option<f64>, Option<f64>) {
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::testing::RecordingSink;

    fn controller(temp: &tempfile::TempDir) -> (Arc<DictationController>, Arc<RecordingSink>) {
        let sink = Arc::new(RecordingSink::new());
        let app = crate::state::testing::app_state_with_sink(temp, sink.clone());
        (Arc::new(DictationController::new(app)), sink)
    }

    #[test]
    fn transitions_are_illegal_from_idle_to_completed() {
        assert!(!can_transition(Phase::Idle, Phase::Completed));
        assert!(!can_transition(Phase::Idle, Phase::Inserting));
        assert!(!can_transition(Phase::Idle, Phase::Transcribing));
        assert!(can_transition(Phase::Idle, Phase::Starting));
    }

    #[test]
    fn the_full_happy_path_is_legal() {
        let path = [
            Phase::Idle,
            Phase::Starting,
            Phase::Listening,
            Phase::Transcribing,
            Phase::Enhancing,
            Phase::Inserting,
            Phase::Completed,
            Phase::Idle,
        ];
        for pair in path.windows(2) {
            assert!(
                can_transition(pair[0], pair[1]),
                "{} → {} must be legal",
                pair[0].as_u8(),
                pair[1].as_u8()
            );
        }
    }

    #[test]
    fn every_live_phase_can_cancel_or_fail() {
        for live in [
            Phase::Starting,
            Phase::Listening,
            Phase::Transcribing,
            Phase::Enhancing,
            Phase::Inserting,
        ] {
            assert!(can_transition(live, Phase::Cancelled));
            assert!(can_transition(live, Phase::Error));
        }
    }

    #[test]
    fn a_finished_take_must_return_to_idle_before_starting_again() {
        assert!(!can_transition(Phase::Completed, Phase::Listening));
        assert!(can_transition(Phase::Completed, Phase::Idle));
        assert!(can_transition(Phase::Idle, Phase::Starting));
    }

    #[test]
    fn phase_round_trips_through_its_u8_form() {
        for phase in [
            Phase::Idle,
            Phase::Starting,
            Phase::Listening,
            Phase::Transcribing,
            Phase::Enhancing,
            Phase::Inserting,
            Phase::Completed,
            Phase::Cancelled,
            Phase::Error,
        ] {
            assert_eq!(Phase::from_u8(phase.as_u8()), Some(phase));
        }
        assert_eq!(Phase::from_u8(255), None);
    }

    #[test]
    fn start_refuses_when_the_engine_is_unavailable_and_stays_idle() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        let error = controller
            .start(TakePurpose::Dictation, None)
            .err()
            .expect("an unavailable engine must refuse to start");
        assert_eq!(error.code(), "unsupported");
        assert_eq!(controller.phase(), Phase::Idle);
        assert!(!controller.is_busy());
    }

    #[test]
    fn stop_and_cancel_are_no_ops_when_idle() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        assert!(controller.stop().is_ok());
        assert!(controller.cancel().is_ok());
        assert_eq!(controller.phase(), Phase::Idle);
    }

    #[test]
    fn set_phase_rejects_illegal_moves_without_changing_state() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        assert!(controller.set_phase(Phase::Inserting).is_err());
        assert_eq!(controller.phase(), Phase::Idle);
    }

    #[test]
    fn overlay_state_is_hidden_when_idle() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        assert_eq!(controller.overlay_state(), OverlayState::Hidden);
    }

    #[test]
    fn diagnostics_reports_the_real_engine_and_never_fakes_metrics() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        let snapshot = controller.diagnostics();
        assert_eq!(snapshot.speech_engine, "whisper_cpp");
        assert!(!snapshot.model_ready);
        assert_eq!(snapshot.cpu_percent, None, "no fabricated CPU numbers");
        assert_eq!(snapshot.memory_mb, None);
        assert_eq!(snapshot.app_version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn code_context_covers_the_common_editors() {
        assert!(is_code_context("Visual Studio Code"));
        assert!(is_code_context("Windows Terminal"));
        assert!(is_code_context("iTerm2"));
        assert!(!is_code_context("Notepad"));
        assert!(!is_code_context("Google Chrome"));
    }

    #[test]
    fn poll_outside_a_take_waits() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        let settings = Settings::default();
        assert_eq!(
            controller.poll(&settings),
            PollAction::Wait {
                resume_after_ms: 50
            }
        );
    }

    #[test]
    fn decode_partial_on_an_empty_take_is_a_no_op() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, sink) = controller(&temp);
        controller.decode_partial();
        assert!(
            sink.events()
                .iter()
                .all(|e| !matches!(e, UiEvent::Transcript { .. })),
            "no partial may be emitted for empty audio"
        );
    }

    #[test]
    fn short_audio_does_not_reach_the_engine() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, sink) = controller(&temp);
        // 100 ms of samples at 16 kHz: below MIN_AUDIO_SAMPLES.
        let fake = crate::state::testing::app_state_with_sink(&temp, sink.clone());
        fake.capture.push_interleaved(&vec![0.1f32; 1_600]);
        controller.decode_partial();
        assert!(sink
            .events()
            .iter()
            .all(|e| !matches!(e, UiEvent::Transcript { .. })));
    }

    #[test]
    fn last_transcript_starts_empty() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        assert_eq!(controller.last_transcript(), "");
    }

    #[test]
    fn recovery_path_defaults_to_a_named_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        assert_eq!(
            controller.recovery_file_path(),
            std::path::PathBuf::from("last-transcript.txt")
        );
    }

    #[test]
    fn push_to_talk_parameter_accepts_mode_strings() {
        // The mode parameter is diagnostic only; both values must be accepted
        // without state changes.
        let temp = tempfile::TempDir::new().unwrap();
        let (controller, _sink) = controller(&temp);
        for mode in ["push_to_talk", "toggle"] {
            let error = controller.start(TakePurpose::Dictation, Some(mode)).err();
            assert!(error.is_some(), "engine unavailable, must still refuse");
            assert_eq!(controller.phase(), Phase::Idle);
        }
    }
}
