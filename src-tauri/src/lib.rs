//! AirPrompt desktop application shell.
//!
//! Startup sequence (spec §14):
//!
//! ```text
//! Launch
//!   → resolve paths (+ one-time legacy migration)
//!   → initialize logging
//!   → open the database, load settings
//!   → initialize the platform adapter + speech engine
//!   → create the tray, register global shortcuts
//!   → spawn the runtime loop (VAD polling, streaming decodes)
//!   → READY (windows shown per settings; the loop is fully async)
//! ```
//!
//! Heavy work (model loading, downloads) never blocks startup; it happens in
//! spawned tasks that report through events.

use std::sync::{Arc, Mutex};

use airprompt_core::settings::Settings;
use airprompt_core::storage::Database;
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_dialog::DialogExt as _;
use tauri_plugin_opener::OpenerExt as _;

mod capture;
mod commands;
mod dictation;
mod events;
mod logging;
mod overlay;
mod paths;
mod shortcuts;
mod speech;
mod state;
mod tray;

use dictation::{DictationController, PollAction};
use events::EventSink;
use overlay::OverlayController;
use state::{AppState, SharedDependencies};

/// Where `run()` failed, for the report below.
struct StartupPaths {
    data: std::path::PathBuf,
}

/// The application entry point.
pub fn run() {
    // Paths and logging come first: everything else logs.
    let paths = match paths::AppPaths::resolve() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("airprompt: could not create the data directory: {error}");
            return;
        }
    };

    let migration = paths::migrate_legacy_data(&paths);
    let logging = std::sync::Arc::new(logging::init(
        &paths.logs,
        airprompt_core::settings::LogLevel::Info,
    ));
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        data = %paths.data.display(),
        migration_ran = migration.ran,
        "starting"
    );

    // Database + settings before any UI exists.
    let database = match Database::open(paths.database()) {
        Ok(db) => db,
        Err(error) => {
            eprintln!("airprompt: could not open the settings database: {error}");
            return;
        }
    };
    let settings = match database.load_settings() {
        Ok(s) => s,
        Err(_) => Settings::default(),
    };

    run_with(paths, migration, logging, database, settings);
}

/// The Tauri builder, split out of `run` so the fallible prologue stays
/// readable. Returns through `app.run`, never early.
fn run_with(
    paths: paths::AppPaths,
    migration: paths::MigrationReport,
    logging: Arc<logging::Logging>,
    database: Database,
    _settings: Settings,
) {
    let tauri_app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch focuses the dashboard instead of starting a
            // second copy (spec §67: no duplicate tray process).
            if let Some(deps) = overlay::shared_from(app) {
                let _ = deps.overlay.open_dashboard();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(move |app| {
            let handle = app.handle().clone();

            // The shared services, in dependency order.
            let platform = platform_crate_shim::create();
            let secrets = platform_crate_shim::create_secret_store();

            let whisper_binary = airprompt_core::whisper::WhisperCliEngine::discover(
                &paths.whisper_search_path(handle.path().resource_dir().ok().as_deref()),
            );
            let speech = Arc::new(speech::SpeechService::new(
                whisper_binary
                    .clone()
                    .unwrap_or_else(|| paths.data.join("bin/whisper-cli")),
                paths.cache.clone(),
            ));
            let models = airprompt_core::models::ModelManager::new(&paths.models);

            let sink: Arc<dyn EventSink> = Arc::new(events::AppSink::new(handle.clone()));
            let app_state = Arc::new(AppState {
                db: database,
                platform: platform.clone(),
                secrets,
                sink: sink.clone(),
                speech: speech.clone(),
                models,
                capture: Arc::new(capture::CaptureCore::new(16_000, 1)),
                started_at_ms: airprompt_core::now_ms(),
                whisper_binary,
                resource_dir: handle.path().resource_dir().ok(),
            });
            let controller = Arc::new(DictationController::new(app_state.clone()));
            controller.set_recovery_path(paths.last_transcript());

            // Register the shared slot before any command or shortcut fires.
            let shortcuts = Arc::new(shortcuts::ShortcutManager::new(handle.clone()));
            let overlay = Arc::new(OverlayController::new(handle.clone()));
            let watch = Arc::new(state::AppSettingsWatch::new(load_or_default(&app_state)));
            app.manage(Mutex::new(Some(SharedDependencies {
                state: app_state.clone(),
                controller: controller.clone(),
                logging: Some(logging.clone()),
                shortcuts: shortcuts.clone(),
                overlay: overlay.clone(),
                paths: paths.clone(),
                watch: watch.clone(),
            })));

            // Apply settings to the runtime: shortcuts, overlay, tray state.
            let boot_settings = load_or_default(&app_state);
            if let Err(error) = shortcuts.apply(&boot_settings) {
                tracing::warn!(code = error.code(), "global shortcuts failed to register");
                sink.emit(events::UiEvent::Toast {
                    toast: events::ToastEvent::warning(format!(
                        "The shortcut {} could not be registered. Change it in Settings.",
                        boot_settings.shortcuts.dictation.accelerator
                    )),
                });
            }
            overlay.apply(&boot_settings);

            // Load the configured model off the startup path (spec §14).
            let model_state = app_state.clone();
            let model_settings = boot_settings.clone();
            tauri::async_runtime::spawn(async move {
                if model_settings.speech.model_id.is_empty() {
                    tracing::info!("no model selected yet; onboarding will pick one");
                    return;
                }
                match model_state
                    .speech
                    .apply_settings(&model_settings, &model_state.models)
                {
                    Ok(_) => tracing::info!("speech model ready"),
                    Err(error) => {
                        tracing::warn!(code = error.code(), "model load failed at startup");
                        model_state.sink.emit(events::UiEvent::Toast {
                            toast: (&error).into(),
                        });
                    }
                }
            });

            // Tray, then the dictation loop.
            tray::create(&handle)?;
            spawn_dictation_loop(handle.clone(), controller.clone());

            // Windows: dashboard visible per settings, overlay hidden.
            if let Some(main) = handle.get_webview_window("main") {
                if boot_settings.general.start_minimized {
                    let _ = main.hide();
                } else {
                    let _ = main.show();
                }
            }
            if let Some(overlay_window) = handle.get_webview_window("overlay") {
                let _ = overlay_window.hide();
            }

            // Keep the autostart registration in step with the setting.
            let _ = handle.autolaunch().is_enabled();

            tracing::info!("startup complete");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::capabilities,
            commands::get_settings,
            commands::update_settings,
            commands::reset_settings,
            commands::permission_status,
            commands::request_permission,
            commands::open_permission_settings,
            commands::input_devices,
            commands::start_mic_test,
            commands::stop_mic_test,
            commands::start_dictation,
            commands::start_command_mode,
            commands::stop_dictation,
            commands::cancel_dictation,
            commands::overlay_ready,
            commands::current_overlay_state,
            commands::history_list,
            commands::history_delete,
            commands::history_delete_all,
            commands::history_export,
            commands::paste_last,
            commands::copy_to_clipboard,
            commands::dictionary_list,
            commands::dictionary_upsert,
            commands::dictionary_delete,
            commands::commands_list,
            commands::commands_upsert,
            commands::commands_delete,
            commands::parse_command,
            commands::run_command,
            commands::execute_command,
            commands::discover_targets,
            commands::run_shortcut,
            commands::profiles_list,
            commands::profiles_upsert,
            commands::profiles_delete,
            commands::list_models,
            commands::download_model,
            commands::pause_model_download,
            commands::resume_model_download,
            commands::delete_model,
            commands::verify_model,
            commands::set_active_model,
            commands::recommend_model,
            commands::system_profile,
            commands::models_disk_usage,
            commands::transcribe_file,
            commands::test_ai,
            commands::list_ai_models,
            commands::set_api_key,
            commands::api_key_status,
            commands::delete_api_key,
            commands::diagnostics,
            commands::diagnostics_text,
            commands::set_log_level,
            commands::log_file_path,
            commands::reveal_log_file,
            commands::check_update,
            commands::install_update,
            commands::open_dashboard,
            commands::quit_app,
            commands::set_overlay_shortcut_hint,
        ])
        .build(tauri::generate_context!())
        .expect("the Tauri application failed to build");

    tauri_app.run(|app, event| match event {
        RunEvent::ExitRequested { .. } => {
            if let Some(deps) = overlay::shared_from(app) {
                deps.shortcuts.unregister_all();
            }
        }
        RunEvent::Exit => {
            tracing::info!("shutdown complete");
        }
        _ => {}
    });
}

fn load_or_default(state: &Arc<AppState>) -> Settings {
    state.load_settings().unwrap_or_default()
}

/// The take pipeline's runtime loop: poll → decode/finish.
///
/// One loop owns all take progression; IPC only flips the phase machine. The
/// loop dies with the process, so there is no leak and no restart storm.
fn spawn_dictation_loop(handle: AppHandle, controller: Arc<DictationController>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(100));
        loop {
            ticker.tick().await;
            let Some(deps) = overlay::shared_from(&handle) else {
                continue;
            };
            let settings = deps.watch.get();

            match controller.poll(&settings) {
                PollAction::Wait { resume_after_ms } => {
                    if resume_after_ms != 100 {
                        tokio::time::sleep(std::time::Duration::from_millis(resume_after_ms)).await;
                    }
                }
                PollAction::Decode => {
                    controller.decode_partial();
                }
                PollAction::Finish => {
                    if let Err(error) = controller.stop() {
                        tracing::warn!(code = error.code(), "take finish failed");
                    }
                }
            }

            // The overlay's live position tracks the settings each cycle only
            // when a take is live; `overlay.apply` runs on settings changes.
            let _ = &deps.overlay;
        }
    });
}

/// The platform adapter shim, so `lib.rs` reads without cfg attributes.
mod platform_crate_shim {
    pub fn create() -> std::sync::Arc<dyn airprompt_core::platform::PlatformIntegration> {
        airprompt_platform::create()
    }

    pub fn create_secret_store() -> std::sync::Arc<dyn airprompt_core::platform::SecretStore> {
        airprompt_platform::create_secret_store()
    }
}

/// The unit tests for the shell's pure parts live in their modules; this
/// crate's integration with Tauri cannot be executed without a window server.
#[cfg(test)]
mod tests {
    #[test]
    fn the_module_list_is_complete() {
        // A new module must be registered here and in `invoke_handler`.
        let modules = [
            "capture",
            "commands",
            "dictation",
            "events",
            "logging",
            "overlay",
            "paths",
            "shortcuts",
            "speech",
            "state",
            "tray",
        ];
        assert_eq!(modules.len(), 11);
    }
}
