//! The cpal stream lifecycle (design D6).
//!
//! Three parts, of which only the last touches cpal:
//!
//! - [`choose`] picks a stream configuration from the device's capability
//!   list: the configured rate with the fewest channels (mono preferred),
//!   the configured buffer size when the device accepts it, and the
//!   device's defaults otherwise, with one notice per fallback
//!   (`desktop-app` spec, audio device).
//! - [`Feeder`] runs in the device callback: it owns the engine's
//!   [`AudioSource`], fills a mono buffer, converts it to the device's
//!   sample format and writes the same value to every channel. It never
//!   allocates, locks or blocks.
//! - [`Audio`] is the lifecycle: `Open` with the stream and the engine's
//!   [`SoundHandle`], or `Closed` with the reason, when there is no device,
//!   it could not be opened or it failed while running (`desktop-app` spec,
//!   running without an audio device). While closed, sound goes to a
//!   [`NullDriver`], and switching sound on tries to open the device again.
//!   The device itself is behind the [`Backend`] trait, so the transitions
//!   are tested without a sound card; [`CpalBackend`] is the real one.
//!
//! The stream is created, kept and dropped on the main thread, because
//! cpal streams are not `Send` on every host; only the [`Feeder`] moves to
//! the audio thread.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use digger_sound::{
    AudioSource, NullDriver, SampleRate, SoundConfig, SoundHandle, SoundSink, TuneTables,
    sound_link,
};

/// A sample format the callback can write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) enum SampleKind {
    /// Signed 16-bit, the engine's own format.
    I16,
    /// 32-bit float, −1 to +1.
    F32,
    /// Unsigned 16-bit, 32768 at rest.
    U16,
}

/// One range of stream configurations the device supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Capability {
    /// Channels per frame.
    pub(super) channels: u16,
    /// The lowest supported rate, Hz.
    pub(super) min_rate: u32,
    /// The highest supported rate, Hz.
    pub(super) max_rate: u32,
    /// The sample format, if the callback can write it.
    pub(super) format: Option<SampleKind>,
    /// The supported buffer sizes in frames, when the device says.
    pub(super) buffer: Option<(u32, u32)>,
}

/// The device's default stream configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DefaultConfig {
    /// Channels per frame.
    pub(super) channels: u16,
    /// The rate, Hz.
    pub(super) rate: u32,
    /// The sample format, if the callback can write it.
    pub(super) format: Option<SampleKind>,
    /// The supported buffer sizes in frames, when the device says.
    pub(super) buffer: Option<(u32, u32)>,
}

/// What the settings ask of the device (`sound.sample_rate`,
/// `sound.buffer_size`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AudioRequest {
    /// The sample rate, Hz.
    pub(crate) rate: u32,
    /// The buffer size, frames.
    pub(crate) buffer: u32,
}

/// The stream configuration chosen for a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Choice {
    /// Channels per frame.
    pub(super) channels: u16,
    /// The rate, Hz.
    pub(super) rate: u32,
    /// The sample format.
    pub(super) format: SampleKind,
    /// The buffer size in frames, or `None` for the device's default.
    pub(super) buffer: Option<u32>,
    /// One line per fallback taken, for `digger: notice:`.
    pub(super) notices: Vec<String>,
}

/// Whether `size` lies in the device's buffer range (an unknown range
/// accepts every size; the stream is then built and may still refuse).
fn buffer_fits(range: Option<(u32, u32)>, size: u32) -> bool {
    range.is_none_or(|(min, max)| (min..=max).contains(&size))
}

/// Picks a stream configuration (design D6): among the capabilities with a
/// writable format that contain the requested rate, the one with the
/// fewest channels, then the preferred format (`i16`, `f32`, `u16`); if
/// none contains the rate, the device's default configuration; if that has
/// no writable format either, the writable capability with the fewest
/// channels at the supported rate nearest the request. The requested
/// buffer size is kept when the chosen range accepts it, else the device's
/// default is used. Returns `None` when no capability has a writable
/// format.
pub(super) fn choose(
    capabilities: &[Capability],
    default: Option<DefaultConfig>,
    request: AudioRequest,
) -> Option<Choice> {
    let mut notices = Vec::new();
    let writable = || {
        capabilities
            .iter()
            .filter_map(|cap| cap.format.map(|format| (cap, format)))
    };
    let exact = writable()
        .filter(|(cap, _)| (cap.min_rate..=cap.max_rate).contains(&request.rate))
        .min_by_key(|&(cap, format)| (cap.channels, format));
    let (channels, rate, format, range) = if let Some((cap, format)) = exact {
        (cap.channels, request.rate, format, cap.buffer)
    } else {
        let fallback = default
            .and_then(|default| {
                default
                    .format
                    .map(|format| (default.channels, default.rate, format, default.buffer))
            })
            .or_else(|| {
                writable()
                    .min_by_key(|&(cap, format)| {
                        let nearest = request.rate.clamp(cap.min_rate, cap.max_rate);
                        (cap.channels, nearest.abs_diff(request.rate), format)
                    })
                    .map(|(cap, format)| {
                        let nearest = request.rate.clamp(cap.min_rate, cap.max_rate);
                        (cap.channels, nearest, format, cap.buffer)
                    })
            })?;
        notices.push(format!(
            "the audio device does not support {} Hz; using {} Hz",
            request.rate, fallback.1
        ));
        fallback
    };
    let buffer = if buffer_fits(range, request.buffer) {
        Some(request.buffer)
    } else {
        notices.push(format!(
            "the audio device does not support a buffer of {} frames; using its default",
            request.buffer
        ));
        None
    };
    Some(Choice {
        channels,
        rate,
        format,
        buffer,
        notices,
    })
}

/// A sample type the device callback writes.
pub(super) trait OutputSample: Copy {
    /// The engine's `i16` sample in this format.
    fn from_i16(sample: i16) -> Self;
}

impl OutputSample for i16 {
    fn from_i16(sample: i16) -> Self {
        sample
    }
}

impl OutputSample for u16 {
    fn from_i16(sample: i16) -> Self {
        sample.cast_unsigned() ^ 0x8000
    }
}

impl OutputSample for f32 {
    fn from_i16(sample: i16) -> Self {
        f32::from(sample) / 32768.0
    }
}

/// Where the callback's mono samples come from: the engine's
/// [`AudioSource`], or a test signal.
pub(super) trait MonoSource: Send + 'static {
    /// Fills `out` with the next mono samples.
    fn fill(&mut self, out: &mut [i16]);
}

impl MonoSource for AudioSource {
    fn fill(&mut self, out: &mut [i16]) {
        Self::fill(self, out);
    }
}

/// How many mono frames the callback renders at a time.
const SCRATCH_FRAMES: usize = 2048;

/// The device callback's state: the source it owns exclusively and a mono
/// buffer allocated before the stream starts.
pub(super) struct Feeder<S> {
    source: S,
    scratch: Box<[i16]>,
    channels: usize,
}

impl<S> fmt::Debug for Feeder<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Feeder")
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

impl<S: MonoSource> Feeder<S> {
    /// A feeder for a device with `channels` channels per frame.
    pub(super) fn new(source: S, channels: u16) -> Self {
        Self {
            source,
            scratch: vec![0; SCRATCH_FRAMES].into_boxed_slice(),
            channels: usize::from(channels.max(1)),
        }
    }

    /// Fills the interleaved device buffer `out`: one mono sample per
    /// frame, the same value in every channel.
    pub(super) fn fill<T: OutputSample>(&mut self, out: &mut [T]) {
        let channels = self.channels;
        for chunk in out.chunks_mut(SCRATCH_FRAMES * channels) {
            let frames = chunk.len().div_ceil(channels);
            let Some(mono) = self.scratch.get_mut(..frames) else {
                return;
            };
            self.source.fill(mono);
            for (frame, &sample) in chunk.chunks_mut(channels).zip(mono.iter()) {
                frame.fill(T::from_i16(sample));
            }
        }
    }
}

/// An open device stream as the lifecycle sees it.
#[derive(Debug)]
pub(super) struct Opened<S> {
    /// The stream; dropping it stops the device.
    pub(super) stream: S,
    /// The engine's game-side half.
    pub(super) handle: SoundHandle,
    /// Set by the device when the stream fails.
    pub(super) failed: Arc<AtomicBool>,
    /// One line per fallback taken while opening.
    pub(super) notices: Vec<String>,
}

/// Builds the engine for the rate the device gave; its [`AudioSource`]
/// goes into the stream.
pub(super) type Link<'a> = dyn FnMut(u32) -> Result<(SoundHandle, AudioSource), String> + 'a;

/// An audio output device.
pub(super) trait Backend {
    /// The open stream.
    type Stream;

    /// Opens the default output for `request`, creating the engine with
    /// `link` at the rate obtained. The error is the reason, for the
    /// notice.
    fn open(
        &mut self,
        request: AudioRequest,
        link: &mut Link<'_>,
    ) -> Result<Opened<Self::Stream>, String>;
}

/// The device's state (design D6).
#[derive(Debug)]
enum State<S> {
    /// No device: sound goes to [`NullDriver`].
    Closed {
        /// Why, as the notice said.
        reason: String,
    },
    /// The device plays the engine's output.
    Open {
        stream: S,
        handle: SoundHandle,
        failed: Arc<AtomicBool>,
    },
}

/// The audio device and its engine.
#[derive(Debug)]
pub(super) struct Audio<B: Backend> {
    backend: B,
    request: AudioRequest,
    state: State<B::Stream>,
    null: NullDriver,
    notices: Vec<String>,
}

impl<B: Backend> Audio<B> {
    /// Opens the device for `request`, with an engine set up as `config`
    /// says (its rate is replaced by the device's); closed, with a notice,
    /// when that fails.
    pub(super) fn open(backend: B, request: AudioRequest, config: SoundConfig) -> Self {
        let mut audio = Self {
            backend,
            request,
            state: State::Closed {
                reason: String::from("not opened yet"),
            },
            null: NullDriver,
            notices: Vec::new(),
        };
        audio.try_open(config);
        audio
    }

    /// Tries to open the device; on failure records the notice.
    fn try_open(&mut self, config: SoundConfig) {
        let mut link = |rate: u32| -> Result<(SoundHandle, AudioSource), String> {
            let sample_rate = SampleRate::new(rate).map_err(|error| error.to_string())?;
            let tables = TuneTables::embedded().map_err(|error| error.to_string())?;
            sound_link(
                SoundConfig {
                    sample_rate,
                    ..config
                },
                &tables,
            )
            .map_err(|error| error.to_string())
        };
        match self.backend.open(self.request, &mut link) {
            Ok(opened) => {
                self.notices.extend(opened.notices);
                self.state = State::Open {
                    stream: opened.stream,
                    handle: opened.handle,
                    failed: opened.failed,
                };
            }
            Err(reason) => {
                self.notices.push(format!(
                    "no audio device ({reason}); the game continues without sound"
                ));
                self.state = State::Closed { reason };
            }
        }
    }

    /// Once per frame: a stream the device reported as failed is closed,
    /// with a notice, and the game carries on in silence.
    pub(super) fn check(&mut self) {
        if let State::Open { failed, .. } = &self.state
            && failed.load(Ordering::Acquire)
        {
            let reason = String::from("the audio device failed");
            self.notices
                .push(format!("{reason}; the game continues without sound"));
            self.state = State::Closed { reason };
        }
    }

    /// Sound was switched on: if the device is closed, tries to open it
    /// again with an engine set up as `config` says.
    pub(super) fn retry(&mut self, config: SoundConfig) {
        if matches!(self.state, State::Closed { .. }) {
            self.try_open(config);
        }
    }

    /// Stops the device.
    pub(super) fn close(&mut self) {
        if self.is_open() {
            self.state = State::Closed {
                reason: String::from("the program ended"),
            };
        }
    }

    /// Whether the device is open.
    pub(super) const fn is_open(&self) -> bool {
        matches!(self.state, State::Open { .. })
    }

    /// Why the device is closed, while it is.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the tests only"))]
    pub(super) fn closed_reason(&self) -> Option<&str> {
        match &self.state {
            State::Closed { reason } => Some(reason),
            State::Open { .. } => None,
        }
    }

    /// Where the frame's sound commands and controls go.
    pub(super) fn sink(&mut self) -> &mut dyn SoundSink {
        match &mut self.state {
            State::Open { handle, .. } => handle,
            State::Closed { .. } => &mut self.null,
        }
    }

    /// The notices collected since the last call.
    pub(super) fn take_notices(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notices)
    }

    /// The open stream, if any.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the tests only"))]
    pub(super) const fn stream(&self) -> Option<&B::Stream> {
        match &self.state {
            State::Open { stream, .. } => Some(stream),
            State::Closed { .. } => None,
        }
    }
}

/// The real device: cpal's default host and output.
#[derive(Debug, Default)]
pub(super) struct CpalBackend;

/// A playing cpal stream; dropping it stops the device.
pub(super) struct CpalStream(
    // cpal's browser stream type is never reported unread.
    #[cfg_attr(
        not(target_arch = "wasm32"),
        expect(dead_code, reason = "kept only to keep the stream playing")
    )]
    cpal::Stream,
);

impl fmt::Debug for CpalStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CpalStream")
    }
}

/// The writable format of a cpal sample format.
fn sample_kind(format: cpal::SampleFormat) -> Option<SampleKind> {
    match format {
        cpal::SampleFormat::I16 => Some(SampleKind::I16),
        cpal::SampleFormat::F32 => Some(SampleKind::F32),
        cpal::SampleFormat::U16 => Some(SampleKind::U16),
        _ => None,
    }
}

/// A cpal buffer-size range as a pair.
const fn buffer_range(size: &cpal::SupportedBufferSize) -> Option<(u32, u32)> {
    match *size {
        cpal::SupportedBufferSize::Range { min, max } => Some((min, max)),
        cpal::SupportedBufferSize::Unknown => None,
    }
}

/// Whether a stream error ends the stream. Under- and overruns, a route
/// change the stream followed and a refused real-time priority leave it
/// playing.
const fn is_fatal(kind: cpal::ErrorKind) -> bool {
    !matches!(
        kind,
        cpal::ErrorKind::Xrun | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::RealtimeDenied
    )
}

/// Builds and starts an output stream writing `T` samples.
fn build<T: cpal::SizedSample + OutputSample>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut feeder: Feeder<AudioSource>,
    failed: &Arc<AtomicBool>,
) -> Result<cpal::Stream, cpal::Error> {
    let failed = Arc::clone(failed);
    let stream = device.build_output_stream::<T, _, _>(
        config,
        move |out: &mut [T], _| feeder.fill(out),
        move |error: cpal::Error| {
            if is_fatal(error.kind()) {
                failed.store(true, Ordering::Release);
            }
        },
        None,
    )?;
    stream.play()?;
    Ok(stream)
}

impl Backend for CpalBackend {
    type Stream = CpalStream;

    fn open(
        &mut self,
        request: AudioRequest,
        link: &mut Link<'_>,
    ) -> Result<Opened<CpalStream>, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| String::from("no default output"))?;
        let configs = device.supported_output_configs();
        let default = device.default_output_config();
        // A device that lists nothing and has no default cannot be used;
        // its own error is the better reason.
        if let (Err(error), Err(_)) = (&configs, &default) {
            return Err(error.to_string());
        }
        let capabilities: Vec<Capability> = configs
            .map(|configs| {
                configs
                    .map(|range| Capability {
                        channels: range.channels(),
                        min_rate: range.min_sample_rate(),
                        max_rate: range.max_sample_rate(),
                        format: sample_kind(range.sample_format()),
                        buffer: buffer_range(range.buffer_size()),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let default = default.ok().map(|config| DefaultConfig {
            channels: config.channels(),
            rate: config.sample_rate(),
            format: sample_kind(config.sample_format()),
            buffer: buffer_range(config.buffer_size()),
        });
        let choice = choose(&capabilities, default, request)
            .ok_or_else(|| String::from("the output has no usable sample format"))?;
        let mut notices = choice.notices.clone();
        let failed = Arc::new(AtomicBool::new(false));
        let attempt = |buffer: cpal::BufferSize,
                       link: &mut Link<'_>|
         -> Result<(cpal::Stream, SoundHandle), String> {
            let (handle, source) = link(choice.rate)?;
            let feeder = Feeder::new(source, choice.channels);
            let config = cpal::StreamConfig {
                channels: choice.channels,
                sample_rate: choice.rate,
                buffer_size: buffer,
            };
            let stream = match choice.format {
                SampleKind::I16 => build::<i16>(&device, config, feeder, &failed),
                SampleKind::F32 => build::<f32>(&device, config, feeder, &failed),
                SampleKind::U16 => build::<u16>(&device, config, feeder, &failed),
            }
            .map_err(|error| error.to_string())?;
            Ok((stream, handle))
        };
        let (stream, handle) = match choice.buffer {
            Some(frames) => match attempt(cpal::BufferSize::Fixed(frames), link) {
                Ok(opened) => opened,
                Err(reason) => {
                    notices.push(format!(
                        "the audio device refused a buffer of {frames} frames ({reason}); \
                         using its default"
                    ));
                    attempt(cpal::BufferSize::Default, link)?
                }
            },
            None => attempt(cpal::BufferSize::Default, link)?,
        };
        Ok(Opened {
            stream: CpalStream(stream),
            handle,
            failed,
            notices,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a failed expectation is a failed test"
)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use digger_sound::{SoundCommand, SoundControl, Volume};

    use super::*;

    const REQUEST: AudioRequest = AudioRequest {
        rate: 44_100,
        buffer: 512,
    };

    fn cap(channels: u16, rates: (u32, u32), format: Option<SampleKind>) -> Capability {
        Capability {
            channels,
            min_rate: rates.0,
            max_rate: rates.1,
            format,
            buffer: Some((64, 4096)),
        }
    }

    const STEREO_48K: DefaultConfig = DefaultConfig {
        channels: 2,
        rate: 48_000,
        format: Some(SampleKind::F32),
        buffer: None,
    };

    #[test]
    fn audio_config_selection() {
        // Mono preferred over stereo at the requested rate.
        let caps = [
            cap(2, (8_000, 96_000), Some(SampleKind::I16)),
            cap(1, (8_000, 96_000), Some(SampleKind::F32)),
        ];
        let choice = choose(&caps, Some(STEREO_48K), REQUEST).expect("a choice");
        assert_eq!(
            choice,
            Choice {
                channels: 1,
                rate: 44_100,
                format: SampleKind::F32,
                buffer: Some(512),
                notices: Vec::new(),
            }
        );

        // Same channel count: i16 before f32 before u16.
        let caps = [
            cap(2, (8_000, 96_000), Some(SampleKind::U16)),
            cap(2, (8_000, 96_000), Some(SampleKind::F32)),
            cap(2, (8_000, 96_000), Some(SampleKind::I16)),
        ];
        let choice = choose(&caps, None, REQUEST).expect("a choice");
        assert_eq!((choice.channels, choice.format), (2, SampleKind::I16));

        // Stereo-only, 8-channel device: the fewest channels.
        let caps = [
            cap(8, (44_100, 48_000), Some(SampleKind::F32)),
            cap(2, (44_100, 48_000), Some(SampleKind::F32)),
        ];
        assert_eq!(choose(&caps, None, REQUEST).expect("a choice").channels, 2);

        // Unwritable formats are skipped.
        let caps = [
            cap(1, (8_000, 96_000), None),
            cap(2, (8_000, 96_000), Some(SampleKind::F32)),
        ];
        assert_eq!(choose(&caps, None, REQUEST).expect("a choice").channels, 2);

        // Rate refused: the device default, with one notice.
        let caps = [cap(2, (48_000, 48_000), Some(SampleKind::F32))];
        let choice = choose(&caps, Some(STEREO_48K), REQUEST).expect("a choice");
        assert_eq!((choice.channels, choice.rate), (2, 48_000));
        assert_eq!(
            choice.notices,
            ["the audio device does not support 44100 Hz; using 48000 Hz"]
        );

        // Rate refused and no usable default: the nearest supported rate.
        let caps = [
            cap(2, (8_000, 22_050), Some(SampleKind::I16)),
            cap(2, (48_000, 96_000), Some(SampleKind::I16)),
        ];
        let choice = choose(&caps, None, REQUEST).expect("a choice");
        assert_eq!(choice.rate, 48_000);
        assert_eq!(choice.notices.len(), 1);

        // Buffer size outside the range: the device default, one notice.
        let mut small = cap(1, (8_000, 96_000), Some(SampleKind::I16));
        small.buffer = Some((1024, 4096));
        let choice = choose(&[small], None, REQUEST).expect("a choice");
        assert_eq!(choice.buffer, None);
        assert_eq!(
            choice.notices,
            ["the audio device does not support a buffer of 512 frames; using its default"]
        );

        // Unknown buffer range: the requested size is tried.
        let mut unknown = small;
        unknown.buffer = None;
        assert_eq!(
            choose(&[unknown], None, REQUEST).expect("a choice").buffer,
            Some(512)
        );

        // Nothing writable at all.
        assert_eq!(
            choose(&[cap(2, (8_000, 96_000), None)], None, REQUEST),
            None
        );
        assert_eq!(choose(&[], None, REQUEST), None);
    }

    /// A test signal: 1, 2, 3, … per mono sample.
    struct Ramp(i16);

    impl MonoSource for Ramp {
        fn fill(&mut self, out: &mut [i16]) {
            for sample in out {
                self.0 = self.0.wrapping_add(1);
                *sample = self.0;
            }
        }
    }

    #[test]
    fn audio_fill_duplicates_channels() {
        let mut feeder = Feeder::new(Ramp(0), 2);
        let mut out = [0_i16; 8];
        feeder.fill(&mut out);
        assert_eq!(out, [1, 1, 2, 2, 3, 3, 4, 4]);

        let mut feeder = Feeder::new(Ramp(0), 6);
        let mut out = vec![0.0_f32; 6 * 3];
        feeder.fill(&mut out);
        for (k, frame) in out.chunks(6).enumerate() {
            let expected = f32::from(i16::try_from(k + 1).expect("small")) / 32768.0;
            assert!(frame.iter().all(|&s| (s - expected).abs() < f32::EPSILON));
        }

        // Buffers longer than the scratch buffer continue the signal.
        let frames = SCRATCH_FRAMES * 2 + 7;
        let mut feeder = Feeder::new(Ramp(0), 2);
        let mut out = vec![0_u16; frames * 2];
        feeder.fill(&mut out);
        for (k, frame) in out.chunks(2).enumerate() {
            let expected = i16::try_from(k + 1).expect("small").cast_unsigned() ^ 0x8000;
            assert_eq!(frame, [expected, expected]);
        }

        // Mono is written as is.
        let mut feeder = Feeder::new(Ramp(0), 1);
        let mut out = [0_i16; 3];
        feeder.fill(&mut out);
        assert_eq!(out, [1, 2, 3]);
    }

    #[test]
    fn sample_conversions() {
        assert_eq!(u16::from_i16(0), 32768);
        assert_eq!(u16::from_i16(i16::MIN), 0);
        assert_eq!(u16::from_i16(i16::MAX), u16::MAX);
        assert!((f32::from_i16(i16::MIN) + 1.0).abs() < f32::EPSILON);
        assert!(f32::from_i16(0).abs() < f32::EPSILON);
    }

    #[test]
    fn engine_output_reaches_every_channel() {
        let tables = TuneTables::embedded().expect("tables");
        let (mut handle, source) = sound_link(SoundConfig::DEFAULT, &tables).expect("link");
        handle.send(SoundCommand::Gold);
        let mut feeder = Feeder::new(source, 2);
        let mut out = vec![0_i16; 2 * 4410];
        feeder.fill(&mut out);
        assert!(out.iter().any(|&s| s != 0), "the effect is audible");
        assert!(out.chunks(2).all(|frame| frame[0] == frame[1]));
    }

    /// What the fake device does on each open.
    #[derive(Debug, Clone, Copy)]
    enum Plan {
        Missing,
        Present,
    }

    /// A fake device: present or missing per open; records each rate the
    /// engine was linked at and keeps the sources alive.
    #[derive(Debug)]
    struct Fake {
        plans: Vec<Plan>,
        opens: Rc<RefCell<u32>>,
        sources: Vec<AudioSource>,
    }

    impl Backend for Fake {
        type Stream = Arc<AtomicBool>;

        fn open(
            &mut self,
            _request: AudioRequest,
            link: &mut Link<'_>,
        ) -> Result<Opened<Self::Stream>, String> {
            *self.opens.borrow_mut() += 1;
            let plan = if self.plans.is_empty() {
                Plan::Missing
            } else {
                self.plans.remove(0)
            };
            match plan {
                Plan::Missing => Err(String::from("no default output")),
                Plan::Present => {
                    let (handle, source) = link(48_000)?;
                    self.sources.push(source);
                    let failed = Arc::new(AtomicBool::new(false));
                    Ok(Opened {
                        stream: Arc::clone(&failed),
                        handle,
                        failed,
                        notices: Vec::new(),
                    })
                }
            }
        }
    }

    fn fake(plans: &[Plan]) -> (Fake, Rc<RefCell<u32>>) {
        let opens = Rc::new(RefCell::new(0));
        (
            Fake {
                plans: plans.to_vec(),
                opens: Rc::clone(&opens),
                sources: Vec::new(),
            },
            opens,
        )
    }

    #[test]
    fn audio_state_transitions() {
        // No device at start: closed, one notice, sound goes nowhere.
        let (backend, opens) = fake(&[Plan::Missing, Plan::Present, Plan::Present]);
        let mut audio = Audio::open(backend, REQUEST, SoundConfig::DEFAULT);
        assert!(!audio.is_open());
        assert_eq!(audio.closed_reason(), Some("no default output"));
        let notices = audio.take_notices();
        assert_eq!(notices.len(), 1);
        assert!(
            report_line(&notices[0]).starts_with("digger: notice: no audio device"),
            "{notices:?}"
        );
        audio.sink().send(SoundCommand::Gold);
        audio.sink().control(SoundControl::ToggleSound);
        audio.check();
        assert_eq!(audio.take_notices(), Vec::<String>::new());

        // The sound toggle retries; the device is now there.
        audio.retry(SoundConfig::DEFAULT);
        assert!(audio.is_open());
        assert_eq!(audio.take_notices(), Vec::<String>::new());
        assert_eq!(*opens.borrow(), 2);

        // A retry while open does nothing.
        audio.retry(SoundConfig::DEFAULT);
        assert_eq!(*opens.borrow(), 2);

        // The stream fails at run time: closed at the next frame, one
        // notice, the game carries on.
        audio.stream().expect("open").store(true, Ordering::Release);
        audio.check();
        assert!(!audio.is_open());
        assert_eq!(audio.take_notices().len(), 1);
        audio.check();
        assert_eq!(audio.take_notices(), Vec::<String>::new());
        audio.sink().send(SoundCommand::Gold);

        // Switching sound on again reopens.
        audio.retry(SoundConfig {
            volume: Volume::new(50),
            ..SoundConfig::DEFAULT
        });
        assert!(audio.is_open());
        assert_eq!(*opens.borrow(), 3);

        // A failed retry stays closed with a notice.
        audio.close();
        assert!(!audio.is_open());
        audio.retry(SoundConfig::DEFAULT);
        assert!(!audio.is_open());
        assert_eq!(audio.take_notices().len(), 1);
    }

    fn report_line(message: &str) -> String {
        crate::report::line(crate::report::Level::Notice, message)
    }
}
