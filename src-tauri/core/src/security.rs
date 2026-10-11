//! Safety policy (spec §28, §29, §68, §99, §100, §101).
//!
//! The single most important rule in this file: **no code path turns model
//! output into a shell command.** Voice commands are parsed into a closed set
//! of intents, and every intent is executed by calling a specific platform
//! function. There is no `sh -c`, no `powershell -Command`, no `Command::new`
//! fed from transcribed text anywhere in the project.

use crate::error::{Error, Result};

/// The complete allowlist of actions the command engine may perform.
///
/// Adding an entry here is the *only* way to add a capability, and each one is
/// implemented by a named function in the platform adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionKind {
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

impl ActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ActionKind::OpenApp => "open_app",
            ActionKind::OpenUrl => "open_url",
            ActionKind::OpenFile => "open_file",
            ActionKind::OpenFolder => "open_folder",
            ActionKind::RunShortcut => "run_shortcut",
            ActionKind::CopyText => "copy_text",
            ActionKind::SearchWeb => "search_web",
            ActionKind::Dictate => "dictate",
            ActionKind::StopDictation => "stop_dictation",
            ActionKind::ChangeModel => "change_model",
            ActionKind::OpenSettings => "open_settings",
        }
    }

    /// Risk classification used to decide whether confirmation is required.
    ///
    /// Nothing in the current allowlist is high risk: high-risk actions
    /// (running a script, deleting a file) simply do not exist as intents. The
    /// variant is kept so the policy is explicit rather than implied.
    pub fn risk(self) -> RiskLevel {
        match self {
            ActionKind::OpenApp
            | ActionKind::OpenUrl
            | ActionKind::OpenFile
            | ActionKind::OpenFolder
            | ActionKind::CopyText
            | ActionKind::SearchWeb
            | ActionKind::Dictate
            | ActionKind::StopDictation
            | ActionKind::ChangeModel
            | ActionKind::OpenSettings => RiskLevel::Low,
            ActionKind::RunShortcut => RiskLevel::Medium,
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

/// Why an action was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Allowed,
    /// Allowed, but the user must confirm first.
    NeedsConfirmation(&'static str),
    Blocked(&'static str),
}

impl Verdict {
    pub fn is_blocked(&self) -> bool {
        matches!(self, Verdict::Blocked(_))
    }
}

/// URI schemes the app is willing to hand to the operating system.
///
/// Deliberately tiny. `file:` is excluded from URL opening because `open_path`
/// is the intent that handles local files, and it applies path validation;
/// allowing arbitrary `file:` URLs here would bypass that.
pub const ALLOWED_SCHEMES: &[&str] = &["http", "https"];

/// Schemes known to be dangerous to launch from a spoken phrase.
const DANGEROUS_SCHEMES: &[&str] = &[
    "javascript",
    "data",
    "vbscript",
    "file",
    "ms-msdt",
    "search-ms",
    "shell",
    "command",
    "powershell",
    "pwsh",
    "bash",
    "sh",
    "zsh",
    "cscript",
    "wscript",
    "reg",
    "sc",
    "mshta",
    "rundll32",
];

/// Validate a URL before it reaches the OS opener (spec §68).
pub fn validate_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::Refused("the address was empty".into()));
    }
    // Whitespace inside a URL is a strong sign of a malformed or hostile value.
    if trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(Error::Refused("the address contained spaces".into()));
    }

    let (scheme, rest) = match trimmed.split_once("://") {
        Some((scheme, rest)) => (scheme.to_ascii_lowercase(), rest),
        // A bare host like "github.com" is normalised to https rather than
        // passed through, so the scheme can never be attacker-chosen.
        None => {
            if trimmed.contains(':') && !trimmed.contains('.') {
                // Something like "mailto:x" or "cmd:calc" — refuse rather than guess.
                return Err(Error::Refused("unsupported link format".into()));
            }
            return validate_url(&format!("https://{trimmed}"));
        }
    };

    if DANGEROUS_SCHEMES.contains(&scheme.as_str()) || !ALLOWED_SCHEMES.contains(&scheme.as_str()) {
        return Err(Error::Refused(format!(
            "the '{scheme}' link type is not allowed"
        )));
    }
    if rest.is_empty() {
        return Err(Error::Refused("the address had no host".into()));
    }
    // Credentials embedded in a URL (`https://user:pass@host`) are a phishing
    // vector; the host after the `@` is what a user reads.
    if rest.split('/').next().unwrap_or("").contains('@') {
        return Err(Error::Refused("the address contained credentials".into()));
    }
    Ok(format!("{scheme}://{rest}"))
}

/// Validate a filesystem path before opening it (spec §29).
pub fn validate_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::Refused("no file was named".into()));
    }
    if trimmed.contains('\0') || trimmed.chars().any(|c| c.is_control()) {
        return Err(Error::Refused(
            "the path contained control characters".into(),
        ));
    }
    // Refuse obviously executable targets. Opening a folder is a convenience;
    // launching an arbitrary binary from a spoken phrase is not.
    let lower = trimmed.to_ascii_lowercase();
    for extension in [
        ".exe", ".bat", ".cmd", ".com", ".scr", ".msi", ".ps1", ".vbs", ".js", ".jar", ".sh",
        ".command", ".app", ".dll", ".pif", ".hta", ".lnk", ".reg",
    ] {
        if lower.ends_with(extension) {
            return Err(Error::Refused(
                "executables cannot be launched from a spoken command".into(),
            ));
        }
    }
    Ok(trimmed.to_string())
}

/// Decide whether an action may proceed, given its kind and the parsed target.
pub fn judge(kind: ActionKind, target: &str) -> Verdict {
    if target.trim().is_empty() {
        return Verdict::Blocked("no target was recognised");
    }
    match kind.risk() {
        RiskLevel::Low => Verdict::Allowed,
        RiskLevel::Medium => Verdict::NeedsConfirmation("this will run a shortcut on your Mac"),
        // Unreachable with the current allowlist; kept so the policy is explicit.
        RiskLevel::High => Verdict::Blocked("this action is not permitted"),
    }
}

/// Text that must never be treated as a command, whatever it contains.
///
/// Generated at parse time rather than trusted from the model: the parser's
/// output is a value, and this checks the value.
pub fn looks_like_shell(text: &str) -> bool {
    // Multi-character operators are only meaningful to a shell; a normal spoken
    // phrase never contains them.
    const MARKERS: &[&str] = &[
        "&&",
        "||",
        "|",
        "$(",
        "`",
        ">",
        "<",
        ";",
        "\\n",
        "rm -",
        "sudo ",
        "chmod ",
        "mkfs",
        "del /f",
        "format ",
        "shutdown ",
        "/dev/sd",
        "curl ",
        "wget ",
        "Invoke-Expression",
        "Start-Process",
        "-EncodedCommand",
    ];
    let lower = text.to_ascii_lowercase();
    MARKERS
        .iter()
        .any(|m| lower.contains(&m.to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_bare_hosts_to_https() {
        assert_eq!(validate_url("github.com").unwrap(), "https://github.com");
        assert_eq!(
            validate_url("  https://example.com/a?b=c  ").unwrap(),
            "https://example.com/a?b=c"
        );
    }

    #[test]
    fn rejects_dangerous_schemes() {
        for bad in [
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>",
            "ms-msdt:/id",
            "search-ms:query=x",
        ] {
            assert!(
                validate_url(bad).is_err(),
                "{bad} must be refused, not opened"
            );
        }
        // A bare scheme with no host is refused rather than guessed at.
        assert!(validate_url("mailto:someone@example.com").is_err());
    }

    #[test]
    fn rejects_credentials_and_whitespace() {
        assert!(validate_url("https://user:pass@evil.test").is_err());
        assert!(validate_url("https://exa mple.com").is_err());
        assert!(validate_url("   ").is_err());
    }

    #[test]
    fn accepts_ordinary_sites() {
        for good in [
            "https://youtube.com",
            "http://localhost:3000",
            "github.com/some/repo",
        ] {
            assert!(validate_url(good).is_ok(), "{good} should be allowed");
        }
    }

    #[test]
    fn path_validation_refuses_executables() {
        for bad in [
            "~/Downloads/setup.exe",
            "C:\\tmp\\thing.bat",
            "run.sh",
            "Tool.app",
            "script.ps1",
        ] {
            assert!(validate_path(bad).is_err(), "{bad} must be refused");
        }
        assert!(validate_path("~/Documents").is_ok());
        assert!(validate_path("~/Projects/my-app").is_ok());
        assert!(validate_path(" ").is_err());
    }

    #[test]
    fn shell_markers_are_detected() {
        for bad in [
            "rm -rf /",
            "calc && whoami",
            "echo `id`",
            "cat /etc/passwd | mail",
            "Invoke-Expression 'x'",
        ] {
            assert!(looks_like_shell(bad), "{bad} looks like a shell command");
        }
        for good in [
            "open safari",
            "open downloads folder",
            "run my focus shortcut",
        ] {
            assert!(!looks_like_shell(good), "{good} is an ordinary phrase");
        }
    }

    #[test]
    fn risk_policy_is_explicit() {
        assert_eq!(ActionKind::OpenApp.risk(), RiskLevel::Low);
        assert_eq!(ActionKind::OpenFolder.risk(), RiskLevel::Low);
        assert_eq!(ActionKind::RunShortcut.risk(), RiskLevel::Medium);

        assert_eq!(judge(ActionKind::OpenApp, "Safari"), Verdict::Allowed);
        assert!(matches!(
            judge(ActionKind::RunShortcut, "Focus"),
            Verdict::NeedsConfirmation(_)
        ));
        assert!(judge(ActionKind::OpenApp, "  ").is_blocked());
    }

    #[test]
    fn no_intent_can_be_high_risk() {
        // The allowlist must never contain something auto-executable at high risk.
        for kind in [
            ActionKind::OpenApp,
            ActionKind::OpenUrl,
            ActionKind::OpenFile,
            ActionKind::OpenFolder,
            ActionKind::RunShortcut,
            ActionKind::CopyText,
            ActionKind::SearchWeb,
            ActionKind::Dictate,
            ActionKind::StopDictation,
            ActionKind::ChangeModel,
            ActionKind::OpenSettings,
        ] {
            assert_ne!(
                kind.risk(),
                RiskLevel::High,
                "{} must not be reachable without confirmation",
                kind.as_str()
            );
        }
    }
}
