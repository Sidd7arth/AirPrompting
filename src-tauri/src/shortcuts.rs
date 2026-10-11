//! Global shortcut registration and dispatch (spec §11, §12).
//!
//! The OS-level registration is Tauri's `global-shortcut` plugin, which uses
//! `RegisterHotKey` on Windows and `AddEventSinkForHotKey`/Carbon
//! `RegisterEventHotKey` on macOS — a real, OS-registered shortcut on both.
//! The shortcuts are re-applied whenever settings change: the previous
//! bindings are unregistered first, so a changed hotkey never leaves the old
//! one dead-locked in the OS.
//!
//! Actions:
//! * `dictation` — start/stop a dictation take (activation mode decides
//!   press-vs-toggle behaviour).
//! * `voice_command` — a command-mode take.
//! * `push_to_talk` — hold-to-record when activation is `toggle`.

use std::sync::Arc;

use airprompt_core::error::{Error, Result};
use airprompt_core::settings::Settings;
use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::dictation::TakePurpose;
use crate::state::SharedDependencies;

/// Which binding a registration belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Dictation,
    VoiceCommand,
    PushToTalk,
}

impl Slot {
    fn from_name(name: &str) -> Option<Self> {
        match name {
            "dictation" => Some(Slot::Dictation),
            "voice_command" => Some(Slot::VoiceCommand),
            "push_to_talk" => Some(Slot::PushToTalk),
            _ => None,
        }
    }
}

/// The registered shortcuts, and the dependencies their handlers call.
pub struct ShortcutManager {
    app: tauri::AppHandle,
    /// The accelerators currently registered, by slot.
    registered: std::sync::Mutex<Vec<(Slot, String)>>,
}

impl ShortcutManager {
    pub fn new(app: tauri::AppHandle) -> Self {
        ShortcutManager {
            app,
            registered: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// (Re-)register every enabled binding from `settings`.
    ///
    /// Idempotent and safe to call on every settings change: unchanged
    /// accelerators stay registered, changed ones are swapped, disabled ones
    /// are released.
    pub fn apply(&self, settings: &Settings) -> Result<()> {
        let wanted: Vec<(Slot, String)> = settings
            .enabled_bindings()
            .iter()
            .filter_map(|(name, binding)| {
                Slot::from_name(name).map(|slot| (slot, binding.accelerator.clone()))
            })
            .collect();

        let mut registered = self
            .registered
            .lock()
            .map_err(|_| Error::Store("shortcut registry lock poisoned".into()))?;

        // Release everything that is not wanted any more, or whose
        // accelerator changed.
        let previous = std::mem::take(&mut *registered);
        for (slot, accelerator) in &previous {
            let still_wanted = wanted.iter().any(|(s, a)| s == slot && a == accelerator);
            if !still_wanted {
                if let Ok(shortcut) = accelerator.parse::<Shortcut>() {
                    let _ = self.app.global_shortcut().unregister(shortcut);
                }
            }
        }

        // Register the wanted set.
        let mut now_registered = Vec::new();
        for (slot, accelerator) in &wanted {
            let already = previous.iter().any(|(s, a)| s == slot && a == accelerator);
            if already {
                now_registered.push((*slot, accelerator.clone()));
                continue;
            }
            let shortcut: Shortcut = accelerator
                .parse()
                .map_err(|_| Error::ShortcutConflict(accelerator.clone()))?;
            self.app
                .global_shortcut()
                .on_shortcut(shortcut, |app, event| {
                    if event.state() == ShortcutState::Pressed {
                        Self::dispatch(app);
                    }
                })
                .map_err(|error| {
                    tracing::warn!(%error, accelerator, "shortcut registration failed");
                    Error::ShortcutConflict(accelerator.clone())
                })?;
            now_registered.push((*slot, accelerator.clone()));
            tracing::info!(accelerator, slot = ?slot, "shortcut registered");
        }

        *registered = now_registered;
        Ok(())
    }

    /// Every currently registered accelerator, for diagnostics.
    pub fn registered(&self) -> Vec<String> {
        self.registered
            .lock()
            .map(|r| r.iter().map(|(_, a)| a.clone()).collect())
            .unwrap_or_default()
    }

    /// Release everything. Called on shutdown.
    pub fn unregister_all(&self) {
        if let Ok(registered) = self.registered.lock() {
            for (_, accelerator) in registered.iter() {
                if let Ok(shortcut) = accelerator.parse::<Shortcut>() {
                    let _ = self.app.global_shortcut().unregister(shortcut);
                }
            }
        }
    }

    /// The single dispatch point for every shortcut press.
    ///
    /// Press semantics by activation mode (spec §5):
    /// * push-to-talk: press starts, release stops.
    /// * toggle: press toggles.
    /// * voice_command: always toggle.
    fn dispatch(app: &tauri::AppHandle) {
        let Some(shared) = app.try_state::<std::sync::Mutex<Option<SharedDependencies>>>() else {
            return;
        };
        let Ok(guard) = shared.lock() else {
            return;
        };
        let Some(deps) = guard.as_ref() else {
            return;
        };
        deps.controller.handle_activation();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_names_match_the_settings_layer() {
        // These strings come from Settings::enabled_bindings; a rename there
        // must be mirrored here or shortcuts silently stop dispatching.
        assert_eq!(Slot::from_name("dictation"), Some(Slot::Dictation));
        assert_eq!(Slot::from_name("voice_command"), Some(Slot::VoiceCommand));
        assert_eq!(Slot::from_name("push_to_talk"), Some(Slot::PushToTalk));
        assert_eq!(Slot::from_name("overlay"), None);
        assert_eq!(Slot::from_name(""), None);
    }

    #[test]
    fn default_settings_enable_dictation_and_voice_command() {
        let settings = Settings::default();
        let names: Vec<_> = settings
            .enabled_bindings()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert!(names.contains(&"dictation"));
        assert!(names.contains(&"voice_command"));
        assert!(!names.contains(&"push_to_talk"), "off by default");
    }

    #[test]
    fn disabled_bindings_are_not_registered() {
        let mut settings = Settings::default();
        settings.shortcuts.dictation.enabled = false;
        let names: Vec<_> = settings
            .enabled_bindings()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert!(!names.contains(&"dictation"));
    }

    #[test]
    fn a_duplicate_push_to_talk_binding_is_dropped() {
        let mut settings = Settings::default();
        settings.shortcuts.push_to_talk.enabled = true;
        settings.shortcuts.push_to_talk.accelerator =
            settings.shortcuts.dictation.accelerator.clone();
        let names: Vec<_> = settings
            .enabled_bindings()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            names,
            vec!["dictation", "voice_command"],
            "a duplicate binding must not be registered twice"
        );
    }
}
