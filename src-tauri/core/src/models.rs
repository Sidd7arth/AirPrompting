//! Speech model catalogue and lifecycle (spec §9, §10, §92).
//!
//! Models are whisper.cpp GGML files hosted on Hugging Face. Nothing is
//! downloaded until the user asks: only the catalogue is static data. Downloads
//! are resumable via HTTP `Range`, verified before they become usable, and
//! stored under an application-managed directory so an app update never forces
//! a re-download.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/* ------------------------------------------------------------------- spec */

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelTier {
    Tiny,
    Base,
    Small,
    Medium,
}

impl ModelTier {
    pub fn label(self) -> &'static str {
        match self {
            ModelTier::Tiny => "Tiny",
            ModelTier::Base => "Base",
            ModelTier::Small => "Small",
            ModelTier::Medium => "Medium",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelStatus {
    NotDownloaded,
    Downloading,
    Paused,
    Verifying,
    Ready,
    Error,
}

/// A model that can be installed. Static catalogue data only.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub tier: ModelTier,
    /// File name inside the models directory.
    pub file_name: &'static str,
    pub url: &'static str,
    /// Exact byte length of the published file, used as a length check.
    pub size_bytes: u64,
    /// SHA-256 of the published file, when known.
    ///
    /// Left `None` for entries that have not been pinned yet; verification then
    /// falls back to the length and GGML-header checks, which still catches a
    /// truncated or HTML error page masquerading as a model. Fill these in from
    /// the upstream checksums to enable full digest verification.
    pub sha256: Option<&'static str>,
    pub languages: &'static str,
    pub speed: u8,
    pub accuracy: u8,
    pub ram_mb: u32,
    pub description: &'static str,
}

/// The English-only models, which are what the app recommends by default:
/// they are faster and more accurate than the multilingual equivalents at the
/// same size.
pub const CATALOG: &[ModelSpec] = &[
    ModelSpec {
        id: "whisper-tiny-en",
        tier: ModelTier::Tiny,
        file_name: "ggml-tiny.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-tiny.en.bin",
        size_bytes: 77_704_715,
        sha256: None,
        languages: "English",
        speed: 5,
        accuracy: 2,
        ram_mb: 390,
        description: "Fastest option. Best for quick notes and older machines.",
    },
    ModelSpec {
        id: "whisper-base-en",
        tier: ModelTier::Base,
        file_name: "ggml-base.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
        size_bytes: 147_964_211,
        sha256: None,
        languages: "English",
        speed: 4,
        accuracy: 3,
        ram_mb: 440,
        description: "The default balance of speed and accuracy for everyday dictation.",
    },
    ModelSpec {
        id: "whisper-small-en",
        tier: ModelTier::Small,
        file_name: "ggml-small.en.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.en.bin",
        size_bytes: 487_614_501,
        sha256: None,
        languages: "English",
        speed: 3,
        accuracy: 4,
        ram_mb: 1_020,
        description: "Noticeably better with names and technical vocabulary.",
    },
    ModelSpec {
        id: "whisper-medium",
        tier: ModelTier::Medium,
        file_name: "ggml-medium.bin",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-medium.bin",
        size_bytes: 1_533_774_691,
        sha256: None,
        languages: "Multilingual (99 languages)",
        speed: 1,
        accuracy: 5,
        ram_mb: 2_600,
        description: "Highest accuracy and the only multilingual option. Needs a capable machine.",
    },
];

pub fn spec(id: &str) -> Option<&'static ModelSpec> {
    CATALOG.iter().find(|m| m.id == id)
}

/// Entry point for onboarding when the user has not chosen anything.
pub const DEFAULT_MODEL_ID: &str = "whisper-base-en";

/* --------------------------------------------------------------- hardware */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemProfile {
    pub os: String,
    pub arch: String,
    pub cpu_cores: u32,
    pub total_ram_mb: u64,
    pub free_disk_mb: u64,
    pub has_gpu: bool,
    pub accelerator: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRecommendation {
    pub recommended: String,
    pub fastest: String,
    pub accurate: String,
    pub reason: String,
}

/// Pick a model the machine can actually run (spec §10).
///
/// Never silently recommends the largest model: Medium needs ~2.6 GB resident
/// and would make an 8 GB laptop unusable.
pub fn recommend(profile: &SystemProfile) -> ModelRecommendation {
    let recommended = if profile.total_ram_mb < 6_000 {
        "whisper-tiny-en"
    } else if profile.total_ram_mb < 12_000 {
        "whisper-base-en"
    } else if profile.total_ram_mb < 24_000 {
        "whisper-small-en"
    } else {
        // Still cap at Small by default; Medium is an explicit, informed choice.
        "whisper-small-en"
    };

    let reason = match recommended {
        "whisper-tiny-en" => format!(
            "Your machine has about {} GB of memory, so Tiny keeps dictation responsive.",
            profile.total_ram_mb / 1024
        ),
        "whisper-base-en" => format!(
            "With about {} GB of memory, Base gives the best speed for its accuracy.",
            profile.total_ram_mb / 1024
        ),
        _ => format!(
            "With about {} GB of memory and {} CPU cores, Small is accurate without slowing you down.",
            profile.total_ram_mb / 1024,
            profile.cpu_cores
        ),
    };

    ModelRecommendation {
        recommended: recommended.to_string(),
        fastest: "whisper-tiny-en".to_string(),
        accurate: if profile.total_ram_mb >= 24_000 {
            "whisper-medium".to_string()
        } else {
            "whisper-small-en".to_string()
        },
        reason,
    }
}

/* ------------------------------------------------------------ status/logic */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub tier: ModelTier,
    pub size_bytes: u64,
    pub languages: String,
    pub speed: u8,
    pub accuracy: u8,
    pub ram_mb: u32,
    pub description: String,
    pub status: ModelStatus,
    pub progress: f32,
    pub downloaded_bytes: u64,
    pub path: Option<String>,
    pub recommended: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelProgressEvent {
    pub id: String,
    pub status: ModelStatus,
    pub progress: f32,
    pub downloaded_bytes: u64,
    pub error: Option<String>,
}

/// Per-model runtime state, kept out of the immutable catalogue.
#[derive(Debug, Default, Clone)]
struct RuntimeState {
    downloaded_bytes: u64,
    status: Option<ModelStatus>,
    error: Option<String>,
    pause: Option<Arc<AtomicBool>>,
    cancel: Option<Arc<AtomicBool>>,
}

type ProgressSink = Arc<dyn Fn(ModelProgressEvent) + Send + Sync>;

/// Owns the models directory and the per-model runtime state. Cloning shares
/// both, so a download task sees the same progress map as the UI.
#[derive(Clone)]
pub struct ModelManager {
    inner: Arc<ManagerInner>,
}

struct ManagerInner {
    dir: PathBuf,
    runtime: Mutex<HashMap<String, RuntimeState>>,
}

impl ModelManager {
    pub fn new(models_dir: impl AsRef<Path>) -> Self {
        ModelManager {
            inner: Arc::new(ManagerInner {
                dir: models_dir.as_ref().to_path_buf(),
                runtime: Mutex::new(HashMap::new()),
            }),
        }
    }

    pub fn directory(&self) -> &Path {
        &self.inner.dir
    }

    /// Absolute path of an installed model, whether or not it exists yet.
    pub fn path_for(&self, id: &str) -> Result<PathBuf> {
        let spec = spec(id).ok_or_else(|| Error::Store(format!("unknown model {id}")))?;
        Ok(self.inner.dir.join(spec.file_name))
    }

    fn part_path(&self, id: &str) -> Result<PathBuf> {
        Ok(self.path_for(id)?.with_extension("bin.part"))
    }

    fn state(&self, id: &str) -> RuntimeState {
        self.inner
            .runtime
            .lock()
            .ok()
            .and_then(|m| m.get(id).cloned())
            .unwrap_or_default()
    }

    fn update_state(&self, id: &str, f: impl FnOnce(&mut RuntimeState)) {
        if let Ok(mut map) = self.inner.runtime.lock() {
            f(map.entry(id.to_string()).or_default());
        }
    }

    pub fn is_ready(&self, id: &str) -> bool {
        matches!(self.state(id).status, Some(ModelStatus::Ready))
    }

    /// The catalogue with live status applied, ready for the Models screen.
    pub fn list(&self, recommended_id: &str) -> Vec<ModelInfo> {
        CATALOG
            .iter()
            .map(|spec| {
                let path = self.inner.dir.join(spec.file_name);
                let part = self.part_path(spec.id).ok();
                let runtime = self.state(spec.id);

                let on_disk = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                let partial = part
                    .as_ref()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .map(|m| m.len())
                    .unwrap_or(0);

                let installed = on_disk == spec.size_bytes && on_disk > 0;
                let status = match runtime.status {
                    Some(s) if !matches!(s, ModelStatus::NotDownloaded) => s,
                    _ if installed => ModelStatus::Ready,
                    _ if partial > 0 => ModelStatus::Paused,
                    _ => ModelStatus::NotDownloaded,
                };

                let downloaded_bytes = match status {
                    ModelStatus::Ready => spec.size_bytes,
                    ModelStatus::Downloading | ModelStatus::Paused | ModelStatus::Verifying => {
                        partial
                    }
                    _ => 0,
                };

                ModelInfo {
                    id: spec.id.to_string(),
                    name: spec.tier.label().to_string(),
                    tier: spec.tier,
                    size_bytes: spec.size_bytes,
                    languages: spec.languages.to_string(),
                    speed: spec.speed,
                    accuracy: spec.accuracy,
                    ram_mb: spec.ram_mb,
                    description: spec.description.to_string(),
                    status,
                    progress: if spec.size_bytes == 0 {
                        0.0
                    } else {
                        (downloaded_bytes as f32 / spec.size_bytes as f32).clamp(0.0, 1.0)
                    },
                    downloaded_bytes,
                    path: installed.then(|| path.to_string_lossy().to_string()),
                    recommended: spec.id == recommended_id,
                    error: runtime.error,
                }
            })
            .collect()
    }

    /// Total bytes occupied by installed models.
    pub fn disk_usage(&self) -> u64 {
        CATALOG
            .iter()
            .map(|spec| {
                std::fs::metadata(self.inner.dir.join(spec.file_name))
                    .map(|m| m.len())
                    .unwrap_or(0)
            })
            .sum()
    }

    /// Ask an in-flight download to stop; the partial file is kept for resume.
    pub fn pause(&self, id: &str) {
        if let Some(flag) = self.state(id).pause {
            flag.store(true, Ordering::SeqCst);
        }
        self.update_state(id, |s| {
            if matches!(s.status, Some(ModelStatus::Downloading)) {
                s.status = Some(ModelStatus::Paused);
            }
        });
    }

    /// Reset the pause flag so `resume` can continue.
    pub fn resume(&self, id: &str) {
        if let Some(flag) = self.state(id).pause {
            flag.store(false, Ordering::SeqCst);
        }
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        if let Some(flag) = self.state(id).cancel {
            flag.store(true, Ordering::SeqCst);
        }
        for path in [self.path_for(id)?, self.part_path(id)?] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(Error::Io(e.to_string())),
            }
        }
        self.update_state(id, |s| {
            *s = RuntimeState::default();
        });
        Ok(())
    }

    /// Verify an installed model without downloading it.
    pub fn verify(&self, id: &str) -> Result<bool> {
        let spec = spec(id).ok_or_else(|| Error::Store(format!("unknown model {id}")))?;
        let path = self.path_for(id)?;
        let bytes = std::fs::read(&path).map_err(|_| Error::Integrity)?;
        verify_bytes(spec, &bytes)
    }

    /// Download (or resume) a model, reporting progress.
    ///
    /// Returns the path of the installed file. Refuses to touch the network in
    /// Local Only mode.
    pub async fn download(
        &self,
        id: &str,
        local_only: bool,
        progress: ProgressSink,
    ) -> Result<PathBuf> {
        if local_only {
            return Err(Error::LocalOnly);
        }
        let spec = spec(id).ok_or_else(|| Error::Store(format!("unknown model {id}")))?;
        std::fs::create_dir_all(&self.inner.dir)?;

        let final_path = self.path_for(id)?;
        let part_path = self.part_path(id)?;

        // Already installed and valid: nothing to do.
        if let Ok(bytes) = std::fs::read(&final_path) {
            if verify_bytes(spec, &bytes).unwrap_or(false) {
                self.update_state(id, |s| {
                    s.status = Some(ModelStatus::Ready);
                    s.downloaded_bytes = spec.size_bytes;
                    s.error = None;
                });
                emit(
                    &progress,
                    id,
                    ModelStatus::Ready,
                    1.0,
                    spec.size_bytes,
                    None,
                );
                return Ok(final_path);
            }
        }

        let pause = Arc::new(AtomicBool::new(false));
        let cancel = Arc::new(AtomicBool::new(false));
        self.update_state(id, |s| {
            s.pause = Some(Arc::clone(&pause));
            s.cancel = Some(Arc::clone(&cancel));
            s.status = Some(ModelStatus::Downloading);
            s.error = None;
        });

        let mut already = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        if already >= spec.size_bytes {
            // A stale complete-but-unverified part: keep it and re-verify.
            already = 0;
            let _ = std::fs::remove_file(&part_path);
        }

        emit(
            &progress,
            id,
            ModelStatus::Downloading,
            already as f32 / spec.size_bytes as f32,
            already,
            None,
        );

        let client = reqwest::Client::builder()
            .user_agent(format!(
                "{}/{}",
                crate::identity::APP_NAME,
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| Error::Io(e.to_string()))?;

        let mut request = client.get(spec.url);
        if already > 0 {
            // Resume where the previous attempt stopped.
            request = request.header("Range", format!("bytes={already}-"));
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Io(format!("download failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            self.fail(id, &progress, format!("server returned {status}"));
            return Err(Error::Io(format!("server returned {status}")));
        }

        // A 200 means the server ignored our Range header, so start over.
        if already > 0 && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            already = 0;
            let _ = std::fs::remove_file(&part_path);
        }

        use tokio::io::AsyncWriteExt;
        let mut file = if already > 0 {
            tokio::fs::OpenOptions::new()
                .append(true)
                .open(&part_path)
                .await?
        } else {
            tokio::fs::File::create(&part_path).await?
        };

        let mut written = already;
        let mut last_emit = 0u64;
        let mut stream = response.bytes_stream();

        use futures_util::StreamExt;
        while let Some(chunk) = stream.next().await {
            if cancel.load(Ordering::SeqCst) {
                file.flush().await?;
                self.update_state(id, |s| s.status = Some(ModelStatus::Paused));
                return Err(Error::Cancelled);
            }
            if pause.load(Ordering::SeqCst) {
                file.flush().await?;
                self.update_state(id, |s| {
                    s.status = Some(ModelStatus::Paused);
                    s.downloaded_bytes = written;
                });
                emit(
                    &progress,
                    id,
                    ModelStatus::Paused,
                    written as f32 / spec.size_bytes as f32,
                    written,
                    None,
                );
                return Err(Error::Cancelled);
            }

            let chunk = chunk.map_err(|e| Error::Io(format!("download interrupted: {e}")))?;
            file.write_all(&chunk).await?;
            written += chunk.len() as u64;

            // Throttle UI updates to roughly one per 256 KB.
            if written - last_emit > 262_144 {
                last_emit = written;
                self.update_state(id, |s| s.downloaded_bytes = written);
                emit(
                    &progress,
                    id,
                    ModelStatus::Downloading,
                    written as f32 / spec.size_bytes as f32,
                    written,
                    None,
                );
            }
        }
        file.flush().await?;
        drop(file);

        self.update_state(id, |s| s.status = Some(ModelStatus::Verifying));
        emit(&progress, id, ModelStatus::Verifying, 1.0, written, None);

        let bytes = tokio::fs::read(&part_path).await?;
        match verify_bytes(spec, &bytes) {
            Ok(true) => {
                tokio::fs::rename(&part_path, &final_path).await?;
                self.update_state(id, |s| {
                    s.status = Some(ModelStatus::Ready);
                    s.downloaded_bytes = spec.size_bytes;
                    s.error = None;
                    s.pause = None;
                    s.cancel = None;
                });
                emit(
                    &progress,
                    id,
                    ModelStatus::Ready,
                    1.0,
                    spec.size_bytes,
                    None,
                );
                Ok(final_path)
            }
            _ => {
                // Keep the bytes so a user can retry, but never mark them ready.
                self.fail(id, &progress, "integrity check failed".into());
                Err(Error::Integrity)
            }
        }
    }

    fn fail(&self, id: &str, progress: &ProgressSink, message: String) {
        self.update_state(id, |s| {
            s.status = Some(ModelStatus::Error);
            s.error = Some(message.clone());
        });
        emit(progress, id, ModelStatus::Error, 0.0, 0, Some(message));
    }
}

fn emit(
    sink: &ProgressSink,
    id: &str,
    status: ModelStatus,
    progress: f32,
    downloaded_bytes: u64,
    error: Option<String>,
) {
    sink(ModelProgressEvent {
        id: id.to_string(),
        status,
        progress: progress.clamp(0.0, 1.0),
        downloaded_bytes,
        error,
    });
}

/// Validate downloaded bytes before they can be loaded as a model.
///
/// Three independent checks, because they fail in different ways: the exact
/// published length, the GGML magic number, and (when the catalogue pins one)
/// the SHA-256 digest.
pub fn verify_bytes(spec: &ModelSpec, bytes: &[u8]) -> Result<bool> {
    if bytes.len() as u64 != spec.size_bytes {
        return Ok(false);
    }
    // GGML files begin with one of the magic numbers from ggml.c. Checking this
    // catches an HTML error page saved by a proxy.
    const MAGICS: [[u8; 4]; 4] = [
        [0x67, 0x67, 0x6d, 0x6c], // "ggml"
        [0x67, 0x67, 0x6a, 0x74], // "ggjt"
        [0x67, 0x67, 0x6d, 0x66], // "ggmf"
        [0x6c, 0x6d, 0x67, 0x67], // "lmgg"
    ];
    if bytes.len() < 4 || !MAGICS.iter().any(|m| bytes[..4] == *m) {
        return Ok(false);
    }
    if let Some(expected) = spec.sha256 {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        let actual = format!("{:x}", hasher.finalize());
        return Ok(actual == expected);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(ram_mb: u64, cores: u32) -> SystemProfile {
        SystemProfile {
            os: "macos".into(),
            arch: "aarch64".into(),
            cpu_cores: cores,
            total_ram_mb: ram_mb,
            free_disk_mb: 100_000,
            has_gpu: true,
            accelerator: "metal".into(),
        }
    }

    #[test]
    fn catalog_is_internally_consistent() {
        let mut ids = std::collections::HashSet::new();
        for spec in CATALOG {
            assert!(ids.insert(spec.id), "duplicate model id {}", spec.id);
            assert!(spec.size_bytes > 1_000_000, "{} size looks wrong", spec.id);
            assert!(
                spec.url.starts_with("https://"),
                "{} must use https",
                spec.id
            );
            assert!((1..=5).contains(&spec.speed));
            assert!((1..=5).contains(&spec.accuracy));
            assert!(spec.ram_mb > 0);
            assert!(spec.file_name.ends_with(".bin"));
        }
        assert!(spec(DEFAULT_MODEL_ID).is_some(), "default model must exist");
    }

    #[test]
    fn recommendation_scales_with_memory() {
        assert_eq!(recommend(&profile(4_096, 8)).recommended, "whisper-tiny-en");
        assert_eq!(recommend(&profile(8_192, 8)).recommended, "whisper-base-en");
        assert_eq!(
            recommend(&profile(16_384, 10)).recommended,
            "whisper-small-en"
        );
        assert_eq!(
            recommend(&profile(64_000, 16)).recommended,
            "whisper-small-en"
        );
    }

    #[test]
    fn recommendation_never_silently_picks_the_largest_model() {
        // Even on a very large machine, Medium is offered only as an option.
        let rec = recommend(&profile(128_000, 32));
        assert_ne!(rec.recommended, "whisper-medium");
        assert_eq!(rec.accurate, "whisper-medium");
        assert!(!rec.reason.is_empty());
    }

    #[test]
    fn list_reports_not_downloaded_for_a_fresh_directory() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path());
        let list = manager.list(DEFAULT_MODEL_ID);
        assert_eq!(list.len(), CATALOG.len());
        assert!(list.iter().all(|m| m.status == ModelStatus::NotDownloaded));
        assert!(list.iter().all(|m| m.progress == 0.0));
        assert_eq!(list.iter().filter(|m| m.recommended).count(), 1);
        assert_eq!(manager.disk_usage(), 0);
    }

    #[test]
    fn unknown_model_ids_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path());
        assert!(manager.path_for("nope").is_err());
        assert!(manager.verify("nope").is_err());
        assert!(manager.delete("nope").is_err());
    }

    #[test]
    fn verification_rejects_wrong_length_magic_and_digest() {
        let spec = &CATALOG[0];

        // Correct length, wrong magic (an HTML error page).
        let mut html = vec![b'<'];
        html.resize(spec.size_bytes as usize, b'x');
        assert!(!verify_bytes(spec, &html).unwrap());

        // Correct magic, wrong length.
        let mut short = b"ggml".to_vec();
        short.resize(1024, 0);
        assert!(!verify_bytes(spec, &short).unwrap());

        // Correct length and magic, no pinned digest: accepted.
        let mut plausible = b"ggml".to_vec();
        plausible.resize(spec.size_bytes as usize, 0);
        assert!(verify_bytes(spec, &plausible).unwrap());

        // With a pinned digest, the same bytes must now be rejected.
        let mut pinned = spec.clone();
        pinned.sha256 = Some("0000000000000000000000000000000000000000000000000000000000000000");
        assert!(!verify_bytes(&pinned, &plausible).unwrap());
    }

    #[test]
    fn pause_and_delete_are_safe_without_a_download() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path());
        // Must not panic when nothing is running.
        manager.pause("whisper-base-en");
        manager.resume("whisper-base-en");
        assert!(!manager.is_ready("whisper-base-en"));
        manager.delete("whisper-base-en").unwrap();
    }

    #[test]
    fn listed_status_follows_the_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path());
        let spec = &CATALOG[0];

        // A complete file is reported Ready.
        std::fs::write(
            dir.path().join(spec.file_name),
            vec![0u8; spec.size_bytes as usize],
        )
        .unwrap();
        let list = manager.list(DEFAULT_MODEL_ID);
        let entry = list.iter().find(|m| m.id == spec.id).unwrap();
        assert_eq!(entry.status, ModelStatus::Ready);
        assert_eq!(entry.progress, 1.0);
        assert!(entry.path.is_some());
        assert_eq!(manager.disk_usage(), spec.size_bytes);

        // A partial download is reported Paused, not Ready.
        let other = &CATALOG[1];
        std::fs::write(
            dir.path().join(format!("{}.part", other.file_name)),
            vec![0u8; 1_000],
        )
        .unwrap();
        let list = manager.list(DEFAULT_MODEL_ID);
        let entry = list.iter().find(|m| m.id == other.id).unwrap();
        assert_eq!(entry.status, ModelStatus::Paused);
        assert!(entry.progress > 0.0 && entry.progress < 1.0);
        assert!(entry.path.is_none());
    }

    #[tokio::test]
    async fn download_refuses_to_run_in_local_only_mode() {
        let dir = tempfile::tempdir().unwrap();
        let manager = ModelManager::new(dir.path());
        let err = manager
            .download("whisper-base-en", true, Arc::new(|_| {}))
            .await
            .expect_err("local-only must block downloads");
        assert!(matches!(err, Error::LocalOnly));
        // And nothing was written.
        assert_eq!(manager.disk_usage(), 0);
    }
}
