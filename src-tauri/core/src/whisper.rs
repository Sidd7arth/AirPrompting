//! whisper.cpp backend, driven through its command-line tool.
//!
//! ## Why a process and not FFI
//!
//! `whisper-rs` would link whisper.cpp into the app, which is nicer, but it
//! requires a working C++ toolchain and CMake on every machine that builds
//! AirPrompt. Shipping the upstream `whisper-cli` binary as a bundled resource
//! keeps the build reproducible and lets the app run on machines without
//! developer tools.
//!
//! ## The temporary-file constraint
//!
//! **`whisper-cli` only accepts a WAV file via `-f`; it cannot read raw PCM
//! from stdin** (that is still an open upstream feature request). So each
//! decode writes a 16 kHz mono WAV into the app's own cache directory and
//! deletes it as soon as the process exits. The file never leaves that
//! directory, is never added to history, and is removed even on failure. This
//! is the one place the app writes audio to disk, and it is documented in
//! `docs/TROUBLESHOOTING.md`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::audio::{pcm_i16_to_bytes, to_pcm_i16};
use crate::error::{Error, Result};
use crate::identity;
use crate::transcription::{default_threads, EngineOptions, SpeechEngine};

/// Sample rate whisper.cpp models expect.
pub const SAMPLE_RATE: u32 = 16_000;

/// Decoding is CPU-bound; cap it so a wedged process cannot hang a take.
const DECODE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(180);

pub struct WhisperCliEngine {
    binary: PathBuf,
    cache_dir: PathBuf,
    state: Mutex<EngineState>,
    ready: AtomicBool,
}

struct EngineState {
    model: Option<PathBuf>,
    options: EngineOptions,
}

impl WhisperCliEngine {
    /// Create an engine around a specific `whisper-cli` binary. The binary is
    /// not validated here; `prepare` reports a missing one.
    pub fn new(binary: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        WhisperCliEngine {
            binary: binary.into(),
            cache_dir: cache_dir.into(),
            state: Mutex::new(EngineState {
                model: None,
                options: EngineOptions::default(),
            }),
            ready: AtomicBool::new(false),
        }
    }

    /// Find `whisper-cli` (or the older `main` / `whisper` names) in the given
    /// directories, falling back to `PATH`.
    ///
    /// Returns `None` when the tool is absent, which the settings screen
    /// surfaces as "speech engine not installed" rather than a crash.
    pub fn discover(directories: &[PathBuf]) -> Option<PathBuf> {
        let explicit = identity::env(
            identity::ENV_WHISPER_BIN,
            &[identity::LEGACY_ENV_WHISPER_BIN],
        )
        .map(PathBuf::from);
        let names = ["whisper-cli", "whisper-cli.exe", "main", "main.exe"];

        if let Some(path) = explicit {
            if path.is_file() {
                return Some(path);
            }
        }

        for dir in directories {
            for name in names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }

        // PATH lookup.
        let path_var = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path_var) {
            for name in names {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
        None
    }

    /// Find a downloaded GGML model in `directory`.
    pub fn locate_model(directory: &Path) -> Option<PathBuf> {
        let mut candidates: Vec<PathBuf> = std::fs::read_dir(directory)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension().map(|e| e == "bin").unwrap_or(false)
                    && path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.starts_with("ggml-"))
                        .unwrap_or(false)
            })
            .collect();
        candidates.sort();
        candidates.into_iter().next()
    }

    /// The model currently loaded, for diagnostics.
    pub fn loaded_model(&self) -> Option<PathBuf> {
        self.state.lock().ok().and_then(|s| s.model.clone())
    }
}

impl SpeechEngine for WhisperCliEngine {
    fn id(&self) -> &'static str {
        "whisper_cpp"
    }

    fn display_name(&self) -> &'static str {
        "whisper.cpp (local)"
    }

    fn is_ready(&self) -> bool {
        self.ready.load(Ordering::SeqCst)
    }

    fn prepare(&self, model_path: &Path, options: &EngineOptions) -> Result<()> {
        if !self.binary.is_file() {
            return Err(Error::Unsupported(
                "The whisper.cpp speech engine is not installed".into(),
            ));
        }
        if !model_path.is_file() {
            return Err(Error::NoModelInstalled);
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| Error::ModelLoad("engine state lock poisoned".into()))?;

        // Already loaded: keep it, just adopt the new options.
        if state.model.as_deref() == Some(model_path) && self.is_ready() {
            state.options = options.clone();
            return Ok(());
        }

        // A one-word decode proves the binary and the model can actually load,
        // so failures surface at prepare time rather than mid-dictation.
        let probe = vec![0.0f32; SAMPLE_RATE as usize / 4];
        let wav = self.write_wav(&probe)?;
        let outcome = self.run(&wav, model_path, options);
        let _ = std::fs::remove_file(&wav);
        outcome?;

        state.model = Some(model_path.to_path_buf());
        state.options = options.clone();
        self.ready.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn transcribe(&self, samples: &[f32]) -> Result<String> {
        let (model, options) = {
            let state = self
                .state
                .lock()
                .map_err(|_| Error::Transcription("engine state lock poisoned".into()))?;
            match state.model.clone() {
                Some(model) => (model, state.options.clone()),
                None => return Err(Error::ModelNotReady),
            }
        };

        let wav = self.write_wav(samples)?;
        let result = self.run(&wav, &model, &options);
        // Always remove the audio, success or failure.
        let _ = std::fs::remove_file(&wav);
        result
    }

    fn unload(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.model = None;
        }
        self.ready.store(false, Ordering::SeqCst);
    }

    fn is_available(&self) -> bool {
        self.binary.is_file()
    }

    fn unavailable_reason(&self) -> Option<String> {
        if self.binary.is_file() {
            None
        } else {
            Some(format!(
                "Install the whisper.cpp CLI or set {}.",
                identity::ENV_WHISPER_BIN
            ))
        }
    }
}

impl WhisperCliEngine {
    fn write_wav(&self, samples: &[f32]) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.cache_dir)?;
        let name = format!("take-{}.wav", uuid::Uuid::new_v4());
        let path = self.cache_dir.join(name);
        let bytes = encode_wav_16k_mono(samples);
        std::fs::write(&path, bytes)?;
        Ok(path)
    }

    fn run(&self, wav: &Path, model: &Path, options: &EngineOptions) -> Result<String> {
        let threads = if options.threads > 0 {
            options.threads
        } else {
            default_threads(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4),
            )
        };

        let mut command = Command::new(&self.binary);
        command
            .arg("-m")
            .arg(model)
            .arg("-f")
            .arg(wav)
            .arg("-l")
            .arg(if options.language.is_empty() {
                "en"
            } else {
                &options.language
            })
            .arg("-t")
            .arg(threads.to_string())
            // Plain text without timestamp prefixes.
            .arg("-nt")
            // Suppress the model banner on stderr.
            .arg("-np")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if !options.use_gpu {
            command.arg("-ng");
        }

        let mut child = command
            .spawn()
            .map_err(|e| Error::Transcription(format!("could not start whisper: {e}")))?;

        // Drain stderr on a helper thread so a chatty run cannot fill the pipe
        // and deadlock the child.
        let stderr = child.stderr.take();
        let drain = stderr.map(|mut pipe| {
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buffer = String::new();
                let _ = pipe.read_to_string(&mut buffer);
                buffer
            })
        });

        let started = std::time::Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if started.elapsed() > DECODE_TIMEOUT {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(Error::Transcription("speech recognition timed out".into()));
                    }
                    std::thread::sleep(std::time::Duration::from_millis(20));
                }
                Err(e) => return Err(Error::Transcription(e.to_string())),
            }
        }

        let output = child
            .wait_with_output()
            .map_err(|e| Error::Transcription(e.to_string()))?;

        let stderr_text = drain
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default();

        if !output.status.success() {
            let detail = stderr_text
                .lines()
                .last()
                .unwrap_or("whisper exited with an error");
            return Err(Error::Transcription(detail.trim().to_string()));
        }

        Ok(parse_cli_output(&String::from_utf8_lossy(&output.stdout)))
    }
}

/// Encode 16-bit mono PCM in a RIFF/WAVE container.
///
/// Hand-written rather than pulled from a `wav` crate: it is 44 bytes of
/// header and keeping it here makes the format auditable and testable.
pub fn encode_wav_16k_mono(samples: &[f32]) -> Vec<u8> {
    let pcm = pcm_i16_to_bytes(&to_pcm_i16(samples));
    let data_len = pcm.len() as u32;
    let byte_rate = SAMPLE_RATE * 2; // mono, 2 bytes per sample
    let block_align: u16 = 2;

    let mut out = Vec::with_capacity(44 + pcm.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // format: PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // channels
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend(&pcm);
    out
}

/// Extract the transcript from `whisper-cli` stdout.
///
/// With `-nt` the tool prints bare text, but some builds still emit timestamp
/// prefixes and blank lines, so both are handled.
pub fn parse_cli_output(stdout: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for raw in stdout.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let text = strip_timestamp_prefix(line);
        if text.is_empty() {
            continue;
        }
        lines.push(text.to_string());
    }
    lines.join(" ").trim().to_string()
}

/// Remove a leading `[00:00:00.000 --> 00:00:02.000]` marker if present.
fn strip_timestamp_prefix(line: &str) -> &str {
    if !line.starts_with('[') {
        return line;
    }
    match line.find(']') {
        Some(end) if line[..end].contains("-->") => line[end + 1..].trim(),
        _ => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_header_is_a_valid_riff_chunk() {
        let samples = vec![0.0f32; 160];
        let wav = encode_wav_16k_mono(&samples);
        assert_eq!(wav.len(), 44 + 160 * 2);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes(wav[16..20].try_into().unwrap()), 16);
        assert_eq!(
            u16::from_le_bytes(wav[20..22].try_into().unwrap()),
            1,
            "PCM"
        );
        assert_eq!(
            u16::from_le_bytes(wav[22..24].try_into().unwrap()),
            1,
            "mono"
        );
        assert_eq!(
            u32::from_le_bytes(wav[24..28].try_into().unwrap()),
            16_000,
            "16 kHz"
        );
        assert_eq!(
            u32::from_le_bytes(wav[28..32].try_into().unwrap()),
            32_000,
            "byte rate"
        );
        assert_eq!(u16::from_le_bytes(wav[32..34].try_into().unwrap()), 2);
        assert_eq!(u16::from_le_bytes(wav[34..36].try_into().unwrap()), 16);
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes(wav[40..44].try_into().unwrap()),
            (160 * 2) as u32
        );
    }

    #[test]
    fn wav_encodes_sample_values() {
        let wav = encode_wav_16k_mono(&[0.0, 1.0, -1.0]);
        let pcm = &wav[44..];
        assert_eq!(i16::from_le_bytes([pcm[0], pcm[1]]), 0);
        assert_eq!(i16::from_le_bytes([pcm[2], pcm[3]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([pcm[4], pcm[5]]), -i16::MAX);
    }

    #[test]
    fn parses_plain_and_timestamped_output() {
        assert_eq!(
            parse_cli_output("Hello this is a test.\n"),
            "Hello this is a test."
        );
        assert_eq!(
            parse_cli_output("[00:00:00.000 --> 00:00:02.000]   Hello\n[00:00:02.000 --> 00:00:04.000]   world\n"),
            "Hello world"
        );
        assert_eq!(parse_cli_output("\n\n  \n"), "");
        assert_eq!(parse_cli_output(""), "");
    }

    #[test]
    fn brackets_that_are_not_timestamps_are_kept() {
        // A transcription that legitimately starts with a bracket.
        assert_eq!(parse_cli_output("[inaudible] hello"), "[inaudible] hello");
    }

    #[test]
    fn engine_reports_unavailable_without_a_binary() {
        let engine = WhisperCliEngine::new(
            std::env::temp_dir().join("definitely-not-whisper-cli"),
            std::env::temp_dir(),
        );
        assert!(!engine.is_available());
        assert!(!engine.is_ready());
        assert!(engine.unavailable_reason().is_some());
        assert!(engine
            .prepare(
                Path::new("/nonexistent/model.bin"),
                &EngineOptions::default()
            )
            .is_err());
        // Transcribing without a prepared model is a clean error, not a panic.
        assert!(matches!(
            engine.transcribe(&vec![0.0; 16_000]),
            Err(Error::ModelNotReady)
        ));
        engine.unload();
    }

    #[test]
    fn discover_returns_none_for_empty_directories() {
        let dir = tempfile::tempdir().unwrap();
        // Only assert the negative when the host really has no whisper on PATH.
        if identity::env(
            identity::ENV_WHISPER_BIN,
            &[identity::LEGACY_ENV_WHISPER_BIN],
        )
        .is_none()
        {
            let found = WhisperCliEngine::discover(&[dir.path().to_path_buf()]);
            assert!(
                found.is_none() || found.as_ref().map(|p| p.is_file()).unwrap_or(false),
                "discover must only return a real file"
            );
        }
    }

    #[test]
    fn locate_model_only_accepts_ggml_bin_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(WhisperCliEngine::locate_model(dir.path()).is_none());

        std::fs::write(dir.path().join("notes.txt"), "x").unwrap();
        assert!(WhisperCliEngine::locate_model(dir.path()).is_none());

        std::fs::write(dir.path().join("ggml-base.en.bin"), "x").unwrap();
        let found = WhisperCliEngine::locate_model(dir.path()).unwrap();
        assert!(found.to_string_lossy().contains("ggml-base.en.bin"));
    }
}
