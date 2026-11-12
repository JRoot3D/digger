//! The browser's winit application (add-web-build-and-releases, design
//! decisions 4 to 8).
//!
//! The browser never lets a program block, and the game does not need it
//! to: every winit callback returns at once. The game is driven from
//! `RedrawRequested`, which winit ties to the browser's animation callback;
//! each one asks for the next before it returns. Per callback the gamepads
//! are polled once, a locked audio engine is advanced silently by the time
//! since the previous callback, the pacer says how many game frames are
//! due (at most 3; with speed 0 as many as fit in 8 ms), each runs through
//! [`Shell::frame`], and the picture is presented when a frame ran.
//!
//! The first key, pointer or gamepad button press unlocks the audio
//! before it is handled as input (`web-build` spec, audio starts only
//! after a user gesture). A hidden page stops the stepping and the audio;
//! during live play the game first enters its own pause phase. The game's
//! exit outcome never closes the page: the shell runs with the
//! stay-on-title exit policy. Settings are saved as they change.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use digger_sound::{SoundConfig, SoundSink, Volume};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

use super::canvas::GameCanvas;
use super::dom;
use super::page::WebPageUi;
use crate::config::ScaleMode;
use crate::event::AppEvent;
use crate::input::{Bindings, InputState, KeyPress, PadEvent, Pads};
use crate::pacing::{Clock, Pacer, SystemClock};
use crate::page::{AudioStatus, PageUi, Phase, Status};
use crate::platform::audio::{Audio, AudioRequest};
use crate::platform::cpal_output::CpalBackend;
use crate::platform::keys::key_name;
use crate::platform::pads::GamepadPoller;
use crate::report::Level;
use crate::shell::{FrameOutput, Request, Shell, ShellInput};

/// How the page's game starts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct WindowOptions {
    /// The scale mode (`video.scale`).
    pub(crate) scale: ScaleMode,
}

/// The least audio buffer on the web, in milliseconds of sound: the
/// callback runs on the page's main thread through Web Audio, so it gets a
/// generous buffer against a busy page (design, Risks).
const WEB_BUFFER_MS: u32 = 100;

/// The application: owns the shell, the input state, the pacer, the
/// canvas, the audio output and the gamepads.
#[derive(Debug)]
pub(crate) struct App {
    shell: Shell,
    input: InputState,
    pacer: Pacer<SystemClock>,
    audio: Audio<CpalBackend>,
    pads: Pads,
    poller: Option<GamepadPoller>,
    pad_events: Vec<PadEvent>,
    canvas: Option<GameCanvas>,
    options: WindowOptions,
    /// The bindings whose keys lose their browser action, shared with the
    /// canvas's key listener.
    guarded: Rc<RefCell<Bindings>>,
    page: WebPageUi,
    /// When the previous animation callback ran, for the silent advance of
    /// locked audio.
    last_callback: Option<Duration>,
    /// Game frames run so far.
    frames: u64,
}

impl App {
    /// The application around `shell`, before the canvas is attached.
    pub(crate) fn new(shell: Shell, options: WindowOptions) -> Self {
        let input = InputState::new(shell.bindings());
        let pacer = Pacer::new(SystemClock::new(), shell.frame_time());
        let pads = Pads::new(shell.settings().dead_zone);
        let settings = shell.settings();
        let rate = settings.sample_rate;
        let request = AudioRequest {
            rate,
            buffer: settings.buffer_size.max(rate * WEB_BUFFER_MS / 1000),
        };
        // Nothing is opened before a user gesture: `Locked` holds the
        // engine, and a quiet start has none until sound is switched on.
        let audio = if shell.sound_on() {
            Audio::locked(CpalBackend, request, sound_config(&shell))
        } else {
            Audio::off(CpalBackend, request)
        };
        let guarded = Rc::new(RefCell::new(shell.bindings()));
        Self {
            shell,
            input,
            pacer,
            audio,
            pads,
            poller: None,
            pad_events: Vec::new(),
            canvas: None,
            options,
            guarded,
            page: WebPageUi::new(),
            last_callback: None,
            frames: 0,
        }
    }

    /// Sets where the save-recording action hands the last game.
    pub(super) fn set_transfer(&mut self, transfer: impl crate::transfer::FileTransfer + 'static) {
        self.shell.set_transfer(transfer);
    }

    /// Shows a warning about the start-up options on the page.
    pub(crate) fn warn(&mut self, text: &str) {
        self.page.message(Level::Warning, text);
    }

    /// Runs one shell frame and applies its output.
    fn frame(&mut self, input: &ShellInput) {
        let FrameOutput {
            sound,
            controls,
            frame_time,
            requests,
            notices,
            ..
        } = self.shell.frame(input);
        for notice in &notices {
            self.page.set_notice(notice);
        }
        self.audio.check();
        let sink: &mut dyn SoundSink = self.audio.sink();
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
                Request::ToggleFullscreen => dom::toggle_fullscreen(),
                Request::RetryAudio => {
                    let config = sound_config(&self.shell);
                    self.audio.retry(config);
                }
                // The stay-on-title policy never asks to exit; a page is
                // never closed by the game.
                Request::Exit(_) => {}
                Request::BindingsChanged => {
                    let bindings = self.shell.bindings();
                    self.page.set_bindings(&bindings);
                    self.guarded.replace(bindings.clone());
                    self.input.set_bindings(bindings);
                }
            }
        }
        // The browser keeps the settings as they change.
        self.shell.save_changed_settings();
        for notice in self.shell.take_notices() {
            self.page.set_notice(&notice);
        }
        self.frames += 1;
        self.report_status();
    }

    /// Hands the status to the page and prints the audio's notices.
    fn report_status(&mut self) {
        let status = Status {
            phase: self.shell.phase(),
            audio: self.audio.status(),
            frames: self.frames,
            result: self.shell.playback_result().map(str::to_owned),
            can_save: self.shell.can_save(),
        };
        self.page.set_status(&status);
        for notice in self.audio.take_notices() {
            self.page.message(Level::Notice, &notice);
        }
    }

    /// A user gesture: locked audio is unlocked before the gesture is
    /// handled as input, and a context the browser held back is asked to
    /// play.
    /// Audio that is off (a quiet start) stays off: only the sound toggle
    /// switches it on.
    fn gesture(&mut self) {
        if self.audio.status() == AudioStatus::Locked {
            let config = sound_config(&self.shell);
            self.audio.unlock(config);
            self.report_status();
        }
        self.audio.nudge();
    }

    /// One animation callback.
    fn animation_frame(&mut self) {
        let now = self.pacer.clock().now();
        let resized = self.update_layout();
        self.poll_pads();
        if let Some(last) = self.last_callback.replace(now) {
            self.audio.advance(now.saturating_sub(last));
        }
        self.pacer.begin();
        while self.pacer.next_frame() {
            let input = ShellInput::from(self.input.take_frame());
            self.frame(&input);
        }
        let wake = self.pacer.end();
        if let Some(canvas) = self.canvas.as_mut() {
            if wake.redraw || resized {
                canvas.present(self.shell.picture());
            }
            canvas.request_redraw();
        }
    }

    /// Works the canvas size out again; returns whether it changed, and
    /// shows the new scale on the page.
    fn update_layout(&mut self) -> bool {
        let Some(canvas) = self.canvas.as_mut() else {
            return false;
        };
        let changed = canvas.update_layout();
        if changed {
            self.page.set_scale(canvas.scale());
        }
        changed
    }

    /// Feeds the pad events since the last callback into the input state.
    /// A button press is a gesture where the browser counts it as one.
    fn poll_pads(&mut self) {
        let Some(poller) = self.poller.as_mut() else {
            return;
        };
        poller.poll(&mut self.pad_events);
        let pressed = self.pad_events.iter().any(PadEvent::is_press);
        // Browsers do not always count a gamepad press as a user gesture;
        // where it is not one, the audio stays locked and the next gesture
        // tries again.
        if pressed && dom::user_activation().unwrap_or(true) {
            self.gesture();
        }
        for event in self.pad_events.drain(..) {
            self.input.pad_event(&mut self.pads, event);
        }
    }

    /// A key event.
    fn key(&mut self, event: &KeyEvent) {
        let key = key_name(event.physical_key);
        match event.state {
            ElementState::Pressed => {
                self.gesture();
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

    /// The page became visible or hidden (`web-build` spec, hidden tab
    /// pauses the game).
    fn visibility(&mut self, visible: bool) {
        if visible {
            self.shell.event(AppEvent::Visibility(true));
            self.pacer.show();
            self.audio.resume();
            // Hidden time is neither caught up nor advanced in silence.
            self.last_callback = None;
        } else if !self.pacer.is_hidden() {
            self.shell.event(AppEvent::Visibility(false));
            // During live play the game enters its pause at once, at the
            // frame where the page was hidden.
            if matches!(self.shell.phase(), Phase::Playing | Phase::Paused) {
                let input = ShellInput::from(self.input.take_frame());
                self.frame(&input);
            }
            self.pacer.hide();
            self.audio.suspend();
            // Keys held now are not seen released while hidden.
            self.input.focus_lost();
        }
        self.report_status();
    }
}

/// The engine set-up for opening the audio now: the switches as the shell
/// has them, the master volume, the output filter on.
fn sound_config(shell: &Shell) -> SoundConfig {
    SoundConfig {
        sound_on: shell.sound_on(),
        music_on: shell.music_on(),
        volume: Volume::new(shell.settings().volume),
        ..SoundConfig::DEFAULT
    }
}

impl ApplicationHandler<AppEvent> for App {
    fn user_event(&mut self, _: &ActiveEventLoop, event: AppEvent) {
        match event {
            AppEvent::Visibility(visible) => self.visibility(visible),
            event => {
                self.shell.event(event);
                for notice in self.shell.take_notices() {
                    self.page.set_notice(&notice);
                }
            }
        }
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.canvas.is_some() {
            return;
        }
        let opened = dom::canvas()
            .and_then(|canvas| GameCanvas::open(event_loop, canvas, self.options.scale));
        match opened {
            Ok(canvas) => {
                dom::guard_bound_keys(canvas.element(), Rc::clone(&self.guarded));
                canvas.request_redraw();
                self.page.set_scale(canvas.scale());
                self.canvas = Some(canvas);
                self.page.set_bindings(&self.shell.bindings());
                if self.shell.settings().pad_enabled {
                    match GamepadPoller::new() {
                        Ok(poller) => self.poller = Some(poller),
                        Err(reason) => {
                            self.page.message(
                                Level::Notice,
                                &format!("gamepads cannot be read: {reason}"),
                            );
                        }
                    }
                }
                if dom::hidden() {
                    self.visibility(false);
                }
                self.pacer.reanchor();
                self.report_status();
            }
            Err(reason) => self.page.fail(&reason),
        }
    }

    fn window_event(&mut self, _: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::Focused(false) => self.input.focus_lost(),
            WindowEvent::KeyboardInput { event, .. } => self.key(&event),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                ..
            } => self.gesture(),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input.set_alt(modifiers.state().alt_key());
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                if self.update_layout()
                    && let Some(canvas) = self.canvas.as_mut()
                {
                    canvas.present(self.shell.picture());
                }
            }
            WindowEvent::RedrawRequested => self.animation_frame(),
            _ => {}
        }
    }
}
