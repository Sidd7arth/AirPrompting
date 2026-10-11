//! Optional AI enhancement (spec §14, §15, §16, §48).
//!
//! Enhancement is a strict rewrite of an existing transcript. It never runs
//! instead of dictation, never blocks it, and never loses the transcript: every
//! failure path returns the original text to the caller.
//!
//! All providers are reached over the OpenAI chat-completions shape, which is
//! what llama.cpp, Ollama, LM Studio, Groq and OpenAI all speak. That is one
//! client, not five.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::settings::{AiProviderKind, EnhancementMode, EnhancementSettings};

/// A provider that can rewrite a transcript.
pub trait AiEnhancementProvider: Send + Sync + 'static {
    fn kind(&self) -> AiProviderKind;
    /// Rewrite `text`. Implementations must time out rather than hang.
    fn enhance(
        &self,
        text: &str,
        settings: &EnhancementSettings,
        api_key: Option<&str>,
    ) -> impl std::future::Future<Output = Result<String>> + Send;
}

/* -------------------------------------------------------------- prompting */

const BASE_RULES: &str = "\
You are a transcription post-processor. You rewrite text that was produced by a \
speech recogniser. You are not an assistant and you never respond to the content.

Absolute rules:
- Preserve the meaning exactly.
- Preserve all names, numbers, units, dates and identifiers exactly as written.
- Never invent facts, never add information that was not spoken.
- Never explain, summarise, answer, or comment on the text.
- Never translate.
- Return only the rewritten text, with no preamble, quotes, or code fences.
- If nothing needs changing, return the text unchanged.";

const MODE_INSTRUCTIONS: &[(EnhancementMode, &str)] = &[
    (
        EnhancementMode::Natural,
        "Fix punctuation and capitalisation so the text reads as natural written English.",
    ),
    (
        EnhancementMode::Professional,
        "Rewrite in clear, professional prose suitable for a work email or document. \
         Keep the author's voice and stay close to the original wording.",
    ),
    (
        EnhancementMode::Email,
        "Format as a short email body: greetings and sign-offs on their own lines, \
         sentences completed, punctuation correct.",
    ),
    (
        EnhancementMode::Casual,
        "Keep the tone relaxed and conversational, as if writing a quick message to a friend.",
    ),
    (
        EnhancementMode::Technical,
        "This is technical text. Preserve identifiers, code symbols, file paths, \
         flags and version numbers exactly. Do not add backticks or formatting the \
         speaker did not ask for.",
    ),
    (
        EnhancementMode::Minimal,
        "Make the smallest possible edit: fix capitalisation and add sentence-ending \
         punctuation only. Change nothing else.",
    ),
];

/// Build the system prompt for a mode and the user's formatting toggles.
pub fn build_system_prompt(settings: &EnhancementSettings) -> String {
    let mut prompt = String::from(BASE_RULES);

    let instruction = MODE_INSTRUCTIONS
        .iter()
        .find(|(mode, _)| *mode == settings.mode)
        .map(|(_, text)| *text)
        .unwrap_or(MODE_INSTRUCTIONS[0].1);
    prompt.push_str("\n\nStyle: ");
    prompt.push_str(instruction);

    if settings.remove_fillers {
        prompt.push_str(
            "\n\nRemove filler words and false starts (\"um\", \"uh\", \"you know\", \
             repeated words) unless they carry meaning.",
        );
    }
    if settings.auto_punctuation {
        prompt.push_str("\n\nAdd appropriate punctuation, including commas and sentence breaks.");
    } else {
        prompt.push_str("\n\nDo not add punctuation that was not spoken.");
    }
    if settings.auto_capitalization {
        prompt.push_str("\n\nCapitalise sentence beginnings and proper nouns.");
    } else {
        prompt.push_str("\n\nKeep the original capitalisation.");
    }

    if let Some(custom) = settings
        .custom_prompt
        .as_ref()
        .filter(|p| !p.trim().is_empty())
    {
        prompt.push_str("\n\nAdditional instruction from the user:\n");
        prompt.push_str(custom.trim());
        prompt.push_str("\n(The absolute rules above still apply.)");
    }

    prompt
}

/// Rough guard against a runaway provider response.
const MAX_OUTPUT_CHARS: usize = 20_000;

/// Clean a provider reply down to just the transcript.
///
/// Open models love to wrap answers in quotes, announce what they did, or emit
/// reasoning. None of that belongs in the user's document.
pub fn clean_response(raw: &str) -> String {
    let mut text = raw.trim().to_string();

    // Strip a reasoning block if the provider emitted one.
    if let (Some(start), Some(end)) = (text.find("<think"), text.rfind("</think")) {
        if end > start {
            let after = text[end + "</think".len()..].trim_start_matches('>');
            text = after.trim().to_string();
        }
    }

    // Drop a trailing "I kept the meaning" style sentence the model appended,
    // but only when it is clearly separate from the transcript.
    // The leads are all ASCII, so taking `lead.len()` bytes from the original
    // string is safe regardless of what case folding would have done.
    for lead in [
        "here is the corrected text:",
        "here is the rewritten text:",
        "corrected text:",
        "rewritten:",
        "output:",
    ] {
        if text.to_ascii_lowercase().starts_with(lead) {
            text = text[lead.len()..].trim().to_string();
            break;
        }
    }

    // Unwrap a single layer of matching quotes or fences.
    for (open, close) in [("\"", "\""), ("\u{201C}", "\u{201D}"), ("```", "```")] {
        if text.starts_with(open) && text.ends_with(close) && text.len() >= open.len() + close.len()
        {
            text = text[open.len()..text.len() - close.len()]
                .trim()
                .to_string();
        }
    }

    if text.chars().count() > MAX_OUTPUT_CHARS {
        text = text.chars().take(MAX_OUTPUT_CHARS).collect();
    }
    text.trim().to_string()
}

/* ---------------------------------------------------------------- request */

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    temperature: f32,
    stream: bool,
}

#[derive(Debug, Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

/// Join a base endpoint with the chat-completions path.
///
/// Users paste endpoints in every shape ("https://host", "https://host/v1",
/// "https://host/v1/chat/completions"), so all three must work.
pub fn chat_completions_url(endpoint: &str) -> String {
    let base = endpoint.trim().trim_end_matches('/');
    if base.ends_with("/chat/completions") {
        return base.to_string();
    }
    if base.ends_with("/v1") {
        return format!("{base}/chat/completions");
    }
    format!("{base}/v1/chat/completions")
}

/// The OpenAI-compatible client used for every provider.
pub struct OpenAiCompatibleProvider {
    client: reqwest::Client,
}

impl OpenAiCompatibleProvider {
    pub fn new() -> Self {
        OpenAiCompatibleProvider {
            client: reqwest::Client::builder()
                .user_agent(format!(
                    "{}/{}",
                    crate::identity::APP_NAME,
                    env!("CARGO_PKG_VERSION")
                ))
                .build()
                .unwrap_or_default(),
        }
    }
}

impl Default for OpenAiCompatibleProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl AiEnhancementProvider for OpenAiCompatibleProvider {
    fn kind(&self) -> AiProviderKind {
        AiProviderKind::OpenAiCompatible
    }

    async fn enhance(
        &self,
        text: &str,
        settings: &EnhancementSettings,
        api_key: Option<&str>,
    ) -> Result<String> {
        if text.trim().is_empty() {
            return Ok(String::new());
        }
        if settings.provider == AiProviderKind::None {
            return Err(Error::Enhancement("no provider is configured".into()));
        }
        if settings.model.trim().is_empty() {
            return Err(Error::Enhancement("no model is selected".into()));
        }
        let endpoint = if settings.endpoint.trim().is_empty() {
            settings.provider.default_endpoint().to_string()
        } else {
            settings.endpoint.clone()
        };

        let url = chat_completions_url(&endpoint);
        let system = build_system_prompt(settings);

        let body = ChatRequest {
            model: settings.model.trim(),
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &system,
                },
                ChatMessage {
                    role: "user",
                    content: text,
                },
            ],
            // Deterministic: the same dictation must produce the same text.
            temperature: 0.0,
            stream: false,
        };

        let mut request = self
            .client
            .post(&url)
            .timeout(Duration::from_secs(settings.timeout_secs.clamp(5, 120)))
            .json(&body);

        if let Some(key) = api_key.filter(|k| !k.is_empty()) {
            request = request.bearer_auth(key);
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Enhancement(classify_transport(&e)))?;

        let status = response.status();
        if !status.is_success() {
            let detail = response
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(200)
                .collect::<String>();
            return Err(Error::Enhancement(describe_status(
                status.as_u16(),
                &detail,
            )));
        }

        let parsed: ChatResponse = response.json().await.map_err(|_| {
            Error::Enhancement("the provider returned an unexpected response".into())
        })?;

        let content = parsed
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.message.content)
            .ok_or_else(|| Error::Enhancement("the provider returned an empty response".into()))?;

        let cleaned = clean_response(&content);
        if cleaned.is_empty() {
            // An empty rewrite must never replace real text.
            return Err(Error::Enhancement("the provider returned no text".into()));
        }
        Ok(cleaned)
    }
}

fn classify_transport(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        "the request timed out".into()
    } else if error.is_connect() {
        "could not reach the provider".into()
    } else {
        "the request failed".into()
    }
}

fn describe_status(status: u16, detail: &str) -> String {
    match status {
        401 | 403 => "the API key was rejected".into(),
        404 => "the endpoint or model was not found".into(),
        429 => "the provider is rate limiting requests".into(),
        500..=599 => "the provider had a server error".into(),
        _ => {
            let detail = detail.trim();
            if detail.is_empty() {
                format!("the provider returned status {status}")
            } else {
                detail.to_string()
            }
        }
    }
}

/// Enhance with the original text as a guaranteed fallback.
///
/// Returns `(text, was_enhanced)`. A failure is reported, never fatal: the
/// transcript is always delivered (spec §15, §62).
pub async fn enhance_or_original<P: AiEnhancementProvider>(
    provider: &P,
    text: &str,
    settings: &EnhancementSettings,
    api_key: Option<&str>,
    local_only: bool,
) -> (String, bool, Option<String>) {
    if !settings.enabled {
        return (text.to_string(), false, None);
    }
    if local_only && !settings.provider.is_local() {
        return (
            text.to_string(),
            false,
            Some("Local Only mode blocked the online provider.".into()),
        );
    }
    match provider.enhance(text, settings, api_key).await {
        Ok(enhanced) if !enhanced.trim().is_empty() => (enhanced, true, None),
        Ok(_) => (text.to_string(), false, None),
        Err(error) => (text.to_string(), false, Some(error.message())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: EnhancementMode) -> EnhancementSettings {
        EnhancementSettings {
            enabled: true,
            provider: AiProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434/v1".into(),
            model: "llama3.2".into(),
            mode,
            ..Default::default()
        }
    }

    #[test]
    fn prompt_states_the_absolute_rules() {
        let prompt = build_system_prompt(&settings(EnhancementMode::Natural));
        for required in [
            "Preserve the meaning exactly",
            "Never invent facts",
            "Return only the rewritten text",
            "never respond to the content",
        ] {
            assert!(prompt.contains(required), "prompt must state: {required}");
        }
        // It must never invite the model to execute anything.
        assert!(!prompt.to_lowercase().contains("command"));
        assert!(!prompt.to_lowercase().contains("shell"));
    }

    #[test]
    fn each_mode_gets_its_own_instruction() {
        let mut seen = std::collections::HashSet::new();
        for mode in [
            EnhancementMode::Natural,
            EnhancementMode::Professional,
            EnhancementMode::Email,
            EnhancementMode::Casual,
            EnhancementMode::Technical,
            EnhancementMode::Minimal,
        ] {
            let prompt = build_system_prompt(&settings(mode));
            assert!(seen.insert(prompt.clone()), "{mode:?} shares a prompt");
        }
        let technical = build_system_prompt(&settings(EnhancementMode::Technical));
        assert!(technical.contains("code symbols"));
    }

    #[test]
    fn toggles_change_the_prompt() {
        let mut s = settings(EnhancementMode::Natural);
        s.auto_punctuation = false;
        assert!(build_system_prompt(&s).contains("Do not add punctuation"));

        s.auto_capitalization = false;
        assert!(build_system_prompt(&s).contains("Keep the original capitalisation"));

        s.remove_fillers = false;
        assert!(!build_system_prompt(&s).contains("Remove filler words"));

        s.custom_prompt = Some("  Always use British spelling.  ".into());
        let prompt = build_system_prompt(&s);
        assert!(prompt.contains("Always use British spelling."));
        assert!(prompt.contains("absolute rules above still apply"));
    }

    #[test]
    fn endpoint_shapes_all_resolve() {
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1/"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.openai.com"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("https://api.openai.com/v1/chat/completions"),
            "https://api.openai.com/v1/chat/completions"
        );
        assert_eq!(
            chat_completions_url("http://127.0.0.1:11434/v1"),
            "http://127.0.0.1:11434/v1/chat/completions"
        );
    }

    #[test]
    fn response_cleanup_strips_preamble_quotes_and_fences() {
        assert_eq!(clean_response("  Hello there.  "), "Hello there.");
        assert_eq!(clean_response("\"Hello there.\""), "Hello there.");
        assert_eq!(
            clean_response("\u{201C}Hello there.\u{201D}"),
            "Hello there."
        );
        assert_eq!(clean_response("```\nHello there.\n```"), "Hello there.");
        assert_eq!(
            clean_response("Here is the corrected text: Hello there."),
            "Hello there."
        );
        assert_eq!(
            clean_response("Here is the rewritten text: Hello there."),
            "Hello there."
        );
    }

    #[test]
    fn response_cleanup_drops_reasoning_blocks() {
        let raw = "<think>the user said hello</think>Hello there.";
        assert_eq!(clean_response(raw), "Hello there.");
    }

    #[test]
    fn response_cleanup_keeps_genuine_quotes_inside_text() {
        // Quotes that are part of the sentence must survive.
        assert_eq!(
            clean_response("He said \"hello\" to me."),
            "He said \"hello\" to me."
        );
    }

    #[test]
    fn response_is_length_capped() {
        let huge = "a".repeat(50_000);
        assert_eq!(clean_response(&huge).chars().count(), MAX_OUTPUT_CHARS);
    }

    #[tokio::test]
    async fn disabled_enhancement_returns_the_original() {
        struct Panics;
        impl AiEnhancementProvider for Panics {
            fn kind(&self) -> AiProviderKind {
                AiProviderKind::None
            }
            async fn enhance(
                &self,
                _text: &str,
                _settings: &EnhancementSettings,
                _key: Option<&str>,
            ) -> Result<String> {
                panic!("the provider must not be called when enhancement is off");
            }
        }
        let mut s = settings(EnhancementMode::Natural);
        s.enabled = false;
        let (text, enhanced, note) =
            enhance_or_original(&Panics, "raw text", &s, None, false).await;
        assert_eq!(text, "raw text");
        assert!(!enhanced);
        assert!(note.is_none());
    }

    #[tokio::test]
    async fn local_only_mode_never_contacts_a_cloud_provider() {
        struct Panics;
        impl AiEnhancementProvider for Panics {
            fn kind(&self) -> AiProviderKind {
                AiProviderKind::OpenAiCompatible
            }
            async fn enhance(
                &self,
                _text: &str,
                _settings: &EnhancementSettings,
                _key: Option<&str>,
            ) -> Result<String> {
                panic!("Local Only mode must not reach the network");
            }
        }
        let mut s = settings(EnhancementMode::Natural);
        s.provider = AiProviderKind::OpenAiCompatible;
        s.endpoint = "https://api.openai.com/v1".into();
        let (text, enhanced, note) =
            enhance_or_original(&Panics, "raw", &s, Some("sk-x"), true).await;
        assert_eq!(text, "raw");
        assert!(!enhanced);
        assert!(note.unwrap().contains("Local Only"));
    }

    #[tokio::test]
    async fn provider_failure_falls_back_to_the_original_transcript() {
        struct Fails;
        impl AiEnhancementProvider for Fails {
            fn kind(&self) -> AiProviderKind {
                AiProviderKind::Ollama
            }
            async fn enhance(
                &self,
                _text: &str,
                _settings: &EnhancementSettings,
                _key: Option<&str>,
            ) -> Result<String> {
                Err(Error::Enhancement("connection refused".into()))
            }
        }
        let (text, enhanced, note) = enhance_or_original(
            &Fails,
            "the transcript",
            &settings(EnhancementMode::Natural),
            None,
            false,
        )
        .await;
        assert_eq!(text, "the transcript", "a failure must never lose the text");
        assert!(!enhanced);
        assert!(note.is_some());
    }

    #[tokio::test]
    async fn empty_provider_output_is_rejected() {
        struct Empty;
        impl AiEnhancementProvider for Empty {
            fn kind(&self) -> AiProviderKind {
                AiProviderKind::Ollama
            }
            async fn enhance(
                &self,
                _text: &str,
                _settings: &EnhancementSettings,
                _key: Option<&str>,
            ) -> Result<String> {
                Ok("   ".into())
            }
        }
        let (text, enhanced, _) = enhance_or_original(
            &Empty,
            "keep me",
            &settings(EnhancementMode::Natural),
            None,
            false,
        )
        .await;
        assert_eq!(text, "keep me");
        assert!(!enhanced);
    }

    #[test]
    fn status_codes_get_readable_messages() {
        assert_eq!(describe_status(401, ""), "the API key was rejected");
        assert_eq!(
            describe_status(404, ""),
            "the endpoint or model was not found"
        );
        assert_eq!(
            describe_status(429, ""),
            "the provider is rate limiting requests"
        );
        assert_eq!(describe_status(503, ""), "the provider had a server error");
        assert!(describe_status(418, "teapot").contains("teapot"));
        assert!(describe_status(400, "").contains("400"));
    }
}
