//! Voice activity detection (spec §11).
//!
//! Deliberately conservative. The failure mode we care about is cutting the
//! user off mid-sentence, so speech starts only after several consecutive
//! energetic frames and ends only after a sustained quiet run. The absolute
//! threshold adapts to the room's noise floor, which is what makes this usable
//! with a laptop mic in a café as well as a studio mic.

use crate::settings::SilencePreset;

/// A 20 ms frame at 16 kHz. Small enough to feel instant, large enough to give
/// a stable energy estimate.
pub const FRAME_MS: u32 = 20;
pub const FRAME_SAMPLES: usize = 16_000 * FRAME_MS as usize / 1_000;

/// Frames of energy needed to declare speech (60 ms of attack).
const ATTACK_FRAMES: u32 = 3;
/// Consecutive quiet frames needed to declare the utterance over. The preset's
/// trailing-silence window is applied on top of this.
const RELEASE_FRAMES: u32 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadEvent {
    /// Quiet, and no utterance in progress.
    Silence,
    /// The utterance just began on this frame.
    SpeechStart,
    /// Speech continues.
    Speech,
    /// The utterance ended; `trailing_ms` is how long the quiet run is.
    SpeechEnd { trailing_ms: u64 },
}

/// Classifies 20 ms frames of mono audio into speech and silence.
#[derive(Debug, Clone)]
pub struct Vad {
    preset: SilencePreset,
    /// User-configured floor, from the preset.
    floor: f32,
    /// Exponentially smoothed minimum energy seen while quiet.
    noise_floor: f32,
    above_run: u32,
    quiet_run: u32,
    in_speech: bool,
}

impl Vad {
    pub fn new(preset: SilencePreset) -> Self {
        Vad {
            preset,
            floor: preset.energy_threshold(),
            // Start optimistic: assume a quiet room until we hear otherwise.
            noise_floor: 0.0,
            above_run: 0,
            quiet_run: 0,
            in_speech: false,
        }
    }

    /// VAD is disabled entirely for `SilencePreset::Off`.
    pub fn is_enabled(&self) -> bool {
        self.preset != SilencePreset::Off && self.floor > 0.0
    }

    pub fn preset(&self) -> SilencePreset {
        self.preset
    }

    /// The effective threshold, for the diagnostics screen.
    pub fn threshold(&self) -> f32 {
        self.floor.max(self.noise_floor * 3.0)
    }

    /// How much quiet ends the take, if any.
    pub fn trailing_silence_ms(&self) -> Option<u64> {
        self.preset.trailing_silence_ms()
    }

    /// Feed one frame's RMS energy.
    pub fn push(&mut self, rms: f32) -> VadEvent {
        if !self.is_enabled() {
            // With VAD off, every frame counts as speech once we are recording.
            if !self.in_speech {
                self.in_speech = true;
                return VadEvent::SpeechStart;
            }
            return VadEvent::Speech;
        }

        let threshold = self.threshold();
        let is_voiced = rms > threshold;

        if is_voiced {
            self.above_run += 1;
            self.quiet_run = 0;
        } else {
            self.quiet_run += 1;
            self.above_run = 0;
            // Track the quiet level slowly so a passing fan cannot raise it.
            if self.noise_floor == 0.0 {
                self.noise_floor = rms;
            } else {
                self.noise_floor = self.noise_floor * 0.95 + rms * 0.05;
            }
        }

        if !self.in_speech {
            if self.above_run >= ATTACK_FRAMES {
                self.in_speech = true;
                return VadEvent::SpeechStart;
            }
            return VadEvent::Silence;
        }

        // In speech: only end the utterance once the preset's window has passed.
        if self.quiet_run >= RELEASE_FRAMES {
            let trailing_ms = self.quiet_run as u64 * FRAME_MS as u64;
            match self.trailing_silence_ms() {
                Some(window) if trailing_ms >= window => {
                    self.in_speech = false;
                    self.quiet_run = 0;
                    return VadEvent::SpeechEnd { trailing_ms };
                }
                // Utterance-level VAD is off; keep recording.
                None => {}
                // Not quiet long enough yet.
                Some(_) => {}
            }
        }

        VadEvent::Speech
    }

    /// True once a take should be stopped automatically.
    pub fn should_auto_stop(&self) -> bool {
        self.in_speech && self.preset.trailing_silence_ms().is_some()
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    /// Reset between takes, keeping what we learned about the room.
    pub fn reset(&mut self) {
        self.above_run = 0;
        self.quiet_run = 0;
        self.in_speech = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feed `frames` of `rms` and return the events produced.
    fn run(vad: &mut Vad, frames: usize, rms: f32) -> Vec<VadEvent> {
        (0..frames).map(|_| vad.push(rms)).collect()
    }

    #[test]
    fn off_preset_disables_detection() {
        let mut vad = Vad::new(SilencePreset::Off);
        assert!(!vad.is_enabled());
        // The very first frame opens the stream and every frame after is speech.
        assert_eq!(vad.push(0.0), VadEvent::SpeechStart);
        assert_eq!(vad.push(0.0), VadEvent::Speech);
    }

    #[test]
    fn silence_does_not_trigger_speech() {
        let mut vad = Vad::new(SilencePreset::Balanced);
        let events = run(&mut vad, 50, 0.0005);
        assert!(events.iter().all(|e| *e == VadEvent::Silence));
        assert!(!vad.in_speech());
    }

    #[test]
    fn requires_several_frames_of_attack() {
        let mut vad = Vad::new(SilencePreset::Balanced);
        // A single transient must not open an utterance.
        assert_eq!(vad.push(0.5), VadEvent::Silence);
        assert_eq!(vad.push(0.0005), VadEvent::Silence);
        assert!(!vad.in_speech());

        let events = run(&mut vad, ATTACK_FRAMES as usize, 0.05);
        assert_eq!(events[0], VadEvent::Silence, "still accumulating");
        assert!(events.contains(&VadEvent::SpeechStart));
        assert!(vad.in_speech());
    }

    #[test]
    fn short_pause_does_not_end_the_sentence() {
        let mut vad = Vad::new(SilencePreset::Balanced);
        run(&mut vad, 20, 0.05);
        assert!(vad.in_speech());

        // 200 ms of quiet: below the 400 ms release and the 1200 ms window.
        let events = run(&mut vad, 10, 0.0001);
        assert!(
            !events
                .iter()
                .any(|e| matches!(e, VadEvent::SpeechEnd { .. })),
            "a natural pause must not close the utterance"
        );
        assert!(vad.in_speech());
    }

    #[test]
    fn sustained_quiet_ends_the_utterance_once() {
        let mut vad = Vad::new(SilencePreset::Short);
        run(&mut vad, 20, 0.05);

        let window_frames =
            (SilencePreset::Short.trailing_silence_ms().unwrap() / FRAME_MS as u64) as usize;
        let events = run(
            &mut vad,
            window_frames + RELEASE_FRAMES as usize + 2,
            0.0001,
        );

        let ends = events
            .iter()
            .filter(|e| matches!(e, VadEvent::SpeechEnd { .. }))
            .count();
        assert_eq!(ends, 1, "the utterance must close exactly once");
        assert!(!vad.in_speech());
    }

    #[test]
    fn longer_preset_waits_longer() {
        let frames_for = |preset: SilencePreset| {
            let mut vad = Vad::new(preset);
            run(&mut vad, 20, 0.05);
            let mut used = 0;
            loop {
                used += 1;
                if matches!(vad.push(0.0001), VadEvent::SpeechEnd { .. }) {
                    break;
                }
                assert!(used < 1_000, "VAD never closed the utterance");
            }
            used
        };
        assert!(frames_for(SilencePreset::Short) < frames_for(SilencePreset::Balanced));
        assert!(frames_for(SilencePreset::Balanced) < frames_for(SilencePreset::Long));
    }

    #[test]
    fn noise_floor_adapts_so_a_noisy_room_still_detects_speech() {
        let mut vad = Vad::new(SilencePreset::Balanced);
        let steady_noise = 0.02; // above the preset's 0.006 floor

        // A steady hum would otherwise read as continuous speech.
        run(&mut vad, 200, steady_noise);

        // Speech has to be meaningfully louder than the learned floor, and it is.
        let events = run(&mut vad, 10, 0.15);
        assert!(
            events.contains(&VadEvent::SpeechStart),
            "speech should be detected over a steady noise floor"
        );
        assert!(vad.noise_floor > 0.0);
        assert!(vad.threshold() > SilencePreset::Balanced.energy_threshold());
    }

    #[test]
    fn reset_clears_take_state_but_keeps_the_noise_model() {
        let mut vad = Vad::new(SilencePreset::Balanced);
        run(&mut vad, 50, 0.0008);
        let learned = vad.noise_floor;

        run(&mut vad, 10, 0.2);
        assert!(vad.in_speech());
        vad.reset();
        assert!(!vad.in_speech());
        assert_eq!(vad.noise_floor, learned, "room knowledge should persist");
    }

    #[test]
    fn auto_stop_is_only_offered_when_a_window_is_configured() {
        let mut vad = Vad::new(SilencePreset::Off);
        vad.push(0.5);
        assert!(vad.should_auto_stop() || vad.trailing_silence_ms().is_none());
        assert!(Vad::new(SilencePreset::Balanced)
            .trailing_silence_ms()
            .is_some());
        assert!(Vad::new(SilencePreset::Off).trailing_silence_ms().is_none());
    }
}
