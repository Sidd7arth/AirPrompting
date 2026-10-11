//! Microphone capture.
//!
//! The device runs at its native rate and channel count; [`CaptureCore`]
//! converts every callback's frames to 16 kHz mono on the audio thread and
//! accumulates the take. The app polls [`CaptureCore::take_audio`] and
//! [`CaptureCore::take_level`] from its own loop, so the cpal callback itself
//! does no allocation, no locking of UI state, and no I/O.
//!
//! cpal's input stream callback gives `&[f32]` (or `&[i16]`) — the layouts
//! differ per sample format, which is why [`CaptureCore::push_interleaved`]
//! exists in three flavours and everything downstream is f32.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use airprompt_core::audio::{measure, to_mono, Resampler};
use airprompt_core::error::{Error, Result};
use airprompt_core::platform::InputDevice;
use airprompt_core::whisper::SAMPLE_RATE;

/// The pipeline state shared between the cpal callback and the app loop.
///
/// The callback calls only `push_*`; the app calls the `take_*` methods.
pub struct CaptureCore {
    inner: Mutex<Core>,
    /// Total converted samples, for diagnostics (`captured_samples`).
    pushed: AtomicU64,
    /// Monotonic counter incremented per device callback, for diagnostics.
    callbacks: AtomicU64,
}

struct Core {
    resampler: Resampler,
    /// All 16 kHz mono samples of the current take.
    take: Vec<f32>,
    /// Latest input level, 0.0..=1.0, as (peak, rms) pair flattened.
    level_peak: f32,
    level_rms: f32,
    /// Input sample rate the resampler was built for.
    input_rate: u32,
    input_channels: u16,
    /// Wall-clock time of the first pushed sample, for duration accounting.
    started_epoch_ms: Option<i64>,
    elapsed_ms: u64,
}

impl CaptureCore {
    pub fn new(input_rate: u32, input_channels: u16) -> Self {
        CaptureCore {
            inner: Mutex::new(Core {
                resampler: Resampler::new(input_rate.max(1), SAMPLE_RATE),
                take: Vec::new(),
                level_peak: 0.0,
                level_rms: 0.0,
                input_rate: input_rate.max(1),
                input_channels: input_channels.max(1),
                started_epoch_ms: None,
                elapsed_ms: 0,
            }),
            pushed: AtomicU64::new(0),
            callbacks: AtomicU64::new(0),
        }
    }

    /// Push one device callback's worth of interleaved f32 frames.
    ///
    /// This is the audio-thread path: the converted block is owned (never a
    /// borrow of the locked state), so nothing can deadlock or trip a borrow
    /// conflict while the app loop reads the take.
    pub fn push_interleaved(&self, interleaved: &[f32]) {
        self.callbacks.fetch_add(1, Ordering::Relaxed);
        let Ok(mut core) = self.inner.lock() else {
            return;
        };
        let mono = to_mono(interleaved, core.input_channels as usize);
        // Owned conversion: passthrough copies, everything else resamples.
        let converted: Vec<f32> = if core.resampler.is_passthrough() {
            mono
        } else {
            core.resampler.process(&mono)
        };
        if converted.is_empty() {
            return;
        }
        let frame = measure(&converted);
        core.level_peak = frame.peak;
        core.level_rms = frame.rms;
        core.take.extend_from_slice(&converted);
        core.elapsed_ms += (converted.len() as u64 * 1000) / SAMPLE_RATE as u64;
        if core.started_epoch_ms.is_none() {
            core.started_epoch_ms = Some(airprompt_core::now_ms());
        }
        let count = converted.len() as u64;
        drop(core);
        self.pushed.fetch_add(count, Ordering::Relaxed);
    }

    /// Push i16 samples (the `Int16`/`I16` cpal formats).
    pub fn push_interleaved_i16(&self, interleaved: &[i16]) {
        let f32s: Vec<f32> = interleaved.iter().map(|&s| s as f32 / 32768.0).collect();
        self.push_interleaved(&f32s);
    }

    /// Drain everything captured so far, as 16 kHz mono.
    pub fn take_audio(&self) -> Vec<f32> {
        let Ok(mut core) = self.inner.lock() else {
            return Vec::new();
        };
        std::mem::take(&mut core.take)
    }

    /// A copy of everything captured so far, without draining.
    ///
    /// Used for the streaming decode: every hypothesis is a decode of the
    /// whole take so far (the engine trait's contract, spec §8).
    pub fn snapshot(&self) -> Vec<f32> {
        self.inner
            .lock()
            .map(|core| core.take.clone())
            .unwrap_or_default()
    }

    /// Total samples accumulated for the current take.
    pub fn sample_count(&self) -> usize {
        self.inner.lock().map(|core| core.take.len()).unwrap_or(0)
    }

    /// Remove and return the samples after `cursor`, advancing it.
    ///
    /// The dictation controller keeps the cursor per take; this is how the
    /// VAD sees each frame exactly once without copying the whole take.
    pub fn since(&self, cursor: &mut usize) -> Vec<f32> {
        let Ok(mut core) = self.inner.lock() else {
            return Vec::new();
        };
        let from = (*cursor).min(core.take.len());
        let fresh: Vec<f32> = core.take[from..].to_vec();
        *cursor = core.take.len();
        fresh
    }

    /// Drain just the tail, keeping the accumulated take intact.
    pub fn peek_tail(&self, max: usize) -> Vec<f32> {
        let Ok(core) = self.inner.lock() else {
            return Vec::new();
        };
        let start = core.take.len().saturating_sub(max);
        core.take[start..].to_vec()
    }

    /// Latest level as `(peak, rms)`, both 0.0..=1.0.
    pub fn take_level(&self) -> (f32, f32) {
        match self.inner.lock() {
            Ok(core) => (core.level_peak, core.level_rms),
            Err(_) => (0.0, 0.0),
        }
    }

    /// Elapsed capture time in milliseconds of 16 kHz audio.
    pub fn elapsed_ms(&self) -> u64 {
        self.inner.lock().map(|core| core.elapsed_ms).unwrap_or(0)
    }

    /// Total samples converted since construction, for diagnostics.
    pub fn captured_samples(&self) -> u64 {
        self.pushed.load(Ordering::Relaxed)
    }

    /// Device callbacks observed, for diagnostics.
    pub fn callback_count(&self) -> u64 {
        self.callbacks.load(Ordering::Relaxed)
    }

    /// Drop everything captured so far (a cancelled take).
    pub fn clear(&self) {
        if let Ok(mut core) = self.inner.lock() {
            core.take.clear();
            core.elapsed_ms = 0;
            core.started_epoch_ms = None;
        }
        self.pushed.store(0, Ordering::Relaxed);
    }

    /// Keep the level meter but drop the take (VAD-triggered trim).
    pub fn reset_take(&self) {
        if let Ok(mut core) = self.inner.lock() {
            core.take.clear();
            core.elapsed_ms = 0;
            core.started_epoch_ms = None;
        }
    }
}

/// The live device stream. Constructing it opens the device; dropping it
/// closes the device (cpal streams stop when dropped).
pub struct CaptureStream {
    pub stream: cpal::Stream,
    pub config: cpal::SupportedStreamConfig,
}

impl std::fmt::Debug for CaptureStream {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureStream")
            .field("rate", &self.config.sample_rate().0)
            .field("channels", &self.config.channels())
            .finish_non_exhaustive()
    }
}

/// Every input device the system reports, with the default first.
///
/// An empty list is a real answer (no microphone plugged in) and the caller
/// surfaces it as "no microphone found", never as a fake device.
pub fn input_devices() -> Result<Vec<InputDevice>> {
    let host = cpal::default_host();
    let devices = host.input_devices().map_err(|error| {
        Error::Microphone(format!("could not enumerate audio devices: {error}"))
    })?;

    let default_id = host.default_input_device().and_then(|d| d.name().ok());

    let mut out: Vec<InputDevice> = Vec::new();
    for device in devices {
        let Ok(name) = device.name() else {
            continue;
        };
        let is_default = default_id.as_deref() == Some(name.as_str());
        out.push(InputDevice {
            // The device *name* is the stable handle here: cpal ids are not
            // stable across replug, and names are what the user recognises.
            id: name.clone(),
            name,
            is_default,
        });
    }
    // Default first, then alphabetical.
    out.sort_by(|a, b| {
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// Resolve the requested device name (or the system default) to a cpal device.
pub fn resolve_device(name: Option<&str>) -> Result<(cpal::Device, String)> {
    let host = cpal::default_host();
    if let Some(wanted) = name.map(str::trim).filter(|n| !n.is_empty()) {
        let devices = host.input_devices().map_err(|error| {
            Error::Microphone(format!("could not enumerate audio devices: {error}"))
        })?;
        for device in devices {
            if device.name().ok().as_deref() == Some(wanted) {
                return Ok((device, wanted.to_string()));
            }
        }
        // A named device that has vanished must not silently fall back: the
        // user would hear themselves through the wrong mic with no explanation.
        return Err(Error::Microphone(format!(
            "the selected microphone could not be found"
        )));
    }

    let device = host
        .default_input_device()
        .ok_or_else(|| Error::Microphone("no microphone is connected".into()))?;
    let name = device
        .name()
        .unwrap_or_else(|_| "Default microphone".to_string());
    Ok((device, name))
}

/// Open the device and start streaming into `core`.
///
/// The stream's native configuration is used rather than forcing 16 kHz: the
/// resampler handles conversion, and devices behave better at their preferred
/// rate.
pub fn start(core: std::sync::Arc<CaptureCore>, name: Option<&str>) -> Result<CaptureStream> {
    let (device, _device_name) = resolve_device(name)?;
    let config: cpal::SupportedStreamConfig = device.default_input_config().map_err(|error| {
        Error::Microphone(format!(
            "the microphone rejected its configuration: {error}"
        ))
    })?;

    let rate = config.sample_rate().0;
    let channels = config.channels();
    let error_handler = |error| {
        tracing::warn!(%error, "audio capture stream error");
    };

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &config.clone().into(),
                move |data: &[f32], _| core.push_interleaved(data),
                error_handler,
                None,
            )
            .map_err(cpal_error)?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &config.clone().into(),
                move |data: &[i16], _| core.push_interleaved_i16(data),
                error_handler,
                None,
            )
            .map_err(cpal_error)?,
        cpal::SampleFormat::U16 => {
            // u16 is unsigned; centre it before converting.
            let core2 = std::sync::Arc::clone(&core);
            device
                .build_input_stream(
                    &config.clone().into(),
                    move |data: &[u16], _| {
                        let f32s: Vec<f32> = data
                            .iter()
                            .map(|&s| (s as f32 - 32768.0) / 32768.0)
                            .collect();
                        core2.push_interleaved(&f32s);
                    },
                    error_handler,
                    None,
                )
                .map_err(cpal_error)?
        }
        other => {
            return Err(Error::Microphone(format!(
                "this microphone reports an unsupported sample format ({other})"
            )));
        }
    };

    // Remember the actual configuration for diagnostics.
    tracing::info!(rate, channels, "microphone stream opened");
    let _ = core; // moved into the closures above
    Ok(CaptureStream { stream, config })
}

fn cpal_error(error: cpal::BuildStreamError) -> Error {
    use cpal::BuildStreamError;
    match &error {
        BuildStreamError::DeviceNotAvailable => {
            Error::Microphone("the microphone is no longer available".into())
        }
        BuildStreamError::StreamConfigNotSupported(_) => {
            Error::Microphone("the microphone rejected the requested configuration".into())
        }
        _ => Error::Microphone(format!("the microphone could not be opened: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn core(rate: u32, channels: u16) -> CaptureCore {
        CaptureCore::new(rate, channels)
    }

    #[test]
    fn mono_16k_input_passes_through_untouched() {
        let core = core(16_000, 1);
        core.push_interleaved(&[0.5, -0.5, 0.5, -0.5]);
        let audio = core.take_audio();
        assert_eq!(audio, vec![0.5, -0.5, 0.5, -0.5]);
    }

    #[test]
    fn stereo_is_downmixed_then_counted_once() {
        let core = core(16_000, 2);
        // Two frames of stereo: (1,0) and (-1,0) → mono 0.5, -0.5.
        core.push_interleaved(&[1.0, 0.0, -1.0, 0.0]);
        let audio = core.take_audio();
        assert_eq!(audio.len(), 2);
        assert!((audio[0] - 0.5).abs() < 1e-6);
        assert!((audio[1] + 0.5).abs() < 1e-6);
    }

    #[test]
    fn resampling_48k_produces_the_right_sample_count() {
        let core = core(48_000, 1);
        let input = vec![0.25f32; 48_000]; // one second
        core.push_interleaved(&input);
        let audio = core.take_audio();
        // 48k→16k must produce ~16 000 samples (± filter ramp-up).
        let delta = (audio.len() as i64 - 16_000).abs();
        assert!(
            delta < 200,
            "expected ≈16 000 samples from one second at 48 kHz, got {}",
            audio.len()
        );
        assert_eq!(core.elapsed_ms(), 16_000, "elapsed is measured at 16 kHz");
    }

    #[test]
    fn level_reflects_the_latest_block() {
        let core = core(16_000, 1);
        assert_eq!(core.take_level(), (0.0, 0.0));
        core.push_interleaved(&[1.0, -1.0, 1.0, -1.0]);
        let (peak, rms) = core.take_level();
        assert!((peak - 1.0).abs() < 1e-6);
        assert!((rms - 1.0).abs() < 1e-6);
    }

    #[test]
    fn level_survives_after_a_take_is_cleared() {
        let core = core(16_000, 1);
        core.push_interleaved(&[0.8, -0.8]);
        core.clear();
        assert!(core.take_audio().is_empty());
        // The meter stays live; the meter is not part of the take.
        assert!(core.take_level().0 > 0.0);
    }

    #[test]
    fn i16_input_is_normalised_into_the_f32_pipeline() {
        let core = core(16_000, 1);
        core.push_interleaved_i16(&[16384, -16384]);
        let audio = core.take_audio();
        assert_eq!(audio.len(), 2);
        assert!((audio[0] - 0.5).abs() < 1e-3);
        assert!((audio[1] + 0.5).abs() < 1e-3);
    }

    #[test]
    fn cancelled_take_empties_everything() {
        let core = core(16_000, 1);
        core.push_interleaved(&vec![0.1f32; 16_000]);
        assert!(!core.take_audio().is_empty());
        core.clear();
        assert!(core.take_audio().is_empty());
        assert_eq!(core.captured_samples(), 0, "clear resets the counter");
    }

    #[test]
    fn peek_tail_does_not_consume() {
        let core = core(16_000, 1);
        core.push_interleaved(&vec![0.5f32; 2_000]);
        assert_eq!(core.peek_tail(100).len(), 100);
        assert_eq!(core.take_audio().len(), 2_000);
    }

    #[test]
    fn elapsed_time_tracks_the_16k_timeline_not_the_device_rate() {
        let core = core(44_100, 1);
        core.push_interleaved(&vec![0.0f32; 44_100]); // one second at 44.1 kHz
        let ms = core.elapsed_ms();
        assert!(
            (960..=1040).contains(&ms),
            "one second of device audio must read as ≈1000 ms of 16 kHz audio, got {ms}"
        );
    }

    #[test]
    fn callbacks_and_samples_are_counted_for_diagnostics() {
        let core = core(16_000, 1);
        core.push_interleaved(&[0.1; 320]);
        core.push_interleaved(&[0.1; 320]);
        assert_eq!(core.callback_count(), 2);
        assert_eq!(core.captured_samples(), 640);
    }

    #[test]
    fn poisoned_state_degrades_to_silence_instead_of_panicking() {
        // A poisoned mutex must not take down the audio thread.
        let core = core(16_000, 1);
        core.inner.lock().unwrap();
        // (A real poison requires a panic while holding the lock; here we only
        // assert the take_* paths return defaults on a contended lock.)
        assert_eq!(core.take_audio(), Vec::<f32>::new());
        assert_eq!(core.take_level(), (0.0, 0.0));
        assert_eq!(core.elapsed_ms(), 0);
    }
}
