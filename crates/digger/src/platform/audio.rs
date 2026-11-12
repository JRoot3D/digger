//! The audio output lifecycle, shared by the desktop and the browser
//! (design D6 of `add-desktop-app`; add-web-build-and-releases, design
//! decision 7).
//!
//! Three parts, none of which touches a device:
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
//! - [`Audio`] is the lifecycle, a state machine over [`AudioOutput`]:
//!   `Off` (no engine), `Locked` (the frontend owns the engine's
//!   [`AudioSource`] and advances it silently until a user gesture allows
//!   sound: the browser's start state), `Running` (the source has moved
//!   into the device stream: the desktop's start state) and `Failed` (no
//!   device, it could not be opened or it failed while running:
//!   `desktop-app` spec, running without an audio device). Without a
//!   running stream, sound commands still reach the engine while it is
//!   locked, and go to a [`NullDriver`] otherwise; switching sound on
//!   unlocks or tries to open the device again. The device is behind the
//!   [`Backend`] trait, so the transitions are tested without a sound card;
//!   the platform's cpal backend is the real one.
//!
//! The stream is created, kept and dropped on the main thread, because
//! cpal streams are not `Send` on every host; only the [`Feeder`] moves to
//! the audio thread.

use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use digger_sound::{
    AudioSource, NullDriver, SampleRate, SoundConfig, SoundHandle, SoundSink, TuneTables,
    sound_link,
};

use crate::page::AudioStatus;

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

    /// Pauses a playing stream (the page was hidden). The error is the
    /// reason, for the notice.
    fn pause(stream: &Self::Stream) -> Result<(), String>;

    /// Plays a paused stream again.
    fn play(stream: &Self::Stream) -> Result<(), String>;
}

/// Where the sound goes (add-web-build-and-releases, design decision 7).
#[derive(Debug)]
pub(super) enum AudioOutput<S> {
    /// No engine: sound is switched off for good, or the program ended.
    Off,
    /// The engine exists, but no device may be opened yet: the frontend
    /// owns its [`AudioSource`] and advances it silently, so tunes and
    /// effects progress as if heard.
    Locked {
        handle: SoundHandle,
        /// Boxed: the synthesiser is large next to the other states.
        source: Box<AudioSource>,
        /// The rate the engine was made for, Hz.
        rate: u32,
    },
    /// The source has moved into the device stream.
    Running {
        stream: S,
        handle: SoundHandle,
        failed: Arc<AtomicBool>,
    },
    /// No device: sound goes to [`NullDriver`].
    Failed {
        /// Why, as the notice said.
        reason: String,
    },
}

/// How many samples the silent advance renders at a time.
const SILENT_CHUNK: usize = 1024;

/// Builds the engine as `config` says at `rate`.
fn link_at(rate: u32, config: SoundConfig) -> Result<(SoundHandle, AudioSource), String> {
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
}

/// The audio output and its engine.
#[derive(Debug)]
pub(super) struct Audio<B: Backend> {
    backend: B,
    request: AudioRequest,
    state: AudioOutput<B::Stream>,
    null: NullDriver,
    notices: Vec<String>,
    /// The page is hidden: a running stream is paused and a locked engine
    /// is not advanced.
    suspended: bool,
    /// Nanoseconds of locked time not yet rendered as a whole sample, times
    /// the rate.
    carry: u128,
    /// The silent advance's buffer, allocated on first use.
    scratch: Vec<i16>,
}

impl<B: Backend> Audio<B> {
    /// The lifecycle in `state`.
    fn with_state(backend: B, request: AudioRequest, state: AudioOutput<B::Stream>) -> Self {
        Self {
            backend,
            request,
            state,
            null: NullDriver,
            notices: Vec::new(),
            suspended: false,
            carry: 0,
            scratch: Vec::new(),
        }
    }

    /// The desktop's start: already unlocked. Opens the device for
    /// `request`, with an engine set up as `config` says (its rate is
    /// replaced by the device's); `Failed`, with a notice, when that fails.
    pub(super) fn running(backend: B, request: AudioRequest, config: SoundConfig) -> Self {
        let mut audio = Self::with_state(backend, request, AudioOutput::Off);
        audio.open_fresh(config);
        audio
    }

    /// The browser's start: the engine at the requested rate, `Locked`
    /// until [`Audio::unlock`]; `Failed`, with a notice, when the engine
    /// cannot be made.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's start state (task 3.4)")
    )]
    pub(super) fn locked(backend: B, request: AudioRequest, config: SoundConfig) -> Self {
        let mut audio = Self::with_state(backend, request, AudioOutput::Off);
        audio.lock(config);
        audio
    }

    /// The quiet start without an engine: `Off` until sound is switched
    /// on, which unlocks in one step (the key press is itself a gesture).
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's quiet start (task 3.4)")
    )]
    pub(super) fn off(backend: B, request: AudioRequest) -> Self {
        Self::with_state(backend, request, AudioOutput::Off)
    }

    /// Makes the engine at the requested rate and holds it, locked.
    fn lock(&mut self, config: SoundConfig) {
        let rate = self.request.rate;
        match link_at(rate, config) {
            Ok((handle, source)) => {
                self.state = AudioOutput::Locked {
                    handle,
                    source: Box::new(source),
                    rate,
                };
                self.carry = 0;
            }
            Err(reason) => self.fail(reason),
        }
    }

    /// Records a failure to open: `Failed`, with the notice.
    fn fail(&mut self, reason: String) {
        self.notices.push(format!(
            "no audio device ({reason}); the game continues without sound"
        ));
        self.state = AudioOutput::Failed { reason };
    }

    /// Opens the device with a new engine.
    fn open_fresh(&mut self, config: SoundConfig) {
        let mut link = |rate: u32| link_at(rate, config);
        let opened = self.backend.open(self.request, &mut link);
        self.opened(opened);
    }

    /// The outcome of an open.
    fn opened(&mut self, opened: Result<Opened<B::Stream>, String>) {
        match opened {
            Ok(opened) => {
                self.notices.extend(opened.notices);
                self.state = AudioOutput::Running {
                    stream: opened.stream,
                    handle: opened.handle,
                    failed: opened.failed,
                };
                if self.suspended {
                    self.pause();
                }
            }
            Err(reason) => self.fail(reason),
        }
    }

    /// A user gesture allows sound: a locked engine's source moves into a
    /// new device stream, so the sound continues from where it silently
    /// was; with no engine (`Off`), one is made and moved in the same
    /// step. A device that gives another rate than the engine's gets a new
    /// engine at its rate. A failed open gives `Failed` and a notice.
    /// Running or failed audio is left as it is.
    pub(super) fn unlock(&mut self, config: SoundConfig) {
        if matches!(self.state, AudioOutput::Off) {
            self.lock(config);
        }
        let AudioOutput::Locked { .. } = self.state else {
            return;
        };
        let AudioOutput::Locked {
            handle,
            source,
            rate,
        } = std::mem::replace(&mut self.state, AudioOutput::Off)
        else {
            return;
        };
        let mut held = Some((handle, *source));
        let mut link = |device_rate: u32| match held.take() {
            Some(pair) if device_rate == rate => Ok(pair),
            _ => link_at(device_rate, config),
        };
        let opened = self.backend.open(self.request, &mut link);
        self.opened(opened);
    }

    /// Advances a locked engine silently by `elapsed` of real time: its
    /// output is rendered into a scratch buffer and dropped. Does nothing
    /// in any other state, or while suspended.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's locked audio (task 3.4)")
    )]
    pub(super) fn advance(&mut self, elapsed: Duration) {
        if self.suspended {
            return;
        }
        let AudioOutput::Locked { source, rate, .. } = &mut self.state else {
            return;
        };
        let total = self.carry + elapsed.as_nanos() * u128::from(*rate);
        let mut samples = total / 1_000_000_000;
        self.carry = total % 1_000_000_000;
        if self.scratch.is_empty() {
            self.scratch = vec![0; SILENT_CHUNK];
        }
        while samples > 0 {
            let n = usize::try_from(samples).map_or(SILENT_CHUNK, |n| n.min(SILENT_CHUNK));
            let Some(chunk) = self.scratch.get_mut(..n) else {
                return;
            };
            source.fill(chunk);
            samples -= n as u128;
        }
    }

    /// The page was hidden: a running stream pauses and a locked engine
    /// stops advancing.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's visibility handling (task 3.6)")
    )]
    pub(super) fn suspend(&mut self) {
        if !std::mem::replace(&mut self.suspended, true) {
            self.pause();
        }
    }

    /// The page is visible again: a paused stream plays.
    #[cfg_attr(
        not(any(test, target_arch = "wasm32")),
        expect(dead_code, reason = "the browser's visibility handling (task 3.6)")
    )]
    pub(super) fn resume(&mut self) {
        if std::mem::replace(&mut self.suspended, false)
            && let AudioOutput::Running { stream, .. } = &self.state
            && let Err(reason) = B::play(stream)
        {
            self.notices
                .push(format!("the audio device cannot play again ({reason})"));
        }
    }

    /// Pauses a running stream, with a notice on failure.
    fn pause(&mut self) {
        if let AudioOutput::Running { stream, .. } = &self.state
            && let Err(reason) = B::pause(stream)
        {
            self.notices
                .push(format!("the audio device cannot pause ({reason})"));
        }
    }

    /// Once per frame: a stream the device reported as failed is closed,
    /// with a notice, and the game carries on in silence.
    pub(super) fn check(&mut self) {
        if let AudioOutput::Running { failed, .. } = &self.state
            && failed.load(Ordering::Acquire)
        {
            let reason = String::from("the audio device failed");
            self.notices
                .push(format!("{reason}; the game continues without sound"));
            self.state = AudioOutput::Failed { reason };
        }
    }

    /// Sound was switched on: failed audio tries to open the device again
    /// with an engine set up as `config` says; locked or off audio unlocks,
    /// since the key press is a gesture.
    pub(super) fn retry(&mut self, config: SoundConfig) {
        match self.state {
            AudioOutput::Failed { .. } => self.open_fresh(config),
            AudioOutput::Off | AudioOutput::Locked { .. } => self.unlock(config),
            AudioOutput::Running { .. } => {}
        }
    }

    /// Stops the device and drops the engine.
    pub(super) fn close(&mut self) {
        self.state = AudioOutput::Off;
    }

    /// The state, for the page.
    pub(super) const fn status(&self) -> AudioStatus {
        match self.state {
            AudioOutput::Off => AudioStatus::Off,
            AudioOutput::Locked { .. } => AudioStatus::Locked,
            AudioOutput::Running { .. } => AudioStatus::Running,
            AudioOutput::Failed { .. } => AudioStatus::Failed,
        }
    }

    /// Whether the device plays.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the tests only"))]
    pub(super) const fn is_running(&self) -> bool {
        matches!(self.state, AudioOutput::Running { .. })
    }

    /// Why the device is not there, while it is not.
    #[cfg_attr(not(test), expect(dead_code, reason = "read by the tests only"))]
    pub(super) fn failed_reason(&self) -> Option<&str> {
        match &self.state {
            AudioOutput::Failed { reason } => Some(reason),
            _ => None,
        }
    }

    /// Where the frame's sound commands and controls go: the engine while
    /// it is locked or running, nowhere otherwise.
    pub(super) fn sink(&mut self) -> &mut dyn SoundSink {
        match &mut self.state {
            AudioOutput::Running { handle, .. } | AudioOutput::Locked { handle, .. } => handle,
            AudioOutput::Off | AudioOutput::Failed { .. } => &mut self.null,
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
            AudioOutput::Running { stream, .. } => Some(stream),
            _ => None,
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "a failed expectation is a failed test"
)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use digger_sound::{SoundCommand, SoundControl, Tempo, Tune, Volume};

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

    /// A fake device: present or missing per open, at `rate`; keeps the
    /// sources it was given alive, in order.
    #[derive(Debug)]
    struct Fake {
        plans: Vec<Plan>,
        rate: u32,
        opens: Rc<RefCell<u32>>,
        sources: Vec<AudioSource>,
    }

    /// The fake device's stream: the failure flag and whether it is
    /// paused.
    #[derive(Debug)]
    struct FakeStream {
        failed: Arc<AtomicBool>,
        paused: Cell<bool>,
    }

    impl Backend for Fake {
        type Stream = FakeStream;

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
                    let (handle, source) = link(self.rate)?;
                    self.sources.push(source);
                    let failed = Arc::new(AtomicBool::new(false));
                    Ok(Opened {
                        stream: FakeStream {
                            failed: Arc::clone(&failed),
                            paused: Cell::new(false),
                        },
                        handle,
                        failed,
                        notices: Vec::new(),
                    })
                }
            }
        }

        fn pause(stream: &FakeStream) -> Result<(), String> {
            stream.paused.set(true);
            Ok(())
        }

        fn play(stream: &FakeStream) -> Result<(), String> {
            stream.paused.set(false);
            Ok(())
        }
    }

    fn fake(plans: &[Plan]) -> (Fake, Rc<RefCell<u32>>) {
        let opens = Rc::new(RefCell::new(0));
        (
            Fake {
                plans: plans.to_vec(),
                rate: 48_000,
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
        let mut audio = Audio::running(backend, REQUEST, SoundConfig::DEFAULT);
        assert!(!audio.is_running());
        assert_eq!(audio.status(), AudioStatus::Failed);
        assert_eq!(audio.failed_reason(), Some("no default output"));
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
        assert!(audio.is_running());
        assert_eq!(audio.take_notices(), Vec::<String>::new());
        assert_eq!(*opens.borrow(), 2);

        // A retry while open does nothing.
        audio.retry(SoundConfig::DEFAULT);
        assert_eq!(*opens.borrow(), 2);

        // The stream fails at run time: closed at the next frame, one
        // notice, the game carries on.
        audio
            .stream()
            .expect("open")
            .failed
            .store(true, Ordering::Release);
        audio.check();
        assert!(!audio.is_running());
        assert_eq!(audio.take_notices().len(), 1);
        audio.check();
        assert_eq!(audio.take_notices(), Vec::<String>::new());
        audio.sink().send(SoundCommand::Gold);

        // Switching sound on again reopens.
        audio.retry(SoundConfig {
            volume: Volume::new(50),
            ..SoundConfig::DEFAULT
        });
        assert!(audio.is_running());
        assert_eq!(*opens.borrow(), 3);

        // A failed retry stays closed with a notice.
        audio.close();
        assert!(!audio.is_running());
        audio.retry(SoundConfig::DEFAULT);
        assert!(!audio.is_running());
        assert_eq!(audio.take_notices().len(), 1);
    }

    /// The audio request of the locked tests: the fake device's rate.
    const REQUEST_48K: AudioRequest = AudioRequest {
        rate: 48_000,
        buffer: 512,
    };

    /// The main tune at standard tempo.
    const MUSIC: SoundCommand = SoundCommand::Music {
        tune: Tune::Main,
        tempo: Tempo::Standard,
    };

    #[test]
    fn audio_locked_advances_tune_position() {
        // A reference engine that is heard all along.
        let (mut reference, mut heard) = link_at(48_000, SoundConfig::DEFAULT).expect("link");
        reference.send(MUSIC);
        let (backend, opens) = fake(&[Plan::Present]);
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        assert_eq!(audio.status(), AudioStatus::Locked);
        assert_eq!(*opens.borrow(), 0, "no device before a gesture");
        audio.sink().send(MUSIC);
        // Ten seconds of music, silently, in frame-sized steps of 80 ms.
        let mut skipped = vec![0_i16; 480_000];
        heard.fill(&mut skipped);
        for _ in 0..125 {
            audio.advance(Duration::from_millis(80));
        }
        audio.unlock(SoundConfig::DEFAULT);
        assert_eq!(audio.status(), AudioStatus::Running);
        // The device continues the tune 10 seconds in, not from its start.
        let mut expected = vec![0_i16; 4_800];
        heard.fill(&mut expected);
        let mut played = vec![0_i16; 4_800];
        audio.backend.sources[0].fill(&mut played);
        assert!(expected.iter().any(|&s| s != 0), "the tune is audible");
        assert_eq!(played, expected);
        // A fresh engine would start the tune from the beginning.
        let (mut fresh_handle, mut fresh) = link_at(48_000, SoundConfig::DEFAULT).expect("link");
        fresh_handle.send(MUSIC);
        let mut start = vec![0_i16; 4_800];
        fresh.fill(&mut start);
        assert_ne!(start, expected);
        // An effect that ended while locked is not replayed.
        let (backend, _) = fake(&[Plan::Present]);
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.sink().send(SoundCommand::Gold);
        audio.advance(Duration::from_secs(5));
        audio.unlock(SoundConfig::DEFAULT);
        let mut after = vec![0_i16; 4_800];
        audio.backend.sources[0].fill(&mut after);
        assert!(
            after.iter().all(|&s| s == after[0]),
            "silence after the effect"
        );
    }

    #[test]
    fn audio_unlock_moves_source_once() {
        let (backend, opens) = fake(&[Plan::Present, Plan::Present]);
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.unlock(SoundConfig::DEFAULT);
        assert!(audio.is_running());
        assert_eq!(*opens.borrow(), 1);
        assert_eq!(audio.backend.sources.len(), 1, "the held source moved in");
        // Later gestures and sound toggles leave the stream alone.
        audio.unlock(SoundConfig::DEFAULT);
        audio.retry(SoundConfig::DEFAULT);
        assert_eq!(*opens.borrow(), 1);
        // Advancing is the device's job now.
        audio.advance(Duration::from_secs(1));
        assert_eq!(audio.take_notices(), Vec::<String>::new());
        // A device at another rate gets an engine at its rate.
        let (mut backend, _) = fake(&[Plan::Present]);
        backend.rate = 44_100;
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.unlock(SoundConfig::DEFAULT);
        assert!(audio.is_running());
        // A refused open fails with a notice; sound goes nowhere.
        let (backend, _) = fake(&[Plan::Missing]);
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.unlock(SoundConfig::DEFAULT);
        assert_eq!(audio.status(), AudioStatus::Failed);
        assert_eq!(audio.take_notices().len(), 1);
        audio.sink().send(SoundCommand::Gold);
    }

    #[test]
    fn audio_quiet_then_toggle_unlocks() {
        let (backend, opens) = fake(&[Plan::Present]);
        let mut audio = Audio::off(backend, REQUEST_48K);
        assert_eq!(audio.status(), AudioStatus::Off);
        audio.sink().send(SoundCommand::Gold);
        audio.advance(Duration::from_secs(1));
        assert_eq!(*opens.borrow(), 0);
        // The sound toggle key is a gesture: off, locked and running in
        // one step.
        audio.retry(SoundConfig::DEFAULT);
        assert_eq!(audio.status(), AudioStatus::Running);
        assert_eq!(*opens.borrow(), 1);
    }

    #[test]
    fn audio_suspend_and_resume() {
        let (backend, _) = fake(&[Plan::Present]);
        let mut audio = Audio::running(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.suspend();
        assert!(audio.stream().expect("open").paused.get());
        audio.resume();
        assert!(!audio.stream().expect("open").paused.get());
        // While suspended, a locked engine does not advance.
        let (backend, _) = fake(&[Plan::Present]);
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.sink().send(MUSIC);
        audio.suspend();
        audio.advance(Duration::from_secs(10));
        audio.resume();
        audio.unlock(SoundConfig::DEFAULT);
        let (mut handle, mut start) = link_at(48_000, SoundConfig::DEFAULT).expect("link");
        handle.send(MUSIC);
        let mut expected = vec![0_i16; 4_800];
        start.fill(&mut expected);
        let mut played = vec![0_i16; 4_800];
        audio.backend.sources[0].fill(&mut played);
        assert_eq!(played, expected, "the tune starts where it was hidden");
        // A stream opened while hidden starts paused.
        let (backend, _) = fake(&[Plan::Present]);
        let mut audio = Audio::locked(backend, REQUEST_48K, SoundConfig::DEFAULT);
        audio.suspend();
        audio.unlock(SoundConfig::DEFAULT);
        assert!(audio.stream().expect("open").paused.get());
    }

    fn report_line(message: &str) -> String {
        crate::report::line(crate::report::Level::Notice, message)
    }
}
