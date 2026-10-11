//! Speech recognition abstraction and streaming transcript state (spec §8, §52).
//!
//! The engine trait deliberately deals in finished audio buffers rather than a
//! token stream: every backend the app supports (a whisper.cpp process, Apple
//! Speech, Parakeet) accepts a slice of 16 kHz mono samples and returns text.
//! Streaming is achieved by re-running on a growing window, and
//! [`StreamingTranscript`] turns that noisy sequence of hypotheses into a
//! stable committed prefix plus a volatile tail.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// What the overlay shows right now.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    /// Text that will not change again.
    pub committed: String,
    /// Text that is still being revised.
    pub interim: String,
}

impl Transcript {
    pub fn is_empty(&self) -> bool {
        self.committed.trim().is_empty() && self.interim.trim().is_empty()
    }

    /// Committed text plus the current best guess for the tail.
    pub fn full(&self) -> String {
        match (self.committed.trim(), self.interim.trim()) {
            ("", interim) => interim.to_string(),
            (committed, "") => committed.to_string(),
            (committed, interim) => format!("{committed} {interim}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EngineOptions {
    pub language: String,
    /// 0 lets the engine choose.
    pub threads: u32,
    pub use_gpu: bool,
}

impl Default for EngineOptions {
    fn default() -> Self {
        EngineOptions {
            language: "en".into(),
            threads: 0,
            use_gpu: true,
        }
    }
}

/// A speech-to-text backend.
pub trait SpeechEngine: Send + Sync + 'static {
    /// Stable identifier for diagnostics and settings.
    fn id(&self) -> &'static str;

    /// Human name shown in the Speech settings section.
    fn display_name(&self) -> &'static str;

    /// True once a model is loaded and `transcribe` can be called.
    fn is_ready(&self) -> bool;

    /// Load `model_path`. Idempotent: calling it again with the same path must
    /// not reload, so the settings screen can call it freely.
    fn prepare(&self, model_path: &Path, options: &EngineOptions) -> Result<()>;

    /// Transcribe 16 kHz mono samples in [-1.0, 1.0].
    fn transcribe(&self, samples: &[f32]) -> Result<String>;

    /// Release the model. Safe to call when nothing is loaded.
    fn unload(&self);

    /// True when the backend is usable at all on this machine.
    fn is_available(&self) -> bool {
        true
    }

    /// Why the backend is unavailable, for the settings UI.
    fn unavailable_reason(&self) -> Option<String> {
        None
    }
}

/* ------------------------------------------------------- streaming state */

/// Turns a sequence of re-decoded hypotheses into committed + interim text.
///
/// Each hypothesis is a full decode of the audio so far. Words only become
/// committed once two consecutive hypotheses agree on them, which is what stops
/// the overlay from flickering (spec §52).
#[derive(Debug, Clone, Default)]
pub struct StreamingTranscript {
    committed: Vec<String>,
    interim: String,
    previous: Option<Vec<String>>,
}

impl StreamingTranscript {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the newest full hypothesis. Returns the updated transcript.
    pub fn push(&mut self, hypothesis: &str) -> Transcript {
        let words: Vec<String> = hypothesis
            .split_whitespace()
            .map(|w| w.to_string())
            .collect();

        if words.is_empty() {
            return self.snapshot();
        }

        // How far the two most recent decodes agree, word for word.
        let agreement = match &self.previous {
            Some(previous) => previous
                .iter()
                .zip(words.iter())
                .take_while(|(a, b)| a == b)
                .count(),
            // The very first decode has nothing to agree with, so nothing is
            // committed from it; that keeps the tail editable.
            None => 0,
        };

        if agreement > self.committed.len() {
            self.committed = words[..agreement].to_vec();
        }

        self.interim = words[self.committed.len().min(words.len())..].join(" ");
        self.previous = Some(words);
        self.snapshot()
    }

    /// Freeze everything, including the current interim guess, as final.
    ///
    /// Called when the take ends: the last decode is authoritative even though
    /// it was never confirmed by a following one.
    pub fn finish(&mut self) -> Transcript {
        if !self.interim.trim().is_empty() {
            let tail: Vec<String> = self.interim.split_whitespace().map(String::from).collect();
            self.committed.extend(tail);
            self.interim.clear();
        }
        self.previous = None;
        self.snapshot()
    }

    /// Replace the committed text outright, for a final authoritative decode.
    pub fn reset_to(&mut self, text: &str) -> Transcript {
        self.committed = text.split_whitespace().map(String::from).collect();
        self.interim.clear();
        self.previous = None;
        self.snapshot()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn snapshot(&self) -> Transcript {
        Transcript {
            committed: self.committed.join(" "),
            interim: self.interim.clone(),
        }
    }
}

/// Choose a worker thread count when the user has not chosen one.
///
/// Leaving at least one core for the UI keeps the app responsive while a model
/// decodes, which matters more than shaving milliseconds off transcription.
pub fn default_threads(available_parallelism: usize) -> u32 {
    let usable = available_parallelism.saturating_sub(1);
    usable.clamp(1, 8) as u32
}

/// Reject audio that is too short to be speech, before spending time on a
/// decode. Whisper hallucinates on sub-second buffers.
pub const MIN_AUDIO_SAMPLES: usize = 16_000 / 4; // 250 ms

pub fn ensure_audio_is_decodable(samples: &[f32]) -> Result<()> {
    if samples.len() < MIN_AUDIO_SAMPLES {
        return Err(Error::Transcription("the recording was too short".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_committed_from_a_single_hypothesis() {
        let mut stream = StreamingTranscript::new();
        let t = stream.push("I need to finish");
        assert_eq!(t.committed, "", "an unconfirmed decode must stay editable");
        assert_eq!(t.interim, "I need to finish");
    }

    #[test]
    fn confirmed_prefix_becomes_committed() {
        let mut stream = StreamingTranscript::new();
        stream.push("I need to finish");
        let t = stream.push("I need to finish the report");

        assert_eq!(t.committed, "I need to finish");
        assert_eq!(t.interim, "the report");
        assert_eq!(t.full(), "I need to finish the report");
    }

    #[test]
    fn committed_text_never_retreats() {
        let mut stream = StreamingTranscript::new();
        stream.push("I need to finish");
        stream.push("I need to finish the report");
        // A later, shorter decode must not lose the committed prefix.
        let t = stream.push("I need");
        assert_eq!(t.committed, "I need to finish");
    }

    #[test]
    fn repeated_identical_decodes_commit_incrementally() {
        let mut stream = StreamingTranscript::new();
        stream.push("hello this is a test");
        let t = stream.push("hello this is a test");
        assert_eq!(t.committed, "hello this is a test");
        assert_eq!(t.interim, "");
        assert_eq!(t.full(), "hello this is a test");
    }

    #[test]
    fn finish_promotes_the_tail() {
        let mut stream = StreamingTranscript::new();
        stream.push("hello this is");
        let finalised = stream.finish();
        assert_eq!(finalised.committed, "hello this is");
        assert_eq!(finalised.interim, "");
        assert_eq!(finalised.full(), "hello this is");
    }

    #[test]
    fn reset_to_replaces_everything() {
        let mut stream = StreamingTranscript::new();
        stream.push("garbled guess");
        stream.push("garbled guess two");
        let t = stream.reset_to("the authoritative final decode");
        assert_eq!(t.committed, "the authoritative final decode");
        assert_eq!(t.interim, "");

        stream.clear();
        assert!(stream.snapshot().is_empty());
    }

    #[test]
    fn empty_hypotheses_are_ignored() {
        let mut stream = StreamingTranscript::new();
        stream.push("keep me");
        let t = stream.push("   ");
        assert_eq!(t.interim, "keep me");
    }

    #[test]
    fn punctuation_only_change_does_not_commit_early() {
        let mut stream = StreamingTranscript::new();
        stream.push("hello world");
        // The re-decode differs, so only the shared words commit.
        let t = stream.push("hello, world!");
        assert_eq!(
            t.committed,
            "hello".replace("hello", "hello,").replace("hello,", "")
        );
        assert_eq!(t.interim, "world!");
    }

    #[test]
    fn thread_default_leaves_headroom() {
        assert_eq!(default_threads(1), 1);
        assert_eq!(default_threads(2), 1);
        assert_eq!(default_threads(8), 7);
        assert_eq!(default_threads(64), 8);
    }

    #[test]
    fn short_audio_is_rejected_before_decoding() {
        assert!(ensure_audio_is_decodable(&vec![0.0; 100]).is_err());
        assert!(ensure_audio_is_decodable(&vec![0.0; MIN_AUDIO_SAMPLES]).is_ok());
    }

    #[test]
    fn transcript_full_join_is_clean() {
        let t = Transcript {
            committed: "a".into(),
            interim: "b".into(),
        };
        assert_eq!(t.full(), "a b");
        assert_eq!(
            Transcript {
                committed: "".into(),
                interim: "b".into()
            }
            .full(),
            "b"
        );
        assert!(Transcript::default().is_empty());
    }
}
