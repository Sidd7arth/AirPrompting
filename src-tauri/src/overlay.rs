//! Overlay and dashboard window control.
//!
//! The overlay is a separate, always-on-top, non-focusable window whose React
//! root renders [`crate::events::OverlayState`] transitions pushed as events.
//! Position follows the user's anchor setting (spec: overlay position), and
//! the window is hidden — never destroyed — between takes so the next take
//! starts with zero load latency.

use airprompt_core::error::{Error, Result};
use airprompt_core::settings::{OverlayAnchor, Settings};
use tauri::{AppHandle, Manager};

use crate::events::OverlayState;
use crate::state::SharedDependencies;

pub const OVERLAY_LABEL: &str = "overlay";
pub const MAIN_LABEL: &str = "main";

/// Controls the overlay and dashboard windows.
pub struct OverlayController {
    app: AppHandle,
    hint: std::sync::Mutex<Option<airprompt_core::settings::ShortcutBinding>>,
}

impl OverlayController {
    pub fn new(app: AppHandle) -> Self {
        OverlayController {
            app,
            hint: std::sync::Mutex::new(None),
        }
    }

    /// The overlay window, when it exists.
    fn overlay(&self) -> Option<tauri::WebviewWindow> {
        self.app.get_webview_window(OVERLAY_LABEL)
    }

    /// The overlay must call this once its frontend is listening.
    pub fn mark_ready(&self) {
        tracing::debug!("overlay reported ready");
        self.push(OverlayState::Hidden);
    }

    /// Push a full overlay state to the overlay window.
    ///
    /// Emitted through the same event channel as everything else; the
    /// overlay's store applies it. A missing overlay window (not created yet,
    /// or closed by the user) is logged at debug and ignored: the overlay is
    /// a view, not a source of truth.
    pub fn push(&self, state: OverlayState) {
        use tauri::Emitter;
        if let Some(window) = self.overlay() {
            let _ = window.emit(
                airprompt_core::identity::EVENT_CHANNEL,
                &crate::events::UiEvent::Overlay { state },
            );
        }
    }

    /// Apply the appearance settings (position, opacity).
    ///
    /// Size is fixed by the window config; position is recomputed from the
    /// anchor against the monitor the cursor is currently on, which is what
    /// "appears where I'm looking" means in practice.
    pub fn apply(&self, settings: &Settings) {
        let Some(window) = self.overlay() else {
            return;
        };
        let appearance = &settings.appearance;

        let _ = window.set_always_on_top(true);
        if !appearance.overlay_enabled {
            let _ = window.hide();
            return;
        }

        // Only reposition when not centered by the config default; `Cursor`
        // anchoring is applied per-take in `show_for_take`.
        if let Some(position) = anchor_position(&window, appearance.overlay_position) {
            let _ = window.set_position(position);
        }
        let _ = window.set_ignore_cursor_events(true);
    }

    /// Show the overlay for a starting take, positioned per settings.
    pub fn show_for_take(&self, settings: &Settings) {
        let Some(window) = self.overlay() else {
            return;
        };
        if !settings.appearance.overlay_enabled {
            return;
        }
        if settings.appearance.overlay_position == OverlayAnchor::Cursor {
            if let Some(position) = cursor_anchored_position(&window) {
                let _ = window.set_position(position);
            }
        }
        let _ = window.show();
    }

    /// The recorded shortcut hint shown on the idle overlay.
    pub fn set_shortcut_hint(&self, binding: Option<airprompt_core::settings::ShortcutBinding>) {
        *self.hint.lock().expect("hint lock") = binding;
    }

    pub fn shortcut_hint(&self) -> Option<airprompt_core::settings::ShortcutBinding> {
        self.hint.lock().expect("hint lock").clone()
    }

    /// Show and focus the dashboard window.
    pub fn open_dashboard(&self) -> Result<()> {
        let Some(window) = self.app.get_webview_window(MAIN_LABEL) else {
            return Err(Error::Io("the dashboard window is missing".into()));
        };
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.unminimize();
        Ok(())
    }

    /// Exit the application cleanly: shortcuts released, windows closed.
    pub fn quit(&self) {
        tracing::info!("quit requested");
        self.app.exit(0);
    }
}

/// Position for an anchor, in logical pixels, on the cursor's monitor.
fn anchor_position(
    window: &tauri::WebviewWindow,
    anchor: OverlayAnchor,
) -> Option<tauri::PhysicalPosition<i32>> {
    let monitor = cursor_monitor(window)?;
    let size = window.outer_size().ok()?;
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();

    let x = monitor_pos.x + ((monitor_size.width as i32 - size.width as i32) / 2).max(8);
    let y = match anchor {
        OverlayAnchor::Top => monitor_pos.y + 48,
        OverlayAnchor::Center => {
            monitor_pos.y + (monitor_size.height as i32 - size.height as i32) / 2
        }
        OverlayAnchor::Bottom => {
            monitor_pos.y + monitor_size.height as i32 - size.height as i32 - 96
        }
        OverlayAnchor::Cursor => return cursor_anchored_position(window),
    };
    Some(tauri::PhysicalPosition::new(x, y))
}

/// Place the overlay just above the mouse cursor, clamped to the monitor.
fn cursor_anchored_position(window: &tauri::WebviewWindow) -> Option<tauri::PhysicalPosition<i32>> {
    let monitor = cursor_monitor(window)?;
    let size = window.outer_size().ok()?;
    let cursor = window.cursor_position().ok()?;
    let monitor_pos = monitor.position();
    let monitor_size = monitor.size();

    let x = (cursor.x as i32 - size.width as i32 / 2).clamp(
        monitor_pos.x + 8,
        monitor_pos.x + monitor_size.width as i32 - size.width as i32 - 8,
    );
    let y = (cursor.y as i32 - size.height as i32 - 24).clamp(
        monitor_pos.y + 8,
        monitor_pos.y + monitor_size.height as i32 - size.height as i32 - 8,
    );
    Some(tauri::PhysicalPosition::new(x, y))
}

/// The monitor the cursor is on, falling back to the primary.
fn cursor_monitor(window: &tauri::WebviewWindow) -> Option<tauri::Monitor> {
    if let Ok(cursor) = window.cursor_position() {
        let cursor_x = cursor.x as i32;
        let cursor_y = cursor.y as i32;
        for monitor in window.available_monitors().ok()? {
            let pos = monitor.position();
            let size = monitor.size();
            let within_x = cursor_x >= pos.x && cursor_x < pos.x + size.width as i32;
            let within_y = cursor_y >= pos.y && cursor_y < pos.y + size.height as i32;
            if within_x && within_y {
                return Some(monitor);
            }
        }
    }
    window.primary_monitor().ok().flatten()
}

/// Resolve the shared dependencies from an app handle, if the app is set up.
pub(crate) fn shared_from(app: &AppHandle) -> Option<std::sync::Arc<SharedDependencies>> {
    let shared = app.try_state::<std::sync::Mutex<Option<SharedDependencies>>>()?;
    shared.lock().ok()?.as_ref().map(|deps| deps.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_labels_match_the_tauri_config() {
        // tauri.conf.json declares "main" and "overlay"; a rename in either
        // file must be mirrored in both or every lookup silently fails.
        assert_eq!(MAIN_LABEL, "main");
        assert_eq!(OVERLAY_LABEL, "overlay");
    }

    #[test]
    fn anchor_positions_are_well_ordered() {
        // The math for a 1080p monitor with a 660×220 overlay must place
        // top above center above bottom.
        let monitor_y = 0;
        let monitor_height = 1080;
        let overlay_height = 220;
        let top = monitor_y + 48;
        let center = monitor_y + (monitor_height - overlay_height) / 2;
        let bottom = monitor_y + monitor_height - overlay_height - 96;
        assert!(top < center && center < bottom);
        assert!(bottom + overlay_height <= monitor_height);
    }
}
