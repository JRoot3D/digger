//! The winit `ApplicationHandler` implementation and event translation
//! (design D4).
//!
//! The event loop never blocks inside a callback. Window events are turned
//! into input as they arrive; `about_to_wait` asks the pacer how many frames
//! are due (after draining the gamepads' events), runs each through
//! [`Shell::frame`], forwards its sound to the audio device and
//! applies its requests, then asks for a redraw and tells the event loop
//! when to wake up next: at the next frame's deadline, or at once when the
//! game runs unpaced. `RedrawRequested` presents the shell's picture.
//!
//! A close request is data for the shell, not an exit: the next wake-up runs
//! one frame at once with `close_requested` set, the current scene ends the
//! way it ends on quit (a game steps once more so its recording gets its
//! end-of-game marker and is written), and the shell asks for the exit.

use std::time::Duration;

use digger_sound::{NullDriver, SoundConfig, SoundSink, Volume};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::window::WindowId;

use super::audio::CpalBackend;
use super::pads::GamepadPoller;
use super::window::GameWindow;
use crate::config::ScaleMode;
use crate::input::{InputState, KeyPress, PadEvent, Pads};
use crate::pacing::{Clock, Control, Pacer, SystemClock};
use crate::platform::audio::{Audio, AudioRequest};
use crate::platform::keys::key_name;
use crate::report;
use crate::shell::{FrameOutput, Request, Shell, ShellInput};

/// How the window opens.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowOptions {
    /// Start fullscreen (`video.fullscreen`, `--fullscreen`).
    pub(crate) fullscreen: bool,
    /// The scale mode (`video.scale`, `--scale`).
    pub(crate) scale: ScaleMode,
    /// Print the debug frame counter (design D15).
    pub(crate) frame_counter: bool,
}

/// How often the debug frame counter reports.
const COUNTER_PERIOD: Duration = Duration::from_secs(10);

/// The debug frame counter: frames run since the first one, reported on
/// standard error every [`COUNTER_PERIOD`] of wall-clock time, so the
/// pacing can be checked against a stopwatch (750 frames a minute at the
/// default speed).
#[derive(Debug, Clone, Copy, Default)]
struct FrameCounter {
    /// When the first frame ran.
    start: Option<Duration>,
    /// Frames run since then.
    frames: u64,
    /// How many reports were printed.
    reports: u32,
}

impl FrameCounter {
    /// Counts a frame run at `now`; returns the report line when one is
    /// due.
    fn frame(&mut self, now: Duration) -> Option<String> {
        let start = *self.start.get_or_insert(now);
        self.frames += 1;
        let elapsed = now.saturating_sub(start);
        if elapsed >= COUNTER_PERIOD * (self.reports + 1) {
            self.reports += 1;
            Some(format!(
                "frame counter: {} frames in {:.3} s",
                self.frames,
                elapsed.as_secs_f64()
            ))
        } else {
            None
        }
    }
}

/// How the event loop ended.
#[derive(Debug)]
pub(super) enum Ended {
    /// The shell asked for the exit with this status.
    Exit(u8),
    /// The window could not be opened.
    NoWindow(String),
}

/// The application: owns the shell, the input state, the pacer, the window,
/// the audio device and the gamepads (design D13).
#[derive(Debug)]
pub(crate) struct App {
    shell: Shell,
    input: InputState,
    pacer: Pacer<SystemClock>,
    /// The audio device, once the window exists (design D6).
    audio: Option<Audio<CpalBackend>>,
    /// Where the frames' sound goes before the device is opened.
    null_sound: NullDriver,
    /// Pad slots and stick state (design D7).
    pads: Pads,
    /// The gamepads, when `gamepad.enabled` and the system can read them.
    poller: Option<GamepadPoller>,
    /// Pad events of the current wake-up (kept to reuse its allocation).
    pad_events: Vec<PadEvent>,
    window: Option<GameWindow>,
    options: WindowOptions,
    /// The window's close button was used since the last frame.
    close_requested: bool,
    /// How the program ends, once known.
    ended: Option<Ended>,
    counter: Option<FrameCounter>,
}

impl App {
    /// The application around `shell`, before the window exists.
    pub(crate) fn new(shell: Shell, options: WindowOptions) -> Self {
        let input = InputState::new(shell.bindings());
        let pacer = Pacer::new(SystemClock::new(), shell.frame_time());
        let pads = Pads::new(shell.settings().dead_zone);
        Self {
            shell,
            input,
            pacer,
            audio: None,
            null_sound: NullDriver,
            pads,
            poller: None,
            pad_events: Vec::new(),
            window: None,
            options,
            close_requested: false,
            ended: None,
            counter: options.frame_counter.then(FrameCounter::default),
        }
    }

    /// The single shutdown path (`desktop-app` spec, clean shutdown): if the
    /// shell has not ended yet (the event loop stopped for another reason),
    /// it runs one more frame with a close request, so a game in progress
    /// ends and its recording is written; then the sound stops. Returns the
    /// shell, for the settings save and the verification report, and how
    /// the program ends.
    pub(super) fn shut_down(mut self) -> (Shell, Ended) {
        if self.ended.is_none() {
            let input = ShellInput {
                input: self.input.take_frame(),
                close_requested: true,
            };
            self.frame(&input, None);
        }
        let ended = self.ended.take().unwrap_or(Ended::Exit(0));
        // The sound stops before the window closes.
        if let Some(audio) = self.audio.as_mut() {
            audio.close();
        }
        drop(self.window);
        (self.shell, ended)
    }

    /// Runs one shell frame and applies its output.
    fn frame(&mut self, input: &ShellInput, event_loop: Option<&ActiveEventLoop>) {
        let FrameOutput {
            sound,
            controls,
            frame_time,
            requests,
            notices,
            ..
        } = self.shell.frame(input);
        for notice in notices {
            report::notice(notice);
        }
        let sink: &mut dyn SoundSink = match self.audio.as_mut() {
            Some(audio) => {
                audio.check();
                audio.sink()
            }
            None => &mut self.null_sound,
        };
        for command in sound {
            sink.send(command);
        }
        for control in controls {
            sink.control(control);
        }
        if frame_time != self.pacer.frame_time() {
            self.pacer.set_frame_time(frame_time);
        }
        for request in requests {
            match request {
                Request::ToggleFullscreen => {
                    if let Some(window) = self.window.as_mut() {
                        let fullscreen = window.toggle_fullscreen();
                        self.shell.fullscreen_changed(fullscreen);
                    }
                }
                Request::RetryAudio => {
                    let config = self.sound_config();
                    if let Some(audio) = self.audio.as_mut() {
                        audio.retry(config);
                    }
                }
                Request::Exit(status) => {
                    self.ended.get_or_insert(Ended::Exit(status));
                    if let Some(event_loop) = event_loop {
                        event_loop.exit();
                    }
                }
                Request::BindingsChanged => self.input.set_bindings(self.shell.bindings()),
            }
        }
        self.report_audio_notices();
        if let Some(counter) = self.counter.as_mut()
            && let Some(line) = counter.frame(self.pacer.clock().now())
        {
            report::notice(line);
        }
    }

    /// The engine set-up for opening the audio device now: the sound and
    /// music switches as the shell has them (so a quiet start still opens
    /// the device and the toggle works at once), the master volume, the
    /// output filter on; the rate is the device's.
    fn sound_config(&self) -> SoundConfig {
        SoundConfig {
            sound_on: self.shell.sound_on(),
            music_on: self.shell.music_on(),
            volume: Volume::new(self.shell.settings().volume),
            ..SoundConfig::DEFAULT
        }
    }

    /// Opens the audio device and starts reading gamepads (`desktop-app`
    /// spec, audio device; `input-controls` spec, gamepad assignment).
    fn open_devices(&mut self) {
        let settings = self.shell.settings();
        let request = AudioRequest {
            rate: settings.sample_rate,
            buffer: settings.buffer_size,
        };
        let pads_enabled = settings.pad_enabled;
        self.audio = Some(Audio::running(CpalBackend, request, self.sound_config()));
        self.report_audio_notices();
        if pads_enabled {
            match GamepadPoller::new() {
                Ok(poller) => self.poller = Some(poller),
                Err(reason) => report::notice(format!("gamepads cannot be read: {reason}")),
            }
        }
    }

    /// Prints the audio device's notices.
    fn report_audio_notices(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            for notice in audio.take_notices() {
                report::notice(notice);
            }
        }
    }

    /// Feeds the pad events since the last wake-up into the input state.
    fn poll_pads(&mut self) {
        if let Some(poller) = self.poller.as_mut() {
            poller.poll(&mut self.pad_events);
            for event in self.pad_events.drain(..) {
                self.input.pad_event(&mut self.pads, event);
            }
        }
    }

    /// A key event: a press or release of a physical key, with its text.
    fn key(&mut self, event: &KeyEvent, is_synthetic: bool) {
        let key = key_name(event.physical_key);
        match event.state {
            ElementState::Pressed => {
                // X11 reports the keys held when focus returns as synthetic
                // presses; they are not new presses (`desktop-app` spec, a
                // key held from before never starts a game).
                if is_synthetic {
                    return;
                }
                self.input.key_down(KeyPress {
                    key,
                    repeat: event.repeat,
                    text: event.text.as_deref(),
                });
            }
            ElementState::Released => {
                if let Some(key) = key {
                    self.input.key_up(key);
                }
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() || self.ended.is_some() {
            return;
        }
        match GameWindow::open(event_loop, self.options.fullscreen, self.options.scale) {
            Ok(window) => {
                window.request_redraw();
                self.window = Some(window);
                self.open_devices();
                // The first frame is due now that there is a window.
                self.pacer.reanchor();
            }
            Err(reason) => {
                self.ended = Some(Ended::NoWindow(reason));
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.close_requested = true,
            WindowEvent::Focused(false) => self.input.focus_lost(),
            WindowEvent::Focused(true) => self.pacer.reanchor(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => self.key(&event, is_synthetic),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input.set_alt(modifiers.state().alt_key());
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if let Some(window) = self.window.as_mut() {
                    window.resized();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(window) = self.window.as_mut() {
                    window.present(self.shell.picture());
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.ended.is_some() || self.window.is_none() {
            return;
        }
        if std::mem::take(&mut self.close_requested) {
            let input = ShellInput {
                input: self.input.take_frame(),
                close_requested: true,
            };
            self.frame(&input, Some(event_loop));
            // Every scene ends on a close request; should one not, the
            // window still closes.
            if self.ended.is_none() {
                self.ended = Some(Ended::Exit(0));
            }
            event_loop.exit();
            return;
        }
        self.poll_pads();
        self.pacer.begin();
        while self.pacer.next_frame() {
            let input = ShellInput::from(self.input.take_frame());
            self.frame(&input, Some(event_loop));
            if self.ended.is_some() {
                return;
            }
        }
        let wake = self.pacer.end();
        if wake.redraw
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        event_loop.set_control_flow(match wake.control {
            Control::WaitUntil(at) => ControlFlow::WaitUntil(self.pacer.clock().instant(at)),
            Control::Poll => ControlFlow::Poll,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::FrameCounter;

    #[test]
    fn frame_counter_reports_every_ten_seconds() {
        let mut counter = FrameCounter::default();
        let frame = Duration::from_millis(80);
        let mut lines = Vec::new();
        for k in 0..=1500_u32 {
            if let Some(line) = counter.frame(frame * k) {
                lines.push(line);
            }
        }
        assert_eq!(
            lines,
            [
                "frame counter: 126 frames in 10.000 s",
                "frame counter: 251 frames in 20.000 s",
                "frame counter: 376 frames in 30.000 s",
                "frame counter: 501 frames in 40.000 s",
                "frame counter: 626 frames in 50.000 s",
                "frame counter: 751 frames in 60.000 s",
                "frame counter: 876 frames in 70.000 s",
                "frame counter: 1001 frames in 80.000 s",
                "frame counter: 1126 frames in 90.000 s",
                "frame counter: 1251 frames in 100.000 s",
                "frame counter: 1376 frames in 110.000 s",
                "frame counter: 1501 frames in 120.000 s",
            ]
        );
    }
}
