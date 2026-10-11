//! Windows implementation of [`PlatformIntegration`] and [`SecretStore`].
//!
//! Everything OS-specific for Windows lives here. Nothing in `airprompt-core`
//! knows that `SendInput` or `ShellExecuteW` exist; the core sees only the trait
//! (spec §10).
//!
//! ## What is implemented, and how
//!
//! | Trait method | Mechanism |
//! |---|---|
//! | `active_application` | `GetForegroundWindow` → owning process (`OpenProcess` + `QueryFullProcessImageNameW`) → window title |
//! | `restore_focus` | finds the target process's top-level window and calls `SetForegroundWindow` |
//! | `is_secure_input` | focused control's class + `ES_PASSWORD` style, via `GetGUIThreadInfo` |
//! | `insert_text` | clipboard-paste with preservation, direct Unicode `SendInput`, or clipboard only |
//! | `clipboard` | `arboard` (the Windows clipboard API through one audited crate) |
//! | `launch_application` | resolves a spoken name against App Paths and the Start Menu, then `ShellExecuteW` on the resolved file — never a command line |
//! | `open_url` / `open_path` | validated by `airprompt_core::security`, then `ShellExecuteW` |
//! | `installed_applications` | App Paths keys plus Start Menu shortcut names |
//! | permissions | microphone read from the ConsentStore registry; Windows needs no Accessibility grant for `SendInput` |
//! | `set_launch_at_login` | `HKCU\…\CurrentVersion\Run` value named after the product |
//! | `SecretStore` | Windows Credential Manager via `keyring`, under the AirPrompt service name |
//!
//! ## What is deliberately not implemented here
//!
//! * **Audio device enumeration and capture.** `input_devices` and
//!   `default_input_device` return [`Error::Unsupported`]. Device enumeration
//!   and capture are owned by the application shell
//!   (`src-tauri/src/capture.rs`), which links `cpal` — the capture graph is a
//!   runtime concern (device → resample → VAD → engine), not a desktop action,
//!   and keeping the audio backend out of this crate is what lets it be checked
//!   and tested without one. The methods stay in the trait so an adapter that
//!   *can* enumerate (none today) has somewhere to put it.
//! * **Shortcut running.** macOS Shortcuts have no Windows equivalent; the
//!   methods return `Unsupported`, and the UI hides the feature on this platform.
//!
//! ## Security posture
//!
//! * No method here ever builds a command line. `ShellExecuteW` is always given
//!   a resolved path or a validated URL, and no parameters.
//! * A spoken application name is matched against the operating system's own
//!   registry of installed applications; an unmatched name is refused rather
//!   than handed to the shell for interpretation.
//! * Targets containing shell syntax (`&&`, `|`, `;`, `$(`, backticks, `>`…) are
//!   refused before anything is opened.
//! * `insert_text` refuses to type into this process's own windows, so a stuck
//!   dictation cannot inject text into AirPrompt's own settings fields.

use airprompt_core::error::{Error, Result};
use airprompt_core::identity;
use airprompt_core::platform::{
    ActiveApplication, InputDevice, InsertMode, InsertOutcome, PermissionKind, PermissionState,
    PermissionStatus, PlatformIntegration, SecretStore,
};
use airprompt_core::security::{self, ActionKind, Verdict};

use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, ERROR_SUCCESS, HWND, LPARAM};
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_SZ,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, VIRTUAL_KEY, VK_CONTROL, VK_V,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
    SetForegroundWindow, ShowWindow, GUITHREADINFO, GWL_STYLE, SW_RESTORE, SW_SHOWNORMAL,
};

/// Buffer size for Win32 string APIs. 260 is `MAX_PATH`; process image paths can
/// be longer, so the process query retries with a bigger buffer.
const PATH_BUFFER: usize = 32_768;

/// `ES_PASSWORD`: the style bit a Windows edit control sets for a password box.
const ES_PASSWORD: i32 = 0x0020;

/// Pause after synthesising a paste, before restoring the clipboard. Long enough
/// for a normal application to service `WM_PASTE`, short enough not to be felt.
const PASTE_SETTLE_MS: u64 = 140;

/// Executables that exist to run other things. They are refused by name even
/// though arguments are never passed, because "open PowerShell" as a *voice*
/// action is a capability the product does not want to offer (spec §25).
const REFUSED_EXECUTABLES: &[&str] = &[
    "cmd",
    "cmd.exe",
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
    "wscript",
    "wscript.exe",
    "cscript",
    "cscript.exe",
    "mshta",
    "mshta.exe",
    "rundll32",
    "rundll32.exe",
    "regsvr32",
    "regsvr32.exe",
    "msiexec",
    "msiexec.exe",
    "bash",
    "bash.exe",
    "wsl",
    "wsl.exe",
];

/// Office keeps its executables under keys that do not resemble their names.
const OFFICE_ALIASES: &[(&str, &str)] = &[
    ("word", "WINWORD.EXE"),
    ("microsoft word", "WINWORD.EXE"),
    ("excel", "EXCEL.EXE"),
    ("microsoft excel", "EXCEL.EXE"),
    ("powerpoint", "POWERPNT.EXE"),
    ("outlook", "OUTLOOK.EXE"),
    ("microsoft outlook", "OUTLOOK.EXE"),
    ("access", "MSACCESS.EXE"),
];

/// The Windows platform adapter.
///
/// Stateless on purpose: every call reads the current state of the desktop, so
/// there is nothing to fall out of sync after a sleep, a fast user switch or a
/// crashed helper process.
pub struct WindowsIntegration;

impl WindowsIntegration {
    pub fn new() -> Self {
        WindowsIntegration
    }
}

impl Default for WindowsIntegration {
    fn default() -> Self {
        Self::new()
    }
}

/* --------------------------------------------------------------- helpers --- */

/// A Rust string as a NUL-terminated UTF-16 buffer for Win32.
fn to_wide(value: &str) -> Vec<u16> {
    OsString::from(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// A NUL-terminated UTF-16 buffer as a Rust string, stopping at the first NUL.
fn from_wide(value: &[u16]) -> String {
    let end = value
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(value.len());
    OsString::from_wide(&value[..end])
        .to_string_lossy()
        .into_owned()
}

/// Read a `REG_SZ` value, or `None` when it is absent or not a string.
fn read_reg_string(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            root,
            PCWSTR(to_wide(subkey).as_ptr()),
            0,
            KEY_READ,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return None;
        }

        let name = to_wide(value_name);
        let mut kind = REG_SZ;
        let mut size: u32 = 0;

        // First call sizes the value.
        if RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            None,
            Some(&mut size),
        ) != ERROR_SUCCESS
            || size == 0
        {
            let _ = RegCloseKey(key);
            return None;
        }

        let mut buffer = vec![0u8; size as usize];
        let status = RegQueryValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut kind),
            Some(buffer.as_mut_ptr()),
            Some(&mut size),
        );
        let _ = RegCloseKey(key);

        if status != ERROR_SUCCESS {
            return None;
        }

        let units: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let text = from_wide(&units);
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }
}

/// Enumerate the subkey names directly under `subkey`.
fn read_reg_subkeys(root: HKEY, subkey: &str) -> Vec<String> {
    let mut names = Vec::new();

    unsafe {
        let mut key = HKEY::default();
        if RegOpenKeyExW(
            root,
            PCWSTR(to_wide(subkey).as_ptr()),
            0,
            KEY_READ,
            &mut key,
        ) != ERROR_SUCCESS
        {
            return names;
        }

        let mut index = 0u32;
        loop {
            let mut buffer = [0u16; 512];
            let mut length = buffer.len() as u32;
            let status = RegEnumKeyExW(
                key,
                index,
                PWSTR(buffer.as_mut_ptr()),
                &mut length,
                None,
                PWSTR::null(),
                None,
                None,
            );
            if status != ERROR_SUCCESS {
                break;
            }
            names.push(from_wide(&buffer[..length as usize]));
            index += 1;
        }

        let _ = RegCloseKey(key);
    }

    names
}

/// The process id owning a window, if any.
fn window_pid(window: HWND) -> Option<u32> {
    if window.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(window, Some(&mut pid));
    }
    if pid == 0 {
        None
    } else {
        Some(pid)
    }
}

/// The window's title, empty string when it has none.
fn window_title(window: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(window) };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let written = unsafe { GetWindowTextW(window, &mut buffer) };
    from_wide(&buffer[..written.max(0) as usize])
}

/// The executable path behind a process id, e.g.
/// `C:\Program Files\Google\Chrome\Application\chrome.exe`.
fn process_image(pid: u32) -> Option<PathBuf> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; PATH_BUFFER];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(handle);

        if result.is_err() || length == 0 {
            return None;
        }
        Some(PathBuf::from(from_wide(&buffer[..length as usize])))
    }
}

/// The friendly executable name: `chrome.exe` → `chrome`.
fn executable_stem(file_name: &str) -> Option<String> {
    Path::new(file_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().to_lowercase())
}

/// Normalise a spoken or typed application name for comparison.
fn normalize_name(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".exe")
        .chars()
        .filter(|c| c.is_alphanumeric() || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Directories Windows uses for per-user and machine-wide Start Menu entries.
fn start_menu_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();

    if let Some(appdata) = dirs::data_dir() {
        directories.push(appdata.join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(program_data) = std::env::var_os("ProgramData") {
        directories
            .push(PathBuf::from(program_data).join(r"Microsoft\Windows\Start Menu\Programs"));
    }

    directories
}

/// Every shortcut name under the Start Menu, as a user would say it.
fn start_menu_names() -> Vec<String> {
    let mut names = Vec::new();

    for directory in start_menu_directories() {
        collect_shortcut_names(&directory, 0, &mut names);
    }

    names.sort();
    names.dedup();
    names
}

fn collect_shortcut_names(directory: &Path, depth: u32, names: &mut Vec<String>) {
    // Bounded: a malicious or accidental directory loop must not hang startup.
    if depth > 6 || names.len() > 2_000 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_shortcut_names(&path, depth + 1, names);
        } else if path
            .extension()
            .map(|extension| extension.eq_ignore_ascii_case("lnk"))
            .unwrap_or(false)
        {
            if let Some(stem) = path.file_stem() {
                names.push(stem.to_string_lossy().into_owned());
            }
        }
    }
}

/// Application Paths keys: the operating system's own registry of launchable
/// applications, mapping `chrome.exe` to its full path.
fn app_paths() -> Vec<(String, PathBuf)> {
    const APP_PATHS: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths";

    let mut applications = Vec::new();

    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for key in read_reg_subkeys(root, APP_PATHS) {
            let subkey = format!(r"{APP_PATHS}\{key}");
            if let Some(path) = read_reg_string(root, &subkey, "") {
                applications.push((key.clone(), PathBuf::from(path)));
            }
        }
    }

    applications
}

/// Resolve a spoken application name to a real executable path.
///
/// Only names the operating system itself knows about are accepted. Returns
/// `None` when nothing matches, which the caller reports to the user instead of
/// guessing.
fn resolve_application(name: &str) -> Option<PathBuf> {
    let wanted = normalize_name(name);
    if wanted.is_empty() {
        return None;
    }

    if REFUSED_EXECUTABLES
        .iter()
        .any(|refused| normalize_name(refused) == wanted)
    {
        return None;
    }

    // A friendly alias first, so "Word" finds WINWORD.EXE.
    let aliased = OFFICE_ALIASES
        .iter()
        .find(|(spoken, _)| *spoken == wanted)
        .map(|(_, executable)| normalize_name(executable));

    let applications = app_paths();

    // An exact match on the executable name is unambiguous.
    for (key, path) in &applications {
        if normalize_name(key) == wanted || Some(&normalize_name(key)) == aliased.as_ref() {
            return Some(path.clone());
        }
    }

    // Then an exact Start Menu name, which is what a user actually says.
    for candidate in start_menu_names() {
        if normalize_name(&candidate) == wanted {
            for (key, path) in &applications {
                if let (Some(candidate_stem), Some(key_stem)) = (
                    normalize_name(&candidate).split(' ').next_back(),
                    executable_stem(key),
                ) {
                    if candidate_stem.starts_with(&key_stem) {
                        return Some(path.clone());
                    }
                }
            }
        }
    }

    // Finally a unique substring match on either source. Ambiguity is refused:
    // silently picking one of three similar applications is exactly the
    // dangerous assumption the specification forbids.
    let mut matches: Vec<PathBuf> = Vec::new();

    for (key, path) in &applications {
        let key_name = normalize_name(key);
        if key_name.contains(&wanted) || wanted.contains(&key_name) {
            matches.push(path.clone());
        }
    }
    for candidate in start_menu_names() {
        let candidate_name = normalize_name(&candidate);
        if candidate_name.contains(&wanted)
            || (wanted.contains(&candidate_name) && candidate_name.len() > 3)
        {
            for (key, path) in &applications {
                if let Some(stem) = normalize_name(&candidate).split(' ').next_back() {
                    if normalize_name(key).starts_with(stem) {
                        matches.push(path.clone());
                    }
                }
            }
        }
    }

    matches.sort();
    matches.dedup();
    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

/// True when this window belongs to AirPrompt itself.
fn is_own_window(window: HWND) -> bool {
    window_pid(window) == Some(unsafe { GetCurrentProcessId() })
}

/// The window handle of the focused control, if Windows reports one (used by
/// [`WindowsIntegration::is_secure_input`]).
fn focused_control() -> Option<HWND> {
    unsafe {
        let foreground = GetForegroundWindow();
        if foreground.0.is_null() {
            return None;
        }
        let thread = GetWindowThreadProcessId(foreground, None);
        if thread == 0 {
            return None;
        }
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread, &mut info).is_err() {
            return None;
        }
        if info.hwndFocus.0.is_null() {
            None
        } else {
            Some(info.hwndFocus)
        }
    }
}

/// The class name of a window, e.g. `Edit`.
fn window_class(window: HWND) -> String {
    let mut buffer = [0u16; 256];
    let written = unsafe { GetClassNameW(window, &mut buffer) };
    if written <= 0 {
        String::new()
    } else {
        from_wide(&buffer[..written as usize])
    }
}

/* ------------------------------------------------------- integration --- */

impl PlatformIntegration for WindowsIntegration {
    fn platform_name(&self) -> &'static str {
        "windows"
    }

    fn arch(&self) -> &'static str {
        std::env::consts::ARCH
    }

    fn active_application(&self) -> Result<ActiveApplication> {
        let window = unsafe { GetForegroundWindow() };
        if window.0.is_null() {
            return Err(Error::Unsupported(
                "no window had focus; nothing to dictate into".into(),
            ));
        }

        let pid = window_pid(window);
        let image = pid.and_then(process_image);

        let name = image
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();

        Ok(ActiveApplication {
            name,
            pid,
            identifier: image.map(|path| path.to_string_lossy().into_owned()),
            window_title: {
                let title = window_title(window);
                if title.is_empty() {
                    None
                } else {
                    Some(title)
                }
            },
            // Continuous dictation needs the text before the caret, which on
            // Windows requires UI Automation on the focused element. Not
            // implemented yet; empty means "the platform could not read it",
            // which is the documented contract for this field.
            preceding_text: String::new(),
        })
    }

    fn restore_focus(&self, app: &ActiveApplication) -> Result<()> {
        let Some(pid) = app.pid else {
            return Err(Error::TargetGone);
        };

        let wanted_title = app.window_title.as_deref().unwrap_or("");

        unsafe extern "system" fn visit(window: HWND, lparam: LPARAM) -> BOOL {
            let state = unsafe { &mut *(lparam.0 as *mut FocusSearch) };
            if !unsafe { IsWindowVisible(window).as_bool() } {
                return true.into();
            }
            let Some(pid) = window_pid(window) else {
                return true.into();
            };
            if pid != state.pid {
                return true.into();
            }
            if state.fallback.is_none() {
                state.fallback = Some(window);
            }
            if !state.title.is_empty() && window_title(window) == state.title {
                state.best = Some(window);
                return false.into();
            }
            true.into()
        }

        let mut state = FocusSearch {
            pid,
            title: wanted_title.to_string(),
            best: None,
            fallback: None,
        };

        unsafe {
            let _ = EnumWindows(Some(visit), LPARAM(&mut state as *mut FocusSearch as isize));
        }

        if let Some(window) = state.best.or(state.fallback) {
            unsafe {
                let _ = ShowWindow(window, SW_RESTORE);
                if SetForegroundWindow(window).as_bool() {
                    return Ok(());
                }
            }
        }

        // The application may have closed while the user was speaking. That is a
        // normal outcome, not a crash: the pipeline puts the text on the
        // clipboard (spec §22).
        Err(Error::TargetGone)
    }

    fn is_secure_input(&self) -> bool {
        let Some(control) = focused_control() else {
            return false;
        };

        let class = window_class(control).to_lowercase();
        let looks_like_text_input = class.contains("edit") || class.contains("text");
        if !looks_like_text_input {
            return false;
        }

        let style = unsafe { GetWindowLongW(control, GWL_STYLE) };
        style & ES_PASSWORD != 0
    }

    fn insert_text(&self, text: &str, mode: InsertMode) -> Result<InsertOutcome> {
        if text.is_empty() {
            return Err(Error::Insertion("there was nothing to insert".into()));
        }

        // Never type into our own windows. Without this, a stuck dictation would
        // inject its transcript into AirPrompt's own settings fields.
        let foreground = unsafe { GetForegroundWindow() };
        if is_own_window(foreground) {
            return Err(Error::Insertion(
                "AirPrompt had focus, so the text was left on the clipboard".into(),
            ));
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
                // A clipboard holding something other than text (an image, files,
                // rich content) must not be destroyed for our convenience. In
                // that case the direct-typing path is used instead.
                let previous = match read_clipboard_text() {
                    Some(previous) => Some(previous),
                    None => {
                        type_unicode(text)?;
                        return Ok(InsertOutcome::Inserted);
                    }
                };

                set_clipboard_text(text)?;
                let pasted = send_ctrl_v();

                if let Some(previous) = previous {
                    if let Err(error) = set_clipboard_text(&previous) {
                        // Explicit, documented outcome: the transcript is still
                        // on the clipboard, which is the user's fallback, and
                        // the failure is reported rather than swallowed.
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
        if security::looks_like_shell(name) {
            return Err(Error::Refused(
                "that looks like a command line, not an application name".into(),
            ));
        }
        if name.contains(['"', '\'', '\\', '/', '\n', '\r', '\t']) {
            return Err(Error::Refused(
                "application names cannot contain path separators or quotes".into(),
            ));
        }

        if let Verdict::Blocked(reason) = security::judge(ActionKind::OpenApp, name) {
            return Err(Error::Refused(reason.to_string()));
        }

        let Some(path) = resolve_application(name) else {
            // Refused, not guessed: an unknown name is never handed to the shell
            // for interpretation (spec §26, §27).
            return Err(Error::Refused(format!(
                "no installed application matched \"{name}\""
            )));
        };

        open_with_shell(&path)
    }

    fn open_url(&self, url: &str) -> Result<()> {
        let normalized = security::validate_url(url)?;
        if let Verdict::Blocked(reason) = security::judge(ActionKind::OpenUrl, &normalized) {
            return Err(Error::Refused(reason.to_string()));
        }
        // The URL has already been constrained to an allowed scheme, so passing
        // it to the shell opener is not a command-line risk.
        open_shell_target(&normalized)
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

        open_with_shell(&path)
    }

    fn list_shortcuts(&self) -> Result<Vec<String>> {
        // Windows has no Shortcuts app. An empty list is the honest answer; the
        // UI hides the section rather than offering a feature that cannot work.
        Ok(Vec::new())
    }

    fn run_shortcut(&self, _name: &str) -> Result<()> {
        Err(Error::Unsupported(
            "Shortcuts are a macOS feature; use a voice command instead".into(),
        ))
    }

    fn installed_applications(&self) -> Result<Vec<String>> {
        let mut applications: Vec<String> = app_paths()
            .into_iter()
            .filter_map(|(key, _)| {
                let stem = key.trim_end_matches(".exe").to_string();
                if REFUSED_EXECUTABLES
                    .iter()
                    .any(|refused| normalize_name(refused) == normalize_name(&stem))
                {
                    None
                } else {
                    Some(pretty_name(&stem))
                }
            })
            .collect();

        applications.extend(start_menu_names());
        applications.sort();
        applications.dedup();
        Ok(applications)
    }

    fn permission_status(&self) -> PermissionStatus {
        PermissionStatus {
            microphone: microphone_permission(),
            // Windows grants no "Accessibility" permission to a desktop app:
            // SendInput is available to any process the user is running. The
            // settings screen must therefore not ask for one.
            accessibility: PermissionState::Unsupported,
            input_monitoring: PermissionState::Unsupported,
        }
    }

    fn request_permission(&self, kind: PermissionKind) -> Result<PermissionStatus> {
        match kind {
            PermissionKind::Microphone => {
                open_settings_page("ms-settings:privacy-microphone")?;
                Ok(self.permission_status())
            }
            other => Err(Error::Unsupported(format!(
                "{} is not a permission Windows asks for",
                match other {
                    PermissionKind::Accessibility => "Accessibility",
                    PermissionKind::InputMonitoring => "Input Monitoring",
                    PermissionKind::Microphone => "Microphone",
                }
            ))),
        }
    }

    fn open_permission_settings(&self, kind: PermissionKind) -> Result<()> {
        match kind {
            PermissionKind::Microphone => open_settings_page("ms-settings:privacy-microphone"),
            PermissionKind::Accessibility => open_settings_page("ms-settings:easeofaccess"),
            kind => Err(Error::Unsupported(format!(
                "{kind:?} has no settings page on Windows"
            ))),
        }
    }

    fn input_devices(&self) -> Result<Vec<InputDevice>> {
        Err(Error::Unsupported(
            "microphone selection is not implemented on Windows yet".into(),
        ))
    }

    fn default_input_device(&self) -> Result<Option<String>> {
        Err(Error::Unsupported(
            "microphone selection is not implemented on Windows yet".into(),
        ))
    }

    fn set_launch_at_login(&self, enabled: bool) -> Result<()> {
        const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

        let executable = std::env::current_exe()
            .map_err(|error| Error::Io(format!("could not locate the application: {error}")))?;
        let value = run_value(&executable);

        unsafe {
            let mut key = HKEY::default();
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(to_wide(RUN_KEY).as_ptr()),
                0,
                KEY_WRITE,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return Err(Error::Io("could not open the startup registry key".into()));
            }

            let name = to_wide(identity::APP_NAME);
            let status = if enabled {
                let bytes: Vec<u8> = value
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                RegSetValueExW(key, PCWSTR(name.as_ptr()), 0, REG_SZ, Some(&bytes))
            } else {
                RegDeleteValueW(key, PCWSTR(name.as_ptr()))
            };

            let _ = RegCloseKey(key);

            if status != ERROR_SUCCESS {
                return Err(Error::Io("could not update the startup entry".into()));
            }
        }

        Ok(())
    }

    fn launch_at_login(&self) -> Result<bool> {
        const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

        let recorded = read_reg_string(HKEY_CURRENT_USER, RUN_KEY, identity::APP_NAME);
        let Some(recorded) = recorded else {
            return Ok(false);
        };

        let Ok(executable) = std::env::current_exe() else {
            return Ok(false);
        };

        // The entry counts as ours only when it points at this executable; a
        // stale entry from a moved installation is reported as off so the UI can
        // offer to fix it.
        Ok(recorded
            .trim_matches('"')
            .eq_ignore_ascii_case(&executable.to_string_lossy()))
    }

    /// `GlobalMemoryStatusEx`. Windows reports physical memory directly, so
    /// there is nothing to infer.
    fn total_memory_mb(&self) -> Option<u64> {
        let mut status = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        unsafe { GlobalMemoryStatusEx(&mut status).ok()? };
        Some(status.ullTotalPhys / (1024 * 1024))
    }

    /// `GetDiskFreeSpaceExW` on the volume containing `path`.
    ///
    /// A path that does not exist yet (the models directory on a fresh
    /// install) still resolves through its existing parent, which is why the
    /// Windows API is asked with the directory rather than the file.
    fn free_disk_mb(&self, path: &Path) -> Option<u64> {
        let mut probe = path;
        while !probe.exists() {
            probe = probe.parent()?;
        }

        let wide: Vec<u16> = probe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut available: u64 = 0;
        unsafe {
            GetDiskFreeSpaceExW(PCWSTR(wide.as_ptr()), Some(&mut available), None, None).ok()?
        };
        Some(available / (1024 * 1024))
    }
}

/* ------------------------------------------------------------ internals --- */

/// Parameter block for the `EnumWindows` callback: find a process's window,
/// preferring one whose title matches.
struct FocusSearch {
    pid: u32,
    title: String,
    best: Option<HWND>,
    fallback: Option<HWND>,
}

/// `Word` → `Microsoft Word`, so the list reads the way a user speaks.
fn pretty_name(stem: &str) -> String {
    let lowered = stem.to_lowercase();
    for (spoken, executable) in OFFICE_ALIASES {
        if executable
            .trim_end_matches(".exe")
            .eq_ignore_ascii_case(&lowered)
        {
            let mut characters = spoken.chars();
            return match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => stem.to_string(),
            };
        }
    }

    let mut characters = stem.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
        None => stem.to_string(),
    }
}

/// The value written to the `Run` key: a quoted absolute path, which is what
/// Windows expects when the path contains spaces.
fn run_value(executable: &Path) -> String {
    format!("\"{}\"", executable.to_string_lossy())
}

fn open_with_shell(path: &Path) -> Result<()> {
    let target = path.to_string_lossy().into_owned();
    open_shell_target(&target)
}

/// Hand a validated path or URL to the Windows shell opener.
///
/// `ShellExecuteW` is given no parameters beyond the target itself, and the
/// target is never a user-authored command line: either it came from the
/// operating system's own application registry, or it passed
/// `airprompt_core::security`.
fn open_shell_target(target: &str) -> Result<()> {
    let operation = to_wide("open");
    let file = to_wide(target);

    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    // ShellExecuteW reports values of 32 or below as errors, historically.
    if result.0 as isize <= 32 {
        return Err(Error::Refused(format!(
            "Windows could not open \"{target}\""
        )));
    }

    Ok(())
}

/// Open one of Windows' own settings pages. The URI is a constant in this file,
/// never user input.
fn open_settings_page(uri: &str) -> Result<()> {
    open_shell_target(uri)
}

/// Read the microphone consent state from the registry.
///
/// Windows records the answer under a per-application key and a `NonPackaged`
/// key for classic desktop applications. Both are consulted: either one saying
/// `Deny` means the microphone is unavailable.
fn microphone_permission() -> PermissionState {
    const CONSENT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

    let mut seen_any = false;
    for subkey in [CONSENT.to_string(), format!(r"{CONSENT}\NonPackaged")] {
        if let Some(value) = read_reg_string(HKEY_CURRENT_USER, &subkey, "Value") {
            seen_any = true;
            match value.as_str() {
                "Allow" => {}
                "Deny" => return PermissionState::Denied,
                _ => return PermissionState::NotDetermined,
            }
        }
    }

    if seen_any {
        PermissionState::Granted
    } else {
        // No entry means the user has never been asked.
        PermissionState::NotDetermined
    }
}

fn clipboard() -> Result<arboard::Clipboard> {
    arboard::Clipboard::new()
        .map_err(|error| Error::Clipboard(format!("the clipboard is unavailable: {error}")))
}

fn read_clipboard_text() -> Option<String> {
    let mut clipboard = clipboard().ok()?;
    clipboard.get_text().ok()
}

fn set_clipboard_text(text: &str) -> Result<()> {
    let mut clipboard = clipboard()?;
    clipboard
        .set_text(text)
        .map_err(|error| Error::Clipboard(format!("the clipboard could not be updated: {error}")))
}

/// One synthetic keystroke.
fn key_input(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// One synthetic Unicode character.
fn unicode_input(unit: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE | flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Type text by synthesising Unicode key events.
///
/// This path uses no clipboard at all, which is why it is the fallback when the
/// clipboard holds something the user would not want to lose.
fn type_unicode(text: &str) -> Result<()> {
    const CHUNK: usize = 256;

    let units: Vec<u16> = text.encode_utf16().collect();
    if units.len() > 20_000 {
        return Err(Error::Insertion(
            "that much text cannot be typed safely; it is on the clipboard instead".into(),
        ));
    }

    for chunk in units.chunks(CHUNK) {
        let mut events: Vec<INPUT> = Vec::with_capacity(chunk.len() * 2);
        for &unit in chunk {
            events.push(unicode_input(unit, KEYBD_EVENT_FLAGS(0)));
            events.push(unicode_input(unit, KEYEVENTF_KEYUP));
        }

        let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
        if sent as usize != events.len() {
            return Err(Error::Insertion(
                "the keystrokes were blocked; the transcript is on the clipboard".into(),
            ));
        }
    }

    Ok(())
}

/// Synthesise Ctrl+V, the paste keystroke.
fn send_ctrl_v() -> bool {
    let events = [
        key_input(VK_CONTROL, KEYBD_EVENT_FLAGS(0)),
        key_input(VK_V, KEYBD_EVENT_FLAGS(0)),
        key_input(VK_V, KEYEVENTF_KEYUP),
        key_input(VK_CONTROL, KEYEVENTF_KEYUP),
    ];

    let sent = unsafe { SendInput(&events, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != events.len() {
        return false;
    }

    std::thread::sleep(std::time::Duration::from_millis(PASTE_SETTLE_MS));
    true
}

/* ------------------------------------------------------------- secrets --- */

/// API keys in the Windows Credential Manager.
///
/// The service name comes from the product identity, and keys written by an
/// earlier build are migrated on first read (spec §11, §33). The old entry is
/// left in place: a failed migration must not cost the user their key.
pub struct WindowsSecretStore;

impl WindowsSecretStore {
    pub fn new() -> Self {
        WindowsSecretStore
    }
}

impl Default for WindowsSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretStore for WindowsSecretStore {
    fn set(&self, key: &str, value: &str) -> Result<()> {
        let entry = keyring::Entry::new(identity::SECRET_SERVICE, key).map_err(|error| {
            Error::Store(format!("the credential store rejected the entry: {error}"))
        })?;
        entry
            .set_password(value)
            .map_err(|error| Error::Store(format!("the key could not be stored: {error}")))
    }

    fn get(&self, key: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(identity::SECRET_SERVICE, key).map_err(|error| {
            Error::Store(format!("the credential store rejected the entry: {error}"))
        })?;

        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => migrate_legacy_key(key),
            Err(error) => Err(Error::Store(format!(
                "the stored key could not be read: {error}"
            ))),
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let entry = keyring::Entry::new(identity::SECRET_SERVICE, key).map_err(|error| {
            Error::Store(format!("the credential store rejected the entry: {error}"))
        })?;

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

        tracing::info!("migrated a stored key from an earlier credential-store name");
        return Ok(Some(value));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_strings_round_trip() {
        let buffer = to_wide("AirPrompt");
        assert_eq!(buffer.last(), Some(&0));
        assert_eq!(from_wide(&buffer), "AirPrompt");
    }

    #[test]
    fn names_normalise_the_way_people_speak_them() {
        assert_eq!(normalize_name("  Google   CHROME.exe "), "google chrome");
        assert_eq!(normalize_name("VS Code"), "vs code");
        assert_eq!(normalize_name(""), "");
    }

    #[test]
    fn shell_interpreters_are_never_resolved() {
        for refused in ["cmd", "PowerShell.exe", "wscript", "mshta.exe", "wsl"] {
            assert!(
                resolve_application(refused).is_none(),
                "{refused} must never be treated as a launchable application"
            );
        }
    }

    #[test]
    fn a_command_line_is_refused_before_resolution() {
        for hostile in [
            "chrome && powershell -EncodedCommand Zg==",
            "calc; rm -rf /",
            "notepad $(whoami)",
            "cmd /c del /f C:\\",
        ] {
            assert!(
                security::looks_like_shell(hostile),
                "{hostile} must be recognisable as shell syntax"
            );
        }
    }

    #[test]
    fn the_run_key_value_is_a_quoted_absolute_path() {
        let value = run_value(Path::new(r"C:\Program Files\AirPrompt\airprompt.exe"));
        assert_eq!(value, r#""C:\Program Files\AirPrompt\airprompt.exe""#);
        assert!(value.starts_with('"') && value.ends_with('"'));
    }

    #[test]
    fn refused_executables_cover_the_script_hosts() {
        for interpreter in ["cmd.exe", "powershell.exe", "pwsh.exe", "cscript.exe"] {
            let normalized = normalize_name(interpreter);
            assert!(
                REFUSED_EXECUTABLES
                    .iter()
                    .any(|refused| normalize_name(refused) == normalized),
                "{interpreter} must be on the refusal list"
            );
        }
    }

    #[test]
    fn office_aliases_map_to_the_real_executables() {
        assert!(OFFICE_ALIASES
            .iter()
            .any(|(spoken, executable)| *spoken == "word" && *executable == "WINWORD.EXE"));
        assert_eq!(pretty_name("WINWORD"), "Word");
    }
}
