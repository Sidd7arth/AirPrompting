//! Local storage (spec §69).
//!
//! One SQLite database holds every persisted artefact: settings, history, the
//! custom dictionary, user commands and per-application profiles. Raw audio is
//! deliberately absent — it is never written to disk unless the user opts in,
//! and even then it is a separate file, not a database row.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::settings::Settings;

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS kv (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS history (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    created_at     INTEGER NOT NULL,
    duration_ms    INTEGER NOT NULL DEFAULT 0,
    app_name       TEXT    NOT NULL DEFAULT '',
    transcript     TEXT    NOT NULL,
    mode           TEXT    NOT NULL DEFAULT 'standard',
    model_id       TEXT    NOT NULL DEFAULT '',
    enhanced       INTEGER NOT NULL DEFAULT 0,
    command_intent TEXT
);
CREATE INDEX IF NOT EXISTS history_created_at ON history (created_at DESC);
CREATE INDEX IF NOT EXISTS history_app ON history (app_name);

CREATE TABLE IF NOT EXISTS dictionary (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    spoken      TEXT    NOT NULL,
    replacement TEXT    NOT NULL,
    whole_word  INTEGER NOT NULL DEFAULT 1,
    enabled     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS commands (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    triggers              TEXT NOT NULL,
    steps                 TEXT NOT NULL,
    enabled               INTEGER NOT NULL DEFAULT 1,
    requires_confirmation INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS profiles (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    app_pattern         TEXT NOT NULL UNIQUE,
    insert_mode         TEXT,
    enhancement_mode    TEXT,
    enhancement_enabled INTEGER,
    enabled             INTEGER NOT NULL DEFAULT 1
);

-- Aggregate latency samples for the diagnostics screen (spec §98).
-- Only timings and stage names are stored, never transcript content.
CREATE TABLE IF NOT EXISTS usage_samples (
    id     INTEGER PRIMARY KEY AUTOINCREMENT,
    at     INTEGER NOT NULL,
    stage  TEXT    NOT NULL,
    millis REAL    NOT NULL
);
CREATE INDEX IF NOT EXISTS usage_stage ON usage_samples (stage, at DESC);
"#;

/* ----------------------------------------------------------------- models */

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: i64,
    pub created_at: i64,
    pub duration_ms: i64,
    pub app_name: String,
    pub transcript: String,
    pub mode: String,
    pub model_id: String,
    pub enhanced: bool,
    pub command_intent: Option<String>,
}

/// Fields supplied when recording a new dictation. `id` is assigned by SQLite.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewHistoryEntry {
    pub duration_ms: i64,
    pub app_name: String,
    pub transcript: String,
    pub mode: String,
    pub model_id: String,
    pub enhanced: bool,
    pub command_intent: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub mode: Option<String>,
    pub app_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DictionaryEntry {
    pub id: i64,
    pub spoken: String,
    pub replacement: String,
    pub whole_word: bool,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandIntent {
    OpenApp,
    OpenUrl,
    OpenFile,
    OpenFolder,
    RunShortcut,
    CopyText,
    SearchWeb,
    Dictate,
    StopDictation,
    ChangeModel,
    OpenSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionStep {
    pub intent: CommandIntent,
    pub target: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCommand {
    pub id: String,
    pub name: String,
    pub triggers: Vec<String>,
    pub steps: Vec<ActionStep>,
    pub enabled: bool,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppProfile {
    pub id: i64,
    pub app_pattern: String,
    pub insert_mode: Option<crate::platform::InsertMode>,
    pub enhancement_mode: Option<crate::settings::EnhancementMode>,
    pub enhancement_enabled: Option<bool>,
    pub enabled: bool,
}

impl AppProfile {
    /// Does this profile apply to `app_name`? Matching is a case-insensitive
    /// substring test so "Code" covers "Visual Studio Code" (spec §67).
    pub fn matches(&self, app_name: &str) -> bool {
        if !self.enabled || self.app_pattern.trim().is_empty() {
            return false;
        }
        app_name
            .to_lowercase()
            .contains(&self.app_pattern.trim().to_lowercase())
    }
}

/* ---------------------------------------------------------------- database */

/// Escape the `LIKE` metacharacters so a search term is matched literally.
fn escape_like(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

pub struct Database {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Database {
    /// Open (creating if needed) the database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(&path)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Database {
            conn: Mutex::new(conn),
            path,
        })
    }

    /// In-memory database, used by tests.
    pub fn in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(SCHEMA)?;
        Ok(Database {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>> {
        self.conn
            .lock()
            .map_err(|_| Error::Store("database lock poisoned".into()))
    }

    /* ------------------------------------------------------------ settings */

    pub fn load_settings(&self) -> Result<Settings> {
        let conn = self.conn()?;
        let raw: Option<String> = conn
            .query_row("SELECT value FROM kv WHERE key = 'settings'", [], |row| {
                row.get(0)
            })
            .optional()?;
        match raw {
            // A corrupt blob must not brick the app: fall back to defaults.
            Some(json) => Ok(serde_json::from_str::<Settings>(&json)
                .unwrap_or_default()
                .normalized()),
            None => Ok(Settings::default()),
        }
    }

    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let json = serde_json::to_string(settings)?;
        self.conn()?.execute(
            "INSERT INTO kv (key, value) VALUES ('settings', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json],
        )?;
        Ok(())
    }

    /* ------------------------------------------------------------- history */

    pub fn insert_history(&self, entry: &NewHistoryEntry) -> Result<HistoryEntry> {
        let now = crate::now_ms();
        let conn = self.conn()?;
        conn.execute(
            "INSERT INTO history
                (created_at, duration_ms, app_name, transcript, mode, model_id, enhanced, command_intent)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                now,
                entry.duration_ms,
                entry.app_name,
                entry.transcript,
                entry.mode,
                entry.model_id,
                entry.enhanced as i32,
                entry.command_intent,
            ],
        )?;
        Ok(HistoryEntry {
            id: conn.last_insert_rowid(),
            created_at: now,
            duration_ms: entry.duration_ms,
            app_name: entry.app_name.clone(),
            transcript: entry.transcript.clone(),
            mode: entry.mode.clone(),
            model_id: entry.model_id.clone(),
            enhanced: entry.enhanced,
            command_intent: entry.command_intent.clone(),
        })
    }

    pub fn list_history(&self, query: &HistoryQuery) -> Result<Vec<HistoryEntry>> {
        let limit = query.limit.unwrap_or(200).min(1_000) as i64;
        let offset = query.offset.unwrap_or(0) as i64;

        // Build the WHERE clause from whichever filters are present, binding
        // values positionally so nothing is interpolated into the SQL text.
        let mut clauses: Vec<String> = Vec::new();
        let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(search) = query.search.as_ref().filter(|s| !s.trim().is_empty()) {
            // `%` and `_` are LIKE metacharacters; a user searching for "100%"
            // must match that literal text rather than everything.
            clauses.push("(transcript LIKE ? ESCAPE '\\' OR app_name LIKE ? ESCAPE '\\')".into());
            let like = format!("%{}%", escape_like(search.trim()));
            args.push(Box::new(like.clone()));
            args.push(Box::new(like));
        }
        if let Some(mode) = query.mode.as_ref().filter(|m| !m.is_empty()) {
            clauses.push("mode = ?".into());
            args.push(Box::new(mode.clone()));
        }
        if let Some(app) = query.app_name.as_ref().filter(|a| !a.is_empty()) {
            clauses.push("app_name = ?".into());
            args.push(Box::new(app.clone()));
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", clauses.join(" AND "))
        };

        let sql = format!(
            "SELECT id, created_at, duration_ms, app_name, transcript, mode, model_id, enhanced, command_intent
             FROM history {where_clause}
             ORDER BY created_at DESC, id DESC
             LIMIT {limit} OFFSET {offset}"
        );

        let conn = self.conn()?;
        let mut stmt = conn.prepare(&sql)?;
        let refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(refs.as_slice(), |row| {
            Ok(HistoryEntry {
                id: row.get(0)?,
                created_at: row.get(1)?,
                duration_ms: row.get(2)?,
                app_name: row.get(3)?,
                transcript: row.get(4)?,
                mode: row.get(5)?,
                model_id: row.get(6)?,
                enhanced: row.get::<_, i32>(7)? != 0,
                command_intent: row.get(8)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn latest_history(&self) -> Result<Option<HistoryEntry>> {
        Ok(self
            .list_history(&HistoryQuery {
                limit: Some(1),
                ..Default::default()
            })?
            .into_iter()
            .next())
    }

    pub fn delete_history(&self, id: i64) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_all_history(&self) -> Result<()> {
        self.conn()?.execute("DELETE FROM history", [])?;
        Ok(())
    }

    /// Drop entries older than `days`. `days == 0` means "never delete".
    pub fn prune_history(&self, days: u32) -> Result<usize> {
        if days == 0 {
            return Ok(0);
        }
        let cutoff = crate::now_ms() - (days as i64) * 86_400_000;
        let removed = self
            .conn()?
            .execute("DELETE FROM history WHERE created_at < ?1", params![cutoff])?;
        Ok(removed)
    }

    pub fn history_count(&self) -> Result<i64> {
        Ok(self
            .conn()?
            .query_row("SELECT COUNT(*) FROM history", [], |row| row.get(0))?)
    }

    /* ---------------------------------------------------------- dictionary */

    pub fn list_dictionary(&self) -> Result<Vec<DictionaryEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, spoken, replacement, whole_word, enabled FROM dictionary ORDER BY LENGTH(spoken) DESC, id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DictionaryEntry {
                id: row.get(0)?,
                spoken: row.get(1)?,
                replacement: row.get(2)?,
                whole_word: row.get::<_, i32>(3)? != 0,
                enabled: row.get::<_, i32>(4)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Insert or update by id. Returns the stored row.
    pub fn upsert_dictionary(&self, entry: &DictionaryEntry) -> Result<DictionaryEntry> {
        let conn = self.conn()?;
        let id = if entry.id > 0 {
            let changed = conn.execute(
                "UPDATE dictionary SET spoken = ?2, replacement = ?3, whole_word = ?4, enabled = ?5 WHERE id = ?1",
                params![
                    entry.id,
                    entry.spoken,
                    entry.replacement,
                    entry.whole_word as i32,
                    entry.enabled as i32
                ],
            )?;
            if changed == 0 {
                return Err(Error::Store(format!(
                    "dictionary entry {} not found",
                    entry.id
                )));
            }
            entry.id
        } else {
            conn.execute(
                "INSERT INTO dictionary (spoken, replacement, whole_word, enabled) VALUES (?1, ?2, ?3, ?4)",
                params![
                    entry.spoken,
                    entry.replacement,
                    entry.whole_word as i32,
                    entry.enabled as i32
                ],
            )?;
            conn.last_insert_rowid()
        };
        Ok(DictionaryEntry {
            id,
            ..entry.clone()
        })
    }

    pub fn delete_dictionary(&self, id: i64) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM dictionary WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* ------------------------------------------------------------ commands */

    pub fn list_commands(&self) -> Result<Vec<CustomCommand>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, name, triggers, steps, enabled, requires_confirmation FROM commands ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            let triggers: String = row.get(2)?;
            let steps: String = row.get(3)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                triggers,
                steps,
                row.get::<_, i32>(4)? != 0,
                row.get::<_, i32>(5)? != 0,
            ))
        })?;

        let mut out = Vec::new();
        for row in rows {
            let (id, name, triggers, steps, enabled, requires_confirmation) = row?;
            // A malformed row is skipped rather than failing the whole listing.
            let triggers: Vec<String> = serde_json::from_str(&triggers).unwrap_or_default();
            let steps: Vec<ActionStep> = serde_json::from_str(&steps).unwrap_or_default();
            out.push(CustomCommand {
                id,
                name,
                triggers,
                steps,
                enabled,
                requires_confirmation,
            });
        }
        Ok(out)
    }

    pub fn upsert_command(&self, command: &CustomCommand) -> Result<CustomCommand> {
        let triggers = serde_json::to_string(&command.triggers)?;
        let steps = serde_json::to_string(&command.steps)?;
        self.conn()?.execute(
            "INSERT INTO commands (id, name, triggers, steps, enabled, requires_confirmation)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                triggers = excluded.triggers,
                steps = excluded.steps,
                enabled = excluded.enabled,
                requires_confirmation = excluded.requires_confirmation",
            params![
                command.id,
                command.name,
                triggers,
                steps,
                command.enabled as i32,
                command.requires_confirmation as i32
            ],
        )?;
        Ok(command.clone())
    }

    pub fn delete_command(&self, id: &str) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM commands WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* ------------------------------------------------------------ profiles */

    pub fn list_profiles(&self) -> Result<Vec<AppProfile>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, app_pattern, insert_mode, enhancement_mode, enhancement_enabled, enabled
             FROM profiles ORDER BY app_pattern",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AppProfile {
                id: row.get(0)?,
                app_pattern: row.get(1)?,
                insert_mode: row
                    .get::<_, Option<String>>(2)?
                    .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
                enhancement_mode: row
                    .get::<_, Option<String>>(3)?
                    .and_then(|s| serde_json::from_value(serde_json::Value::String(s)).ok()),
                enhancement_enabled: row.get::<_, Option<i32>>(4)?.map(|v| v != 0),
                enabled: row.get::<_, i32>(5)? != 0,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// The most specific enabled profile matching `app_name`, if any.
    pub fn profile_for(&self, app_name: &str) -> Result<Option<AppProfile>> {
        Ok(self
            .list_profiles()?
            .into_iter()
            .filter(|p| p.matches(app_name))
            // Longest pattern wins, so "Visual Studio Code" beats "Code".
            .max_by_key(|p| p.app_pattern.len()))
    }

    pub fn upsert_profile(&self, profile: &AppProfile) -> Result<AppProfile> {
        let insert_mode = profile.insert_mode.as_ref().map(|m| {
            serde_json::to_value(m)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        });
        let enhancement_mode = profile.enhancement_mode.as_ref().map(|m| {
            serde_json::to_value(m)
                .unwrap()
                .as_str()
                .unwrap()
                .to_string()
        });
        let conn = self.conn()?;
        let id = if profile.id > 0 {
            conn.execute(
                "UPDATE profiles SET app_pattern = ?2, insert_mode = ?3, enhancement_mode = ?4,
                    enhancement_enabled = ?5, enabled = ?6 WHERE id = ?1",
                params![
                    profile.id,
                    profile.app_pattern,
                    insert_mode,
                    enhancement_mode,
                    profile.enhancement_enabled.map(|v| v as i32),
                    profile.enabled as i32
                ],
            )?;
            profile.id
        } else {
            conn.execute(
                "INSERT INTO profiles (app_pattern, insert_mode, enhancement_mode, enhancement_enabled, enabled)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(app_pattern) DO UPDATE SET
                    insert_mode = excluded.insert_mode,
                    enhancement_mode = excluded.enhancement_mode,
                    enhancement_enabled = excluded.enhancement_enabled,
                    enabled = excluded.enabled",
                params![
                    profile.app_pattern,
                    insert_mode,
                    enhancement_mode,
                    profile.enhancement_enabled.map(|v| v as i32),
                    profile.enabled as i32
                ],
            )?;
            conn.last_insert_rowid()
        };
        Ok(AppProfile {
            id,
            ..profile.clone()
        })
    }

    pub fn delete_profile(&self, id: i64) -> Result<()> {
        self.conn()?
            .execute("DELETE FROM profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    /* ------------------------------------------------- usage / latency (§98) */

    pub fn record_sample(&self, stage: &str, millis: f64) -> Result<()> {
        self.conn()?.execute(
            "INSERT INTO usage_samples (at, stage, millis) VALUES (?1, ?2, ?3)",
            params![crate::now_ms(), stage, millis],
        )?;
        Ok(())
    }

    /// Median of the most recent `limit` samples for `stage`.
    pub fn median_sample(&self, stage: &str, limit: u32) -> Result<Option<f64>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT millis FROM usage_samples WHERE stage = ?1 ORDER BY at DESC LIMIT ?2",
        )?;
        let mut values: Vec<f64> = stmt
            .query_map(params![stage, limit], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if values.is_empty() {
            return Ok(None);
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        Ok(Some(values[values.len() / 2]))
    }

    pub fn clear_samples(&self) -> Result<()> {
        self.conn()?.execute("DELETE FROM usage_samples", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Database {
        Database::in_memory().unwrap()
    }

    fn entry(text: &str, app: &str) -> NewHistoryEntry {
        NewHistoryEntry {
            transcript: text.into(),
            app_name: app.into(),
            mode: "standard".into(),
            model_id: "whisper-base-en".into(),
            duration_ms: 2_400,
            ..Default::default()
        }
    }

    #[test]
    fn settings_round_trip_and_defaults_when_absent() {
        let db = db();
        assert_eq!(db.load_settings().unwrap(), Settings::default());

        let mut s = Settings::default();
        s.privacy.local_only = true;
        s.speech.model_id = "whisper-small-en".into();
        db.save_settings(&s).unwrap();
        assert_eq!(db.load_settings().unwrap(), s.normalized());
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let db = db();
        db.conn()
            .unwrap()
            .execute(
                "INSERT INTO kv (key, value) VALUES ('settings', 'not json')",
                [],
            )
            .unwrap();
        assert_eq!(db.load_settings().unwrap(), Settings::default());
    }

    #[test]
    fn history_insert_list_search_and_delete() {
        let db = db();
        let a = db
            .insert_history(&entry("implement the auth middleware", "Code"))
            .unwrap();
        db.insert_history(&entry("buy milk", "Notes")).unwrap();
        assert_eq!(db.list_history(&HistoryQuery::default()).unwrap().len(), 2);
        assert_eq!(db.history_count().unwrap(), 2);

        let found = db
            .list_history(&HistoryQuery {
                search: Some("AUTH".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, a.id);

        let by_app = db
            .list_history(&HistoryQuery {
                app_name: Some("Notes".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_app.len(), 1);

        assert_eq!(
            db.latest_history().unwrap().unwrap().id,
            found[0].id.max(a.id)
        );
        db.delete_history(a.id).unwrap();
        assert_eq!(db.history_count().unwrap(), 1);
        db.delete_all_history().unwrap();
        assert_eq!(db.history_count().unwrap(), 0);
        assert!(db.latest_history().unwrap().is_none());
    }

    #[test]
    fn history_search_treats_wildcards_literally() {
        let db = db();
        db.insert_history(&entry("progress is 100% done", "Notes"))
            .unwrap();
        // Contains the digits, but not a literal '%' after them.
        db.insert_history(&entry("100 done", "Notes")).unwrap();
        db.insert_history(&entry("under_score", "Notes")).unwrap();

        let found = db
            .list_history(&HistoryQuery {
                search: Some("100%".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(found.len(), 1, "'%' must match a literal percent sign");

        let underscored = db
            .list_history(&HistoryQuery {
                search: Some("under_".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(underscored.len(), 1, "'_' must match a literal underscore");

        // A bare wildcard must not select every row.
        let everything = db
            .list_history(&HistoryQuery {
                search: Some("%".into()),
                ..Default::default()
            })
            .unwrap();
        assert!(everything.is_empty());
    }

    #[test]
    fn prune_respects_never_and_cutoff() {
        let db = db();
        db.insert_history(&entry("old", "Notes")).unwrap();
        db.conn()
            .unwrap()
            .execute(
                "UPDATE history SET created_at = ?1",
                params![crate::now_ms() - 40 * 86_400_000],
            )
            .unwrap();
        db.insert_history(&entry("new", "Notes")).unwrap();

        assert_eq!(db.prune_history(0).unwrap(), 0);
        assert_eq!(db.history_count().unwrap(), 2);
        assert_eq!(db.prune_history(30).unwrap(), 1);
        assert_eq!(db.history_count().unwrap(), 1);
    }

    #[test]
    fn dictionary_crud() {
        let db = db();
        let created = db
            .upsert_dictionary(&DictionaryEntry {
                id: 0,
                spoken: "next mart".into(),
                replacement: "NxtMart".into(),
                whole_word: true,
                enabled: true,
            })
            .unwrap();
        assert!(created.id > 0);

        let listed = db.list_dictionary().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].replacement, "NxtMart");

        db.upsert_dictionary(&DictionaryEntry {
            replacement: "EduRox".into(),
            ..created.clone()
        })
        .unwrap();
        assert_eq!(db.list_dictionary().unwrap()[0].replacement, "EduRox");

        assert!(db
            .upsert_dictionary(&DictionaryEntry {
                id: 9_999,
                spoken: "x".into(),
                replacement: "y".into(),
                whole_word: true,
                enabled: true,
            })
            .is_err());

        db.delete_dictionary(created.id).unwrap();
        assert!(db.list_dictionary().unwrap().is_empty());
    }

    #[test]
    fn commands_round_trip_composite_fields() {
        let db = db();
        let command = CustomCommand {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Start Coding".into(),
            triggers: vec!["start coding".into(), "coding mode".into()],
            steps: vec![
                ActionStep {
                    intent: CommandIntent::OpenApp,
                    target: "Visual Studio Code".into(),
                },
                ActionStep {
                    intent: CommandIntent::OpenFolder,
                    target: "~/Projects/my-app".into(),
                },
            ],
            enabled: true,
            requires_confirmation: false,
        };
        db.upsert_command(&command).unwrap();
        let listed = db.list_commands().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].triggers, command.triggers);
        assert_eq!(listed[0].steps, command.steps);

        db.delete_command(&command.id).unwrap();
        assert!(db.list_commands().unwrap().is_empty());
    }

    #[test]
    fn profile_matching_prefers_longest_pattern() {
        let db = db();
        db.upsert_profile(&AppProfile {
            id: 0,
            app_pattern: "Code".into(),
            insert_mode: Some(crate::platform::InsertMode::Type),
            enhancement_mode: None,
            enhancement_enabled: None,
            enabled: true,
        })
        .unwrap();
        db.upsert_profile(&AppProfile {
            id: 0,
            app_pattern: "Visual Studio Code".into(),
            insert_mode: Some(crate::platform::InsertMode::ClipboardOnly),
            enhancement_mode: Some(crate::settings::EnhancementMode::Technical),
            enhancement_enabled: Some(false),
            enabled: true,
        })
        .unwrap();

        let matched = db
            .profile_for("Visual Studio Code — main.rs")
            .unwrap()
            .expect("profile should match");
        assert_eq!(matched.app_pattern, "Visual Studio Code");
        assert_eq!(
            matched.insert_mode,
            Some(crate::platform::InsertMode::ClipboardOnly)
        );
        assert_eq!(
            matched.enhancement_mode,
            Some(crate::settings::EnhancementMode::Technical)
        );

        // A disabled profile must not match at all.
        db.upsert_profile(&AppProfile {
            enabled: false,
            ..matched
        })
        .unwrap();
        assert!(db.profile_for("Visual Studio Code").unwrap().is_none());
    }

    #[test]
    fn latency_samples_report_median() {
        let db = db();
        for value in [100.0, 300.0, 200.0, 1000.0] {
            db.record_sample("insertion", value).unwrap();
        }
        assert_eq!(db.median_sample("insertion", 10).unwrap(), Some(300.0));
        assert_eq!(db.median_sample("unknown", 10).unwrap(), None);
        db.clear_samples().unwrap();
        assert_eq!(db.median_sample("insertion", 10).unwrap(), None);
    }

    #[test]
    fn reopening_the_same_file_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(crate::identity::DATABASE_FILE_NAME);
        {
            let db = Database::open(&path).unwrap();
            db.insert_history(&entry("persist me", "Notes")).unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(db.history_count().unwrap(), 1);
    }
}
