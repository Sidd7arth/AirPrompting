//! Audio primitives: mono downmix, level metering, resampling and a streaming
//! ring buffer.
//!
//! The speech model wants 16 kHz mono `f32`. Native capture devices hand us
//! 44.1/48 kHz stereo, so the conversion has to be real: a plain decimation
//! aliases badly enough to hurt recognition accuracy, so the resampler is a
//! windowed-sinc low-pass followed by fractional-delay interpolation.

use crate::platform::AudioFrame;

/// Collapse interleaved channels to mono by averaging.
///
/// `channels` must be >= 1; a zero count is treated as already-mono so a
/// misconfigured device cannot panic the capture thread.
pub fn to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    let frames = interleaved.len() / channels;
    let mut out = Vec::with_capacity(frames);
    let scale = 1.0 / channels as f32;
    for frame in 0..frames {
        let base = frame * channels;
        let mut sum = 0.0;
        for channel in 0..channels {
            sum += interleaved[base + channel];
        }
        out.push(sum * scale);
    }
    out
}

/// Peak and RMS for a block of mono samples.
pub fn measure(samples: &[f32]) -> AudioFrame {
    if samples.is_empty() {
        return AudioFrame {
            peak: 0.0,
            rms: 0.0,
        };
    }
    let mut peak = 0.0f32;
    let mut sum_sq = 0.0f64;
    for &sample in samples {
        let magnitude = sample.abs();
        if magnitude > peak {
            peak = magnitude;
        }
        sum_sq += (sample as f64) * (sample as f64);
    }
    AudioFrame {
        peak: peak.min(1.0),
        rms: ((sum_sq / samples.len() as f64).sqrt() as f32).min(1.0),
    }
}

/// Converts a float sample block to 16-bit little-endian PCM.
///
/// This is the interchange format for the whisper.cpp sidecar, which expects
/// 16-bit mono input.
pub fn to_pcm_i16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|&s| (s.clamp(-1.0, 1.0) * i16::MAX as f32).round() as i16)
        .collect()
}

pub fn pcm_i16_to_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    out
}

/// A streaming sample-rate converter.
///
/// Coefficients are a Hamming-windowed sinc low-pass at the *destination*
/// Nyquist frequency, which is what prevents aliasing when downsampling.
pub struct Resampler {
    from_hz: u32,
    to_hz: u32,
    /// Low-pass taps.
    taps: Vec<f32>,
    /// Number of input samples carried over between calls.
    history: Vec<f32>,
    /// Fractional read position, in input samples, relative to `history` start.
    position: f64,
}

/// Half-width of the anti-aliasing filter, in input samples, on each side of
/// the interpolation point. 16 taps is ample for speech at these rates.
const TAPS_PER_SIDE: isize = 16;

impl Resampler {
    pub fn new(from_hz: u32, to_hz: u32) -> Self {
        let from_hz = from_hz.max(1);
        let to_hz = to_hz.max(1);
        let cutoff = (to_hz as f32 / 2.0).min(from_hz as f32 / 2.0) / from_hz as f32;

        let tap_count = (TAPS_PER_SIDE * 2 + 1) as usize;
        let mut taps = Vec::with_capacity(tap_count);
        let mut sum = 0.0f32;
        for i in 0..tap_count {
            let n = i as f32 - TAPS_PER_SIDE as f32;
            // sinc low-pass, normalised so the DC gain is 1 once scaled by 2*cutoff.
            let sinc = if n.abs() < f32::EPSILON {
                2.0 * cutoff
            } else {
                (2.0 * std::f32::consts::PI * cutoff * n).sin() / (std::f32::consts::PI * n)
            };
            // Hamming window across the tap span.
            let window = 0.54
                - 0.46 * (2.0 * std::f32::consts::PI * i as f32 / (tap_count as f32 - 1.0)).cos();
            let tap = sinc * window;
            taps.push(tap);
            sum += tap;
        }
        if sum.abs() > f32::EPSILON {
            for tap in &mut taps {
                *tap /= sum;
            }
        }

        Resampler {
            from_hz,
            to_hz,
            taps,
            history: Vec::with_capacity(TAPS_PER_SIDE as usize * 4),
            position: 0.0,
        }
    }

    /// True when input may be passed through untouched.
    pub fn is_passthrough(&self) -> bool {
        self.from_hz == self.to_hz
    }

    pub fn ratio(&self) -> f64 {
        self.from_hz as f64 / self.to_hz as f64
    }

    /// Resample one block, returning the produced output.
    ///
    /// State is retained between calls, so blocks may be any size and the
    /// output is continuous.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if self.is_passthrough() {
            return input.to_vec();
        }

        // `history` keeps the previous samples so interpolation near the start of
        // this block can still look backwards.
        let mut buffer = std::mem::take(&mut self.history);
        buffer.extend_from_slice(input);

        let mut out = Vec::with_capacity(((input.len() as f64) / self.ratio()).ceil() as usize + 2);

        // `history` is contiguous in time with `input`, so a read position is
        // simply an index into `buffer`. A full FIR evaluation needs
        // TAPS_PER_SIDE samples on either side of the integer index it is
        // anchored at, and the fractional case evaluates two adjacent anchors.
        let first = TAPS_PER_SIDE as f64;
        if self.position < first {
            self.position = first;
        }
        // Highest anchor that still has its whole tap span inside the buffer,
        // leaving room for the `anchor + 1` evaluation too.
        let last = buffer.len() as f64 - (TAPS_PER_SIDE as f64 + 2.0);

        while self.position <= last {
            let anchor = self.position.floor() as isize;
            let frac = (self.position - anchor as f64) as f32;

            // Two integer-aligned filter outputs, linearly blended. This is a
            // correct (if simple) fractional-delay interpolator, unlike
            // splitting the taps themselves.
            let mut at_anchor = 0.0f32;
            let mut at_next = 0.0f32;
            for (offset, &tap) in self.taps.iter().enumerate() {
                let base = anchor - TAPS_PER_SIDE + offset as isize;
                if base < 0 {
                    continue;
                }
                let index = base as usize;
                if index < buffer.len() {
                    at_anchor += buffer[index] * tap;
                }
                if index + 1 < buffer.len() {
                    at_next += buffer[index + 1] * tap;
                }
            }
            out.push(at_anchor * (1.0 - frac) + at_next * frac);
            self.position += self.ratio();
        }

        // Retain the tail the next block needs. The read position may sit as
        // low as `last`, i.e. `len - TAPS_PER_SIDE - 2`, so keeping
        // `2 * TAPS_PER_SIDE` samples leaves the re-based position comfortably
        // above `first` and no samples are skipped between calls.
        let keep = (TAPS_PER_SIDE as usize) * 2;
        if buffer.len() > keep {
            let drain = buffer.len() - keep;
            buffer.drain(0..drain);
            self.position -= drain as f64;
        }
        self.history = buffer;
        out
    }
}

/// A bounded FIFO of mono samples used to hand captured audio to the engine.
pub struct AudioRing {
    samples: std::collections::VecDeque<f32>,
    capacity: usize,
}

impl AudioRing {
    pub fn new(capacity: usize) -> Self {
        AudioRing {
            samples: std::collections::VecDeque::with_capacity(capacity.min(1 << 16)),
            capacity,
        }
    }

    /// Append samples, dropping the oldest when over capacity.
    ///
    /// Dropping the oldest is the right failure mode: a stalled consumer must
    /// not make the capture thread allocate without bound.
    pub fn push(&mut self, samples: &[f32]) {
        if self.capacity == 0 {
            return;
        }
        for &sample in samples {
            if self.samples.len() == self.capacity {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Remove and return up to `max` samples.
    pub fn drain(&mut self, max: usize) -> Vec<f32> {
        let take = max.min(self.samples.len());
        self.samples.drain(..take).collect()
    }

    pub fn take_all(&mut self) -> Vec<f32> {
        self.samples.drain(..).collect()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn sine(freq: f32, sample_rate: u32, seconds: f32, amplitude: f32) -> Vec<f32> {
        let count = (sample_rate as f32 * seconds) as usize;
        (0..count)
            .map(|i| amplitude * (2.0 * PI * freq * i as f32 / sample_rate as f32).sin())
            .collect()
    }

    #[test]
    fn downmix_averages_channels() {
        let stereo = vec![1.0, 0.0, 0.5, 0.5, -1.0, -1.0];
        assert_eq!(to_mono(&stereo, 2), vec![0.5, 0.5, -1.0]);
        // A single channel must be untouched.
        assert_eq!(to_mono(&stereo, 1), stereo);
        // A nonsense channel count must not panic or slice out of bounds.
        assert_eq!(to_mono(&stereo, 0), stereo);
        // A partial final frame is discarded rather than read past the end.
        let partial = to_mono(&[0.2, 0.4, 0.6], 2);
        assert_eq!(partial.len(), 1);
        assert!((partial[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn measure_reports_peak_and_rms() {
        let frame = measure(&[1.0, -1.0, 1.0, -1.0]);
        assert!((frame.peak - 1.0).abs() < 1e-6);
        assert!((frame.rms - 1.0).abs() < 1e-6);

        let quiet = measure(&[0.001, -0.001]);
        assert!(quiet.peak < 0.01);
        assert!(quiet.rms < 0.01);

        assert_eq!(measure(&[]).peak, 0.0);
    }

    #[test]
    fn measure_clamps_out_of_range_input() {
        // A device that overdrives must not make the meter exceed 1.0.
        let frame = measure(&[4.0, -4.0]);
        assert_eq!(frame.peak, 1.0);
        assert_eq!(frame.rms, 1.0);
    }

    #[test]
    fn pcm_conversion_round_trips_range() {
        let pcm = to_pcm_i16(&[0.0, 1.0, -1.0, 2.0, -2.0]);
        assert_eq!(pcm[0], 0);
        assert_eq!(pcm[1], i16::MAX);
        assert_eq!(pcm[2], -i16::MAX);
        // Out-of-range input is clamped, not wrapped.
        assert_eq!(pcm[3], i16::MAX);
        assert_eq!(pcm[4], -i16::MAX);

        let bytes = pcm_i16_to_bytes(&[1, -1]);
        assert_eq!(bytes.len(), 4);
        assert_eq!(&bytes[0..2], &[0x01, 0x00]);
    }

    #[test]
    fn passthrough_when_rates_match() {
        let mut r = Resampler::new(16_000, 16_000);
        assert!(r.is_passthrough());
        let input = sine(440.0, 16_000, 0.01, 1.0);
        assert_eq!(r.process(&input), input);
    }

    #[test]
    fn resampling_preserves_frequency_and_amplitude() {
        // 48 kHz -> 16 kHz: the classic capture-device case.
        let mut r = Resampler::new(48_000, 16_000);
        let input = sine(440.0, 48_000, 0.25, 0.5);

        // Feed in 20 ms blocks, as the capture callback would.
        let mut output = Vec::new();
        for block in input.chunks(960) {
            output.extend(r.process(block));
        }

        // Discard filter warm-up, then measure the dominant period by counting
        // zero crossings.
        let settled = &output[800..output.len() - 800];
        let crossings = settled
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        let seconds = settled.len() as f32 / 16_000.0;
        let measured_hz = crossings as f32 / 2.0 / seconds;
        assert!(
            (measured_hz - 440.0).abs() < 15.0,
            "expected ~440 Hz, measured {measured_hz} Hz"
        );

        // Amplitude must survive the anti-aliasing filter.
        let frame = measure(settled);
        assert!(
            (frame.peak - 0.5).abs() < 0.08,
            "expected ~0.5 peak, got {}",
            frame.peak
        );
    }

    #[test]
    fn resampling_blocks_is_continuous() {
        // The same signal processed in one call or many must agree; a
        // discontinuity would show up as a large sample-to-sample jump.
        let input = sine(300.0, 44_100, 0.2, 0.4);
        let mut many = Resampler::new(44_100, 16_000);
        let mut output = Vec::new();
        for block in input.chunks(441) {
            output.extend(many.process(block));
        }
        let settled = &output[500..];
        let max_jump = settled
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0f32, f32::max);
        assert!(
            max_jump < 0.2,
            "resampler produced a discontinuity of {max_jump}"
        );
    }

    #[test]
    fn downsample_output_length_tracks_ratio() {
        let mut r = Resampler::new(48_000, 16_000);
        let input = sine(200.0, 48_000, 0.5, 0.3);
        let mut total = 0;
        for block in input.chunks(480) {
            total += r.process(block).len();
        }
        let expected = input.len() / 3;
        let drift = (total as i64 - expected as i64).unsigned_abs();
        assert!(
            drift < expected as u64 / 20,
            "produced {total} samples, expected about {expected}"
        );
    }

    #[test]
    fn ring_buffer_keeps_the_newest_samples() {
        let mut ring = AudioRing::new(4);
        ring.push(&[1.0, 2.0, 3.0]);
        assert_eq!(ring.len(), 3);
        ring.push(&[4.0, 5.0, 6.0]);
        // Oldest three are dropped.
        assert_eq!(ring.len(), 4);
        assert_eq!(ring.drain(2), vec![3.0, 4.0]);
        assert_eq!(ring.take_all(), vec![5.0, 6.0]);
        assert!(ring.is_empty());

        let mut empty = AudioRing::new(0);
        empty.push(&[1.0]);
        assert!(empty.is_empty());
    }
}
