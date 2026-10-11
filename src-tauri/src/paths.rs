//! Where AirPrompt keeps its files, and the one-time migration from an earlier
//! product identity.
//!
//! All four directories are resolved once at startup and handed around inside
//! [`crate::state::AppState`]; no other module calls `dirs::` directly, so
//! there is exactly one place to look when asking "where did that file go?".

use std::path::{Path, PathBuf};

use airprompt_core::error::{Error, Result};
use airprompt_core::{identity, migration};

/// The directory layout for this install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    /// Settings database, logs and the migration marker.
    pub data: PathBuf,
    /// Scratch space: the temporary WAV written for each decode, model parts.
    pub cache: PathBuf,
    /// Downloaded GGML speech models.
    pub models: PathBuf,
    /// Rolling log files.
    pub logs: PathBuf,
}

impl AppPaths {
    /// Resolve every directory, creating them on the way.
    ///
    /// A `models` directory supplied through the environment
    /// ([`identity::ENV_MODELS_DIR`]) wins, so a user can keep 3 GB of models on
    /// a second drive and have the app respect it.
    pub fn resolve() -> Result<Self> {
        let data_parent = dirs::data_dir()
            .ok_or_else(|| Error::Io("this system has no per-user data directory".into()))?;
        let cache_parent = dirs::cache_dir().unwrap_or_else(|| data_parent.clone());

        let data = migration::target_root(&data_parent);
        let cache = cache_parent.join(identity::CACHE_DIR_NAME);
        let models = models_directory(&data);
        let logs = data.join("logs");

        let paths = AppPaths {
            data,
            cache,
            models,
            logs,
        };
        paths.ensure()?;
        Ok(paths)
    }

    fn ensure(&self) -> Result<()> {
        for dir in [&self.data, &self.cache, &self.models, &self.logs] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }

    /// The settings/history database.
    pub fn database(&self) -> PathBuf {
        self.data.join(identity::DATABASE_FILE_NAME)
    }

    /// The rolling log file the Diagnostics screen links to.
    pub fn log_file(&self) -> PathBuf {
        self.logs.join(format!("{}.log", identity::LOG_NAMESPACE))
    }

    /// Save the last transcript here so a failed insertion can be recovered.
    pub fn last_transcript(&self) -> PathBuf {
        self.cache.join("last-transcript.txt")
    }

    /// Directories searched for the bundled `whisper-cli` sidecar, in order.
    pub fn whisper_search_path(&self, resource_dir: Option<&Path>) -> Vec<PathBuf> {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(resource) = resource_dir {
            // Tauri copies `bundle.resources` next to the executable; look in
            // both the root and a `bin/` subdirectory so either layout works.
            dirs.push(resource.join("bin"));
            dirs.push(resource.to_path_buf());
        }
        dirs.push(self.data.join("bin"));
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                dirs.push(parent.join("bin"));
                dirs.push(parent.to_path_buf());
            }
        }
        dirs
    }
}

/// The models directory: an explicit override, else `data/models`.
fn models_directory(data: &Path) -> PathBuf {
    identity::env(identity::ENV_MODELS_DIR, &[identity::LEGACY_ENV_MODELS_DIR])
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| data.join("models"))
}

/// Outcome of the startup migration check, for the diagnostics log line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub ran: bool,
    pub message: String,
}

/// Run the legacy-data migration if it has not already run on this machine.
///
/// Copy-only and never-overwriting; the old directory is left exactly as it
/// was. A failure here is logged and swallowed: refusing to start because an
/// upgrade copy failed would be far worse than starting with default settings.
pub fn migrate_legacy_data(paths: &AppPaths) -> MigrationReport {
    let Some(data_parent) = paths.data.parent() else {
        return MigrationReport {
            ran: false,
            message: "data directory has no parent".into(),
        };
    };
    let legacy = migration::legacy_roots(data_parent);
    match migration::migrate(&legacy, &paths.data) {
        Ok(outcome) => MigrationReport {
            ran: outcome.ran,
            message: outcome.reason,
        },
        Err(error) => {
            tracing::warn!(code = error.code(), "legacy data migration failed");
            MigrationReport {
                ran: false,
                message: "an earlier version's data could not be copied".into(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airprompt_core::models::SystemProfile;
    use tempfile::TempDir;

    #[test]
    fn models_directory_prefers_the_data_path() {
        // No environment override in a test process.
        let dir = models_directory(Path::new("/tmp/data"));
        assert!(dir.ends_with("models"));
    }

    #[test]
    fn whisper_search_path_covers_the_resource_directory_first() {
        let paths = AppPaths {
            data: PathBuf::from("/data/AirPrompt"),
            cache: PathBuf::from("/cache"),
            models: PathBuf::from("/data/AirPrompt/models"),
            logs: PathBuf::from("/data/AirPrompt/logs"),
        };
        let dirs = paths.whisper_search_path(Some(Path::new("/apps/AirPrompt.app/Resources")));
        assert_eq!(
            dirs[0],
            PathBuf::from("/apps/AirPrompt.app/Resources/bin"),
            "the bundled sidecar wins over a PATH lookup"
        );
        assert_eq!(dirs[1], PathBuf::from("/apps/AirPrompt.app/Resources"));
        assert!(dirs.len() >= 3, "the data directory is always a candidate");
    }

    #[test]
    fn database_and_log_names_use_the_central_identity() {
        let paths = AppPaths {
            data: PathBuf::from("/data/AirPrompt"),
            cache: PathBuf::from("/cache"),
            models: PathBuf::from("/models"),
            logs: PathBuf::from("/logs"),
        };
        assert_eq!(
            paths.database().file_name().unwrap().to_str().unwrap(),
            identity::DATABASE_FILE_NAME
        );
        assert_eq!(
            paths.log_file().file_name().unwrap().to_str().unwrap(),
            format!("{}.log", identity::LOG_NAMESPACE)
        );
    }

    #[test]
    fn migration_reports_rather_than_panics_when_there_is_nothing_to_do() {
        let temp = TempDir::new().unwrap();
        let paths = AppPaths {
            data: temp.path().join(identity::DATA_DIR_NAME),
            cache: temp.path().join("cache"),
            models: temp.path().join("models"),
            logs: temp.path().join("logs"),
        };
        std::fs::create_dir_all(&paths.data).unwrap();
        let report = migrate_legacy_data(&paths);
        assert!(!report.ran, "a fresh install must not report a migration");
    }

    #[test]
    fn system_profile_shape_is_what_the_models_screen_expects() {
        // Guards the `recommend()` call site against a field rename in core.
        let profile = SystemProfile {
            os: "windows".into(),
            arch: "x86_64".into(),
            cpu_cores: 8,
            total_ram_mb: 16_384,
            free_disk_mb: 500_000,
            has_gpu: false,
            accelerator: "cpu".into(),
        };
        let json = serde_json::to_value(profile).unwrap();
        assert!(json.get("cpuCores").is_some());
        assert!(json.get("totalRamMb").is_some());
        assert!(json.get("freeDiskMb").is_some());
        assert!(json.get("hasGpu").is_some());
    }
}
