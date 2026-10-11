//! One-time migration of data written under an earlier identity (spec §33).
//!
//! AirPrompt has not always claimed this name. A machine that ran an earlier
//! build — or the reference implementation this product was studied against —
//! may still hold the user's settings, transcription history, dictionary,
//! downloaded models and provider keys under a different directory name.
//!
//! The rules are deliberately conservative, because the data is the user's and
//! there is no second copy:
//!
//! 1. **Detect** — look only in the per-user application data directory, and
//!    only at the known legacy names.
//! 2. **Validate** — a directory is only migrated when it contains a file this
//!    product recognises ([`identity::RECOGNISED_DATA_FILES`]); a directory that
//!    merely shares a name is left alone.
//! 3. **Copy** — never move. The old data stays exactly where it is.
//! 4. **Preserve** — a destination file that already exists is never
//!    overwritten; an existing AirPrompt install always wins.
//! 5. **Write new** — the copy lands under the AirPrompt names, including the
//!    database file, so the app never ends up with two competing databases.
//! 6. **Do not repeat** — a marker file records a completed run.
//! 7. **Log without content** — paths and counts only. Never a transcript,
//!    never a credential value.
//!
//! A failed copy writes no marker, so the next launch retries. Nothing is ever
//! deleted, which is what makes the retry safe.

use crate::error::{Error, Result};
use crate::identity;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Written into the migrated-to directory once a run has completed.
pub const MARKER_FILE: &str = ".migration-complete";

/// What a migration run did. Counts and paths only — never content.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MigrationOutcome {
    /// False when the run was skipped.
    pub ran: bool,
    /// The legacy directory that was read, when one was found.
    pub source: Option<PathBuf>,
    /// Files written into the AirPrompt directory.
    pub files_copied: usize,
    /// Files left untouched because the destination already existed.
    pub files_kept: usize,
    /// The migrated database in its new location, when one was found.
    pub database: Option<PathBuf>,
    /// A short, non-sensitive explanation, safe to put in a log line.
    pub reason: String,
}

/// The directory this build owns, given the parent of the per-user application
/// data directory.
pub fn target_root(data_dir_parent: &Path) -> PathBuf {
    data_dir_parent.join(identity::DATA_DIR_NAME)
}

/// Candidate legacy directories, most recently used first.
pub fn legacy_roots(data_dir_parent: &Path) -> Vec<PathBuf> {
    identity::LEGACY_DATA_DIR_NAMES
        .iter()
        .map(|name| data_dir_parent.join(name))
        .collect()
}

/// True when a completed migration is already recorded for `target`.
pub fn has_run(target: &Path) -> bool {
    target.join(MARKER_FILE).is_file()
}

/// The name a migrated file takes in the AirPrompt directory.
///
/// Only the database is renamed: settings, models and history sidecars keep
/// their own names, which is what lets them survive a copy unchanged.
fn destination_name(file_name: &str) -> String {
    if identity::LEGACY_DATABASE_FILE_NAMES.contains(&file_name) {
        identity::DATABASE_FILE_NAME.to_string()
    } else {
        file_name.to_string()
    }
}

/// True when `dir` holds at least one file this product recognises.
fn is_our_data(dir: &Path) -> bool {
    identity::RECOGNISED_DATA_FILES
        .iter()
        .any(|name| dir.join(name).is_file())
}

/// Copy data left by an earlier identity into `target`, unless that has already
/// been done.
///
/// `legacy` is searched in order and the first valid candidate is used. Returns
/// an outcome with `ran == false` when there is nothing to do — that is a
/// normal result, not an error.
pub fn migrate(legacy: &[PathBuf], target: &Path) -> Result<MigrationOutcome> {
    if has_run(target) {
        debug!(
            target = %target.display(),
            "legacy data migration was already completed; skipping"
        );
        return Ok(MigrationOutcome {
            ran: false,
            reason: "already migrated".into(),
            ..Default::default()
        });
    }

    let mut named: Vec<&PathBuf> = Vec::new();
    for candidate in legacy {
        if candidate.is_dir() {
            named.push(candidate);
        }
    }

    let mut source = None;
    for candidate in &named {
        if is_our_data(candidate) {
            source = Some((*candidate).clone());
            break;
        }
        debug!(
            candidate = %candidate.display(),
            "directory exists but holds no recognised product data; leaving it alone"
        );
    }

    let Some(source) = source else {
        if !named.is_empty() {
            warn!(
                "found {} directory resembling an earlier version, but none held \
                 recognisable data; nothing was migrated",
                named.len()
            );
        }
        return Ok(MigrationOutcome {
            ran: false,
            reason: "no data from an earlier version was found".into(),
            ..Default::default()
        });
    };

    let mut outcome = MigrationOutcome {
        ran: true,
        source: Some(source.clone()),
        reason: format!("copied from {}", source.display()),
        ..Default::default()
    };

    copy_tree(&source, target, target, &mut outcome)?;

    let marker = format!(
        "migrated_from {}\ncompleted_at_ms {}\n",
        source.display(),
        crate::now_ms()
    );
    fs::write(target.join(MARKER_FILE), marker)
        .map_err(|e| Error::Io(format!("could not record the migration: {e}")))?;

    info!(
        copied = outcome.files_copied,
        kept = outcome.files_kept,
        source = %source.display(),
        "migrated data from an earlier version; the previous data was left in place"
    );

    Ok(outcome)
}

/// Recursively copy `from` into `to`, creating directories as needed.
fn copy_tree(from: &Path, to: &Path, target: &Path, outcome: &mut MigrationOutcome) -> Result<()> {
    fs::create_dir_all(to).map_err(|e| Error::Io(format!("could not create {to:?}: {e}")))?;

    let entries =
        fs::read_dir(from).map_err(|e| Error::Io(format!("could not read {from:?}: {e}")))?;

    for entry in entries {
        let entry = entry.map_err(|e| Error::Io(e.to_string()))?;
        let path = entry.path();

        // Never follow a symlink out of the legacy directory: it could point
        // anywhere, and a migration must not copy unrelated files.
        let file_type = entry
            .file_type()
            .map_err(|e| Error::Io(format!("could not inspect {path:?}: {e}")))?;
        if file_type.is_symlink() {
            warn!(path = %path.display(), "skipping a symbolic link during migration");
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();

        if file_type.is_dir() {
            copy_tree(&path, &to.join(name.as_ref()), target, outcome)?;
            continue;
        }

        let destination = to.join(destination_name(&name));
        if destination.exists() {
            outcome.files_kept += 1;
            debug!(
                destination = %destination.display(),
                "destination already exists; leaving it untouched"
            );
            continue;
        }

        fs::copy(&path, &destination).map_err(|e| {
            Error::Io(format!(
                "could not copy {} into {}: {e}",
                path.display(),
                destination.display()
            ))
        })?;

        outcome.files_copied += 1;
        if destination.file_name() == Some(identity::DATABASE_FILE_NAME.as_ref())
            && destination.parent() == Some(target)
            && outcome.database.is_none()
        {
            outcome.database = Some(destination);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn legacy_tree(dir: &Path) {
        fs::create_dir_all(dir.join("models")).unwrap();
        fs::write(dir.join("settings.json"), "{\"theme\":\"dark\"}").unwrap();
        fs::write(dir.join("TranscriptionHistory.sqlite3"), "sqlite").unwrap();
        fs::write(dir.join("models/ggml-base.en.bin"), "weights").unwrap();
        fs::write(dir.join("notes.txt"), "not ours").unwrap();
    }

    fn parent() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    #[test]
    fn migrates_recognised_data_and_keeps_the_original() {
        let parent = parent();
        let from = parent.path().join("FluidVoice");
        legacy_tree(&from);
        let target = target_root(parent.path());

        let outcome = migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert!(outcome.ran);
        assert_eq!(outcome.files_copied, 4);
        assert_eq!(outcome.files_kept, 0);
        assert_eq!(outcome.source.as_deref(), Some(from.as_path()));
        assert!(target.join("settings.json").is_file());
        assert!(target.join("models/ggml-base.en.bin").is_file());
        assert_eq!(
            outcome.database,
            Some(target.join(identity::DATABASE_FILE_NAME))
        );
        // The old data is untouched: every file is still there.
        assert!(from.join("TranscriptionHistory.sqlite3").is_file());
        assert!(from.join("models/ggml-base.en.bin").is_file());
    }

    #[test]
    fn renames_only_the_legacy_database() {
        let parent = parent();
        let from = parent.path().join("Jarvis");
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("jarvis.db"), "sqlite").unwrap();
        let target = target_root(parent.path());

        migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert!(target.join(identity::DATABASE_FILE_NAME).is_file());
        assert!(!target.join("jarvis.db").exists());
    }

    #[test]
    fn never_overwrites_a_file_the_new_install_already_has() {
        let parent = parent();
        let from = parent.path().join("FluidVoice");
        legacy_tree(&from);
        let target = target_root(parent.path());
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("settings.json"), "{\"theme\":\"light\"}").unwrap();
        fs::write(target.join(identity::DATABASE_FILE_NAME), "current").unwrap();

        let outcome = migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert!(outcome.ran);
        assert_eq!(outcome.files_kept, 2);
        assert_eq!(outcome.files_copied, 2);
        assert_eq!(
            fs::read_to_string(target.join("settings.json")).unwrap(),
            "{\"theme\":\"light\"}"
        );
        assert_eq!(
            fs::read_to_string(target.join(identity::DATABASE_FILE_NAME)).unwrap(),
            "current"
        );
        // The database the new install owns is not re-pointed at the old one.
        assert_eq!(outcome.database, None);
    }

    #[test]
    fn a_second_run_does_nothing() {
        let parent = parent();
        legacy_tree(&parent.path().join("FluidVoice"));
        let target = target_root(parent.path());

        assert!(migrate(&legacy_roots(parent.path()), &target).unwrap().ran);

        let marker_after_first = fs::read_to_string(target.join(MARKER_FILE)).unwrap();
        let second = migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert!(!second.ran);
        assert_eq!(second.files_copied, 0);
        assert_eq!(second.reason, "already migrated");
        assert_eq!(
            fs::read_to_string(target.join(MARKER_FILE)).unwrap(),
            marker_after_first
        );
    }

    #[test]
    fn a_directory_without_recognised_data_is_left_alone() {
        let parent = parent();
        let from = parent.path().join("Jarvis");
        fs::create_dir_all(&from).unwrap();
        fs::write(from.join("unrelated.txt"), "someone else's data").unwrap();
        let target = target_root(parent.path());

        let outcome = migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert!(!outcome.ran);
        assert!(!target.exists());
        assert!(!has_run(&target));
    }

    #[test]
    fn nothing_to_migrate_is_not_an_error() {
        let parent = parent();
        let target = target_root(parent.path());

        let outcome = migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert!(!outcome.ran);
        assert_eq!(outcome.source, None);
        assert!(!target.exists());
    }

    #[test]
    fn the_newest_identity_wins_when_several_are_present() {
        let parent = parent();
        legacy_tree(&parent.path().join("FluidVoice"));
        legacy_tree(&parent.path().join("Jarvis"));
        let target = target_root(parent.path());

        let outcome = migrate(&legacy_roots(parent.path()), &target).unwrap();

        assert_eq!(
            outcome.source.as_deref(),
            Some(parent.path().join("Jarvis").as_path())
        );
    }
}
