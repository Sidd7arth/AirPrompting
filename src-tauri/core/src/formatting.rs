//! Deterministic transcript formatting.
//!
//! Everything here runs before any optional AI enhancement, is instant, and is
//! fully testable. Three behaviours live here:
//!
//! * spoken punctuation is token based, so "comma" becomes "," and attaches to
//!   the preceding word;
//! * continuous dictation inserts the separating space and capitalises based on
//!   what is already in the target field;
//! * GAAV mode lowercases the first letter and drops a trailing period, which is
//!   what you want when dictating mid-sentence into a terminal.

use crate::storage::DictionaryEntry;

/* ------------------------------------------------------- filler handling */

/// Filler words removed when the user enables it. Multi-word entries are
/// matched before single words.
const FILLERS: &[&str] = &[
    "you know",
    "i mean",
    "sort of",
    "kind of",
    "basically",
    "literally",
    "um",
    "uh",
    "erm",
    "hmm",
    "mmm",
    "ah",
    "eh",
];

/* ---------------------------------------------------- spoken punctuation */

/// Spoken form -> literal form.
///
/// Split by how they attach to neighbouring words: `Attach` joins the previous
/// word with no space, `Pair` surrounds a phrase, and `Break` starts a new line.
#[derive(Clone, Copy)]
enum Punct {
    /// No space before; the next word starts a sentence.
    Attach(&'static str),
    /// No space after (opening bracket/quote).
    Open(&'static str),
    /// No space before (closing bracket/quote).
    Close(&'static str),
    /// Replaces the surrounding whitespace.
    Break(&'static str),
}

const SPOKEN_PUNCTUATION: &[(&str, Punct)] = &[
    ("full stop", Punct::Attach(".")),
    ("question mark", Punct::Attach("?")),
    ("exclamation mark", Punct::Attach("!")),
    ("exclamation point", Punct::Attach("!")),
    ("semicolon", Punct::Attach(";")),
    ("semi colon", Punct::Attach(";")),
    ("comma", Punct::Attach(",")),
    ("period", Punct::Attach(".")),
    ("colon", Punct::Attach(":")),
    ("ellipsis", Punct::Attach("…")),
    ("apostrophe", Punct::Attach("'")),
    ("open quote", Punct::Open("\u{201C}")),
    ("quote", Punct::Open("\u{201C}")),
    ("close quote", Punct::Close("\u{201D}")),
    ("unquote", Punct::Close("\u{201D}")),
    ("open parenthesis", Punct::Open("(")),
    ("open bracket", Punct::Open("[")),
    ("close parenthesis", Punct::Close(")")),
    ("close bracket", Punct::Close("]")),
    ("new paragraph", Punct::Break("\n\n")),
    ("new line", Punct::Break("\n")),
    ("newline", Punct::Break("\n")),
];

fn lookup_punctuation(token: &str) -> Option<Punct> {
    let normalized = token
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    SPOKEN_PUNCTUATION
        .iter()
        .find(|(phrase, _)| *phrase == normalized)
        .map(|(_, punct)| *punct)
}

/// Convert spoken punctuation words into real punctuation (spec §53).
///
/// A punctuation mark that ends a sentence also capitalises the next word, which
/// is what makes "hello comma world period" come out as "Hello, world.".
pub fn apply_spoken_punctuation(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut capitalize_next = true;

    for token in text.split_whitespace() {
        match lookup_punctuation(token) {
            Some(Punct::Attach(mark)) => {
                // Attach to the previous word, dropping the space between them.
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push_str(mark);
                capitalize_next = matches!(mark, "." | "!" | "?");
                if !capitalize_next {
                    out.push(' ');
                }
            }
            Some(Punct::Open(mark)) => {
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                out.push_str(mark);
            }
            Some(Punct::Close(mark)) => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push_str(mark);
                out.push(' ');
                capitalize_next = true;
            }
            Some(Punct::Break(mark)) => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push_str(mark);
                capitalize_next = true;
            }
            None => {
                if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                    out.push(' ');
                }
                if capitalize_next {
                    let mut chars = token.chars();
                    if let Some(first) = chars.next() {
                        out.extend(first.to_uppercase());
                        out.push_str(chars.as_str());
                        capitalize_next = false;
                        continue;
                    }
                }
                out.push_str(token);
                capitalize_next = false;
            }
        }
    }

    out.trim_end().to_string()
}

/* ------------------------------------------------------------- dictionary */

/// Apply user dictionary substitutions (spec §55).
///
/// Rules are tried longest-first so "Visual Studio Code" is not mangled by an
/// earlier "Code" rule. Matching is case-insensitive and compares by *character*
/// count, so multi-byte text cannot desynchronise the scan. The replacement's
/// own casing is preserved.
pub fn apply_dictionary(text: &str, entries: &[DictionaryEntry]) -> String {
    let mut rules: Vec<(String, usize, &DictionaryEntry)> = entries
        .iter()
        .filter(|e| e.enabled && !e.spoken.trim().is_empty())
        .map(|e| {
            let needle = e.spoken.trim().to_lowercase();
            let chars = needle.chars().count();
            (needle, chars, e)
        })
        .collect();
    rules.sort_by_key(|(needle, _, _)| std::cmp::Reverse(needle.len()));

    let mut result = String::with_capacity(text.len());
    let mut offset = 0usize;

    'scan: while offset < text.len() {
        for (needle, char_count, rule) in &rules {
            let candidate: String = text[offset..].chars().take(*char_count).collect();
            if candidate.chars().count() < *char_count || !candidate.to_lowercase().eq(needle) {
                continue;
            }
            let end = offset + candidate.len();

            // Whole-word mode requires non-word characters on both sides.
            if rule.whole_word {
                let before_ok = text[..offset]
                    .chars()
                    .next_back()
                    .map(|c| !c.is_alphanumeric())
                    .unwrap_or(true);
                let after_ok = text[end..]
                    .chars()
                    .next()
                    .map(|c| !c.is_alphanumeric())
                    .unwrap_or(true);
                if !before_ok || !after_ok {
                    continue;
                }
            }

            result.push_str(&rule.replacement);
            offset = end;
            continue 'scan;
        }
        // No rule matched here; copy one whole character.
        let ch = text[offset..].chars().next().unwrap();
        result.push(ch);
        offset += ch.len_utf8();
    }
    result
}

/* ---------------------------------------------------------- filler words */

/// Remove filler words, keeping surrounding spacing tidy (spec §40).
pub fn remove_fillers(text: &str) -> String {
    let mut words: Vec<&str> = text.split_whitespace().collect();
    let mut index = 0;
    while index < words.len() {
        // Try the longest filler phrase first so "you know" beats "you".
        let mut matched_len = 0;
        for filler in FILLERS {
            let parts: Vec<&str> = filler.split(' ').collect();
            if index + parts.len() > words.len() {
                continue;
            }
            let window: Vec<String> = words[index..index + parts.len()]
                .iter()
                .map(|w| {
                    w.trim_matches(|c: char| !c.is_alphanumeric())
                        .to_lowercase()
                })
                .collect();
            if window.join(" ") == *filler {
                matched_len = parts.len();
                break;
            }
        }
        if matched_len > 0 {
            words.drain(index..index + matched_len);
            continue;
        }
        index += 1;
    }
    let joined = words.join(" ");
    // Removing a filler can orphan leading punctuation ("um, hello" -> ", hello").
    joined
        .trim_start_matches([',', '.', '?', '!', ' '])
        .to_string()
}

/* ------------------------------------------------------------- case rules */

/// Capitalise the first alphabetic character of every sentence.
pub fn capitalize_sentences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_start = true;
    for ch in text.chars() {
        if at_start && ch.is_alphabetic() {
            out.extend(ch.to_uppercase());
            at_start = false;
            continue;
        }
        out.push(ch);
        if matches!(ch, '.' | '!' | '?') {
            at_start = true;
        } else if !ch.is_whitespace() {
            at_start = false;
        }
    }
    out
}

/// GAAV mode: lowercase the first letter and drop a trailing period.
///
/// Only the very first character is lowercased, not the whole first word, and
/// only when the text starts with an uppercase letter. This is the behaviour a
/// user dictating mid-sentence into a terminal expects.
pub fn apply_gaav(text: &str, lowercase_first: bool, remove_trailing_period: bool) -> String {
    let mut value = text.to_string();
    if remove_trailing_period {
        let trimmed = value.trim_end();
        if trimmed.ends_with('.') && !trimmed.ends_with("..") {
            value = trimmed[..trimmed.len() - 1].to_string();
        }
    }
    if lowercase_first {
        if let Some(first) = value.chars().next() {
            if first.is_uppercase() {
                let lowered: String = first.to_lowercase().collect();
                value = format!("{lowered}{}", &value[first.len_utf8()..]);
            }
        }
    }
    value
}

/* --------------------------------------------- continuous dictation (§52) */

/// Join a new transcript onto text already present in the target field.
///
/// Continuous dictation: a separating space is added only where one is missing,
/// and the new text is capitalised based on the last sentence boundary of the
/// text already in the target field.
pub fn apply_continuous(
    preceding: &str,
    text: &str,
    spacing_enabled: bool,
    smart_capitalization: bool,
) -> String {
    if text.is_empty() {
        return text.to_string();
    }
    let mut result = text.to_string();

    if smart_capitalization {
        let last_boundary = preceding
            .chars()
            .rev()
            .find(|c| !c.is_whitespace() && *c != '"' && *c != '\u{201D}');
        let starts_sentence = matches!(last_boundary, None | Some('.') | Some('!') | Some('?'));
        result = if starts_sentence {
            capitalize_sentences(&result)
        } else {
            let mut chars = result.chars();
            if let Some(first) = chars.next() {
                format!("{}{}", first.to_lowercase(), chars.as_str())
            } else {
                result
            }
        };
    }

    if spacing_enabled && !preceding.is_empty() {
        let preceding_ends_space = preceding
            .chars()
            .next_back()
            .map(|c| c.is_whitespace())
            .unwrap_or(true);
        let result_starts_space = result.starts_with(char::is_whitespace);
        if !preceding_ends_space && !result_starts_space {
            result.insert(0, ' ');
        }
    }

    result
}

/* ---------------------------------------------------------------- context */

/// What the formatter needs to know about the destination.
#[derive(Debug, Clone, Default)]
pub struct FormatContext {
    /// Text already in the target field before the caret.
    pub preceding_text: String,
    /// True when the focused control looks like code or a terminal, where
    /// sentence casing and a trailing period are usually unwanted.
    pub code_context: bool,
}

/// The user's formatting choices, flattened so the pipeline is one call.
#[derive(Debug, Clone)]
pub struct FormatOptions {
    pub spoken_punctuation: bool,
    pub remove_fillers: bool,
    pub auto_capitalization: bool,
    pub continuous_dictation: bool,
    pub trailing_space: bool,
    pub gaav_mode: bool,
    pub dictionary: Vec<DictionaryEntry>,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions {
            spoken_punctuation: true,
            remove_fillers: false,
            auto_capitalization: true,
            continuous_dictation: true,
            trailing_space: true,
            gaav_mode: false,
            dictionary: Vec::new(),
        }
    }
}

/// Run the full deterministic pipeline. Order matters and is deliberate:
/// substitutions first (so a dictionary entry is not split by filler removal),
/// then punctuation, then fillers, then casing and joining.
pub fn format_transcript(raw: &str, options: &FormatOptions, context: &FormatContext) -> String {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return String::new();
    }

    if !options.dictionary.is_empty() {
        text = apply_dictionary(&text, &options.dictionary);
    }

    if options.spoken_punctuation {
        text = apply_spoken_punctuation(&text);
    }

    if options.remove_fillers {
        text = remove_fillers(&text);
    }

    // A code context is not prose: leave the user's casing and final mark alone.
    let capitalize = options.auto_capitalization && !context.code_context;

    if options.continuous_dictation && !context.preceding_text.is_empty() {
        text = apply_continuous(&context.preceding_text, &text, true, capitalize);
    } else if capitalize {
        text = capitalize_sentences(&text);
    }

    if options.gaav_mode || context.code_context {
        text = apply_gaav(&text, true, true);
    }

    if options.trailing_space {
        text.push(' ');
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(spoken: &str, replacement: &str, whole_word: bool) -> DictionaryEntry {
        DictionaryEntry {
            id: 1,
            spoken: spoken.into(),
            replacement: replacement.into(),
            whole_word,
            enabled: true,
        }
    }

    /* ------------------------------------------------------ spoken punct */

    #[test]
    fn converts_the_canonical_spoken_punctuation_example() {
        let out = apply_spoken_punctuation("hello john comma how are you question mark");
        assert_eq!(out, "Hello John, how are you?");
    }

    #[test]
    fn attaches_punctuation_without_a_leading_space() {
        assert_eq!(apply_spoken_punctuation("wait comma stop"), "Wait, stop");
        assert_eq!(apply_spoken_punctuation("yes period"), "Yes.");
        assert_eq!(
            apply_spoken_punctuation("really exclamation mark"),
            "Really!"
        );
    }

    #[test]
    fn handles_brackets_and_quotes() {
        assert_eq!(
            apply_spoken_punctuation("open parenthesis note close parenthesis done"),
            "(Note) Done"
        );
        assert_eq!(
            apply_spoken_punctuation("he said open quote hello close quote"),
            "He said \u{201C}hello\u{201D}"
        );
    }

    #[test]
    fn new_line_and_paragraph_break_lines() {
        assert_eq!(
            apply_spoken_punctuation("first line new line second line"),
            "First line\nSecond line"
        );
        assert_eq!(
            apply_spoken_punctuation("para one new paragraph para two"),
            "Para one\n\nPara two"
        );
    }

    #[test]
    fn ordinary_words_are_left_alone() {
        assert_eq!(
            apply_spoken_punctuation("the report is almost finished"),
            "The report is almost finished"
        );
        // "periodic" contains "period" but is not the token "period".
        assert_eq!(apply_spoken_punctuation("periodic"), "Periodic");
    }

    #[test]
    fn capitalises_after_sentence_ends() {
        assert_eq!(
            apply_spoken_punctuation("one period two period three"),
            "One. Two. Three."
        );
    }

    /* -------------------------------------------------------- dictionary */

    #[test]
    fn replaces_dictionary_terms_case_insensitively() {
        let rules = vec![entry("next mart", "NxtMart", true)];
        assert_eq!(
            apply_dictionary("open next mart now", &rules),
            "open NxtMart now"
        );
        assert_eq!(
            apply_dictionary("Open Next Mart now", &rules),
            "Open NxtMart now"
        );
    }

    #[test]
    fn longest_dictionary_rule_wins() {
        let rules = vec![
            entry("code", "Code", true),
            entry("visual studio code", "VS Code", true),
        ];
        assert_eq!(
            apply_dictionary("I use visual studio code", &rules),
            "I use VS Code"
        );
        assert_eq!(apply_dictionary("I write code", &rules), "I write Code");
    }

    #[test]
    fn whole_word_rules_do_not_match_inside_words() {
        let rules = vec![entry("cat", "feline", true)];
        assert_eq!(
            apply_dictionary("cat concatenate", &rules),
            "feline concatenate"
        );

        let substring = vec![entry("cat", "feline", false)];
        assert_eq!(
            apply_dictionary("cat concatenate", &substring),
            "feline confelineenate"
        );
    }

    #[test]
    fn dictionary_handles_multibyte_text_safely() {
        // Must not panic slicing inside a multi-byte character.
        let rules = vec![entry("cafe", "Café", true)];
        let out = apply_dictionary("naïve café cafe", &rules);
        assert!(out.contains("Café"));
        assert!(out.contains("naïve"));
    }

    #[test]
    fn disabled_dictionary_entries_are_ignored() {
        let rules = vec![DictionaryEntry {
            enabled: false,
            ..entry("next mart", "NxtMart", true)
        }];
        assert_eq!(apply_dictionary("next mart", &rules), "next mart");
    }

    /* ----------------------------------------------------------- fillers */

    #[test]
    fn removes_single_and_multi_word_fillers() {
        assert_eq!(
            remove_fillers("um so I was uh thinking"),
            "so I was thinking"
        );
        assert_eq!(remove_fillers("it was, you know, fine"), "it was, fine");
    }

    #[test]
    fn filler_removal_keeps_real_words() {
        // "a" and "ah" are different tokens; a standalone "ah" is a filler.
        assert_eq!(remove_fillers("a cat sat"), "a cat sat");
        assert_eq!(remove_fillers("hello there"), "hello there");
    }

    /* -------------------------------------------------------------- GAAV */

    #[test]
    fn gaav_lowercases_only_the_first_letter() {
        assert_eq!(apply_gaav("Hello World.", true, false), "hello World.");
        // Already lowercase: untouched.
        assert_eq!(apply_gaav("hello World.", true, false), "hello World.");
    }

    #[test]
    fn gaav_removes_a_single_trailing_period() {
        assert_eq!(apply_gaav("Deploy it.", false, true), "Deploy it");
        // An ellipsis is intentional and is kept.
        assert_eq!(apply_gaav("Wait...", false, true), "Wait...");
        assert_eq!(
            apply_gaav("No trailing mark", false, true),
            "No trailing mark"
        );
    }

    #[test]
    fn gaav_combines_both_operations() {
        assert_eq!(apply_gaav("Done now.", true, true), "done now");
    }

    /* -------------------------------------------------- continuous joins */

    #[test]
    fn continuous_adds_a_separating_space_only_when_missing() {
        let joined = apply_continuous("Hello there", "how are you", true, false);
        assert_eq!(joined, " how are you");

        let already_spaced = apply_continuous("Hello there ", "how are you", true, false);
        assert_eq!(already_spaced, "how are you");
    }

    #[test]
    fn continuous_capitalises_after_a_sentence_boundary() {
        // After a period, the new text starts a sentence.
        assert_eq!(
            apply_continuous("Finished the report.", "next task", true, true),
            " Next task"
        );
        // Mid-sentence, it continues lowercased.
        assert_eq!(
            apply_continuous("I was saying", "that it works", true, true),
            " that it works"
        );
    }

    #[test]
    fn continuous_capitalises_at_the_start_of_an_empty_field() {
        assert_eq!(
            apply_continuous("", "hello there", true, true),
            "Hello there"
        );
    }

    /* -------------------------------------------------------- full pipeline */

    #[test]
    fn pipeline_matches_the_expected_dictation_result() {
        let out = format_transcript(
            "hello john comma how are you question mark",
            &FormatOptions::default(),
            &FormatContext::default(),
        );
        assert_eq!(out, "Hello John, how are you? ");
    }

    #[test]
    fn pipeline_joins_onto_existing_text() {
        let options = FormatOptions::default();
        let context = FormatContext {
            preceding_text: "I already wrote this.".into(),
            ..Default::default()
        };
        let out = format_transcript("and this follows", &options, &context);
        assert_eq!(out, " And this follows ");
    }

    #[test]
    fn pipeline_continues_a_sentence_without_capitalising() {
        let options = FormatOptions::default();
        let context = FormatContext {
            preceding_text: "I already wrote".into(),
            ..Default::default()
        };
        let out = format_transcript("that it works", &options, &context);
        assert_eq!(out, " that it works ");
    }

    #[test]
    fn code_context_leaves_casing_and_trailing_marks_alone() {
        let options = FormatOptions::default();
        let context = FormatContext {
            code_context: true,
            ..Default::default()
        };
        let out = format_transcript("const value equals one period", &options, &context);
        assert_eq!(out, "const value equals one ");
    }

    #[test]
    fn dictionary_applies_before_punctuation() {
        let options = FormatOptions {
            dictionary: vec![entry("next mart", "NxtMart", true)],
            ..Default::default()
        };
        let out = format_transcript(
            "open next mart comma then deploy period",
            &options,
            &FormatContext::default(),
        );
        assert_eq!(out, "Open NxtMart, then deploy. ");
    }

    #[test]
    fn empty_input_stays_empty() {
        let out = format_transcript("   ", &FormatOptions::default(), &FormatContext::default());
        assert_eq!(out, "");
    }

    #[test]
    fn trailing_space_can_be_disabled() {
        let options = FormatOptions {
            trailing_space: false,
            ..Default::default()
        };
        let out = format_transcript("deploy now", &options, &FormatContext::default());
        assert_eq!(out, "Deploy now");
    }

    #[test]
    fn auto_capitalization_can_be_disabled() {
        let options = FormatOptions {
            auto_capitalization: false,
            trailing_space: false,
            ..Default::default()
        };
        let out = format_transcript("deploy now", &options, &FormatContext::default());
        assert_eq!(out, "deploy now");
    }
}
