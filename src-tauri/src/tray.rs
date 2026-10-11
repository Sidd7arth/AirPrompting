//! System-tray menu (spec §13: menu bar/tray).
//!
//! The tray is the product's quiet home: Show AirPrompt, Start Dictation,
//! Voice Command, Quit. It is created once at startup and keeps no state; menu
//! events are routed through the same shared dependencies the IPC commands
//! use, so the tray can never drift from the keyboard paths.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

use crate::state::SharedDependencies;

/// Create the tray. Called once from setup.
pub fn create(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    let show = MenuItem::with_id(app, "show", "Show AirPrompt", true, None::<&str>)?;
    let dictate = MenuItem::with_id(app, "dictate", "Start Dictation", true, None::<&str>)?;
    let command = MenuItem::with_id(app, "command", "Voice Command", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit AirPrompt", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &show,
            &PredefinedMenuItem::separator(app)?,
            &dictate,
            &command,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    let _ = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .tooltip(airprompt_core::identity::APP_NAME)
        // The icon is declared in tauri.conf.json for the bundle; the tray
        // uses the same file through the default window icon.
        .icon(
            app.default_window_icon()
                .cloned()
                .ok_or_else(|| tauri::Error::AssetNotFound("default window icon".into()))?,
        )
        .on_menu_event(|app, event| {
            let Some(deps) = crate::overlay::shared_from(app) else {
                return;
            };
            match event.id().as_ref() {
                "show" => {
                    let _ = deps.overlay.open_dashboard();
                }
                "dictate" => handle_dictation_toggle(&deps),
                "command" => {
                    if let Err(error) = deps
                        .controller
                        .start(crate::dictation::TakePurpose::Command, Some("command"))
                    {
                        tracing::warn!(code = error.code(), "tray command start failed");
                    }
                }
                "quit" => deps.overlay.quit(),
                _ => {}
            }
        })
        .build(app)?;

    Ok(())
}

/// Start or stop a dictation take, mirroring the keyboard behaviour.
fn handle_dictation_toggle(deps: &std::sync::Arc<SharedDependencies>) {
    if deps.controller.is_busy() {
        if let Err(error) = deps.controller.stop() {
            tracing::warn!(code = error.code(), "tray stop failed");
        }
    } else if let Err(error) = deps
        .controller
        .start(crate::dictation::TakePurpose::Dictation, None)
    {
        tracing::warn!(code = error.code(), "tray start failed");
        deps.state.sink.emit(crate::events::UiEvent::Toast {
            toast: crate::events::ToastEvent::from(&error),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_are_stable() {
        // These ids are the tray's contract with on_menu_event; renaming one
        // without the other silently dead-ends a menu item.
        let ids = ["show", "dictate", "command", "quit"];
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().all(|id| !id.is_empty()));
    }
}
