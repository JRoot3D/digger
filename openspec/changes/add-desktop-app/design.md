# Design

## Context

See `proposal.md` for the motivation. The state before this change:

- `digger-core` (phases 2–4) offers a deterministic `Game` advanced by `step(&FrameInput)`, a 640×400 indexed framebuffer with a 16-colour palette, public drawing primitives (text, sprites, title picture), recording writers and readers for DRF and eDRF, and a replay verifier.
- `digger-sound` (phase 5) turns `SoundCommand`s into PCM samples and is pure.
- `crates/digger` is a binary skeleton from phase 1.

Constraints from `docs/porting-plan.md`: no C and no SDL; window, picture, audio and pads come from `winit` 0.30, `softbuffer` 0.4, `cpal` 0.18 and `gilrs` 0.11; command line from `clap` 4; files through `serde` + `toml` and `directories`; `#![forbid(unsafe_code)]`; no global mutable state; no compatibility with upstream's settings or score files. Phase 7 builds the same crate for the browser, so everything that is not a device must be usable without one.

The research reports describe upstream as blocking loops (`getkey`, `flashywait`, the title loop, the pause wait) around global flags (`escape`, `pausef`, `start`, `savedrf`, `keyredef`, `mode_change`). None of that structure is kept.

## Goals / Non-Goals

**Goals:**

- A thin platform layer and a device-free shell, so that every rule in the four specs except "pixels reach the screen" and "samples reach the speaker" is covered by tests that run in CI without a display or sound card.
- One owner for each piece of state; the event loop never blocks.
- User-facing errors that name the file, key or option at fault, with stable exit statuses.

**Non-Goals:**

- No GPU presentation, shaders, CRT filters or smoothing.
- No in-game settings menu; settings change through keys, the file and the command line.
- No rebinding of pad buttons on screen (file only) and no remapping of d-pad or stick.
- No audio resampling in the frontend: the synthesiser is created at the device's rate.
- No localisation; all texts are upstream's English strings.

## Decisions

### D1. Module layout of `crates/digger`

```
src/
  main.rs            parse → load → run; maps AppError to ExitCode
  cli/
    mod.rs           clap derive struct `Cli`, conversion to `CliLayer`
    legacy.rs        alias pre-pass: upstream forms → long options
    levels.rs        level-file check and load (`.dlf` retry, 1202-byte minimum)
  config/
    mod.rs           `Settings` (resolved), `Layer` (all-Option), merge, validate
    file.rs          TOML read (table walk with key paths), write, atomic save
    schema.rs        the static schema table (path, type, range, default, comment)
    paths.rs         platform directories, `--config` / `--data-dir` overrides
  scores/
    mod.rs           `ScoreTable`, `TableKey`, qualify, insert
    store.rs         scores.toml schema v1, load with recovery, atomic save
  input/
    mod.rs           `Action`, `KeyName`, `PadButton`, `Bindings`
    state.rs         `InputState`: held/sticky flags per control, action and text queues
    gamepad.rs       pad slot assignment, stick → virtual direction keys
  shell/             device-free program logic (reused by the web build)
    mod.rs           `Shell`, `Scene`, `ShellInput`, `FrameOutput`
    title.rs         title scene, animation script, mode cycle
    play.rs          live game and playback scenes, pause and resume requests
    game_over.rs     end-of-game hand-over: `player_out` → initials entry → back to the core's notice
    initials.rs      new-high-score scene
    redefine.rs      key redefinition scene
    recording.rs     when and where recordings are saved, auto names
  pacing.rs          `Pacer` over an injected `Clock`
  present/
    scale.rs         `viewport()`, column/row maps (pure)
    blit.rs          indices + palette → XRGB rows (pure)
  platform/          the only modules that touch winit, softbuffer, cpal, gilrs
    app.rs           `ApplicationHandler` implementation, event translation
    window.rs        window attributes, icon, fullscreen, softbuffer surface
    audio.rs         cpal stream lifecycle
    pads.rs          gilrs polling
  headless.rs        `--verify --headless` runner (phase 4's `verify`)
  error.rs           `AppError` and its exit statuses (D12)
  report.rs          `digger: error|warning|notice:` lines on standard error (D12)
```

Dependency direction: `platform` → `shell`, `pacing`, `present`, `input`, `config`, `scores`; none of those depend on `platform`. Only `platform/*` and `config/paths.rs` are compiled out or replaced for the browser.

*Alternative considered:* a separate `digger-shell` library crate. Rejected for now: the porting plan fixes three crates, and a module boundary enforced by "no platform imports outside `platform/`" (checked by a test that greps `use winit|softbuffer|cpal|gilrs`) gives the same isolation. Phase 7 can promote the module to a crate if the browser build needs it.

### D2. The shell is a state machine driven one frame at a time

Upstream's nested blocking loops become one enum owned by `Shell`:

```rust
enum Scene {
    Title(TitleScene),                 // animation frame, selected mode
    Redefine(RedefineScene),           // cursor over the action list, pending bindings
    Playing(PlayScene),                // Game + recorder + pending pause request
    Playback(PlaybackScene),           // phase 4 `Replay` + verify flag + take-over tick
    Initials(InitialsScene),           // digger index, letters so far, closing frames; holds the suspended PlayScene
}
```

The platform calls `shell.frame(&ShellInput) -> FrameOutput` once per pacing deadline. `ShellInput` (named so as not to clash with the core's `FrameInput`, which the play scene builds from it) carries what the input layer collected since the last frame (per-slot controls, a list of `Action`s, typed letters, "any key" and "any button" flags, `close_requested`, `focus_lost`, pad connect/disconnect). `FrameOutput` carries a borrowed picture (`&[u8; 640*400]` indices plus palette), the sound commands of this frame, the frame time to use next, and requests to the platform: `ToggleFullscreen`, `RetryAudio`, `Exit(ExitCode)`.

Scene transitions are values returned by each scene's `frame` method (`Transition::Stay | To(Scene) | Exit(code)`), so no scene reaches into another and no flag outlives the frame that set it. The upstream latches map as follows:

| Upstream global | Here |
|---|---|
| `escape` | `Action::Quit` in the frame's action list; each scene decides what it means |
| `pausef` + blocking `getkey` | the play scene sets `FrameInput::pause`; the core's own pause phase holds the game (frames report `Paused`), and any key or button sets `pause` again to resume |
| `start` | computed per frame by `TitleScene` from "any key" minus excluded actions |
| `mode_change`, `keyredef`, `savedrf` | `Action::ChangeMode`, `RedefineKeys`, `SaveRecording` |
| `gotgame`, `gotname` | `Shell.last_game: Option<FinishedRecording>`, `Settings.record_path: Option<PathBuf>` |
| `soundflag`, `musicflag`, `quiet` | `Shell.sound_on`, `Shell.music_on`; the device is separate state in `platform::audio` |

**The shell draws nothing over a running game.** The simulation's framebuffer is game state (a fireball reads pixels back), so every text shown during play comes from the core itself, as in the original: `PRESS ANY KEY` while paused, with scores and lives restored on resume, and `GAME OVER` / `TIME UP` for 50 frames at the end (`add-simulation-core`, game-flow). The shell only presents the core's picture. A pause request is honoured by the core on a main-phase tick only, so the play scene keeps `pause` set on every frame until a frame reports `Paused` (this matters for focus loss during a level draw).

Title, initials and redefinition draw into a shell-owned screen with the core's public text and sprite primitives. The title animation is a data table (`&[ScriptEntry]`, frames 0–286) interpreted by `TitleScene`; it uses core sprite drawing but no simulation objects.

*Alternative considered:* letting `digger-core` own title and initials as further phases of `Game`. Rejected: those screens depend on the score tables, text input and files, which the core must not know about, and they consume no ticks, so keeping them outside costs nothing in replay fidelity.

### D3. Interfaces assumed from earlier changes

These are the points where this design meets phases 2–5. Task group 1 checks each against the code as merged and records any difference in this file before further work.

1. `Game::step(&FrameInput) -> StepOutcome`. `FrameInput { slots: [Controls; 2], pause, quit }` carries, per input slot, five booleans (up, down, left, right, fire) meaning "held now or pressed since the previous frame", plus the pause and quit requests. The direction-resolution state (rising edges, one-tick pulse, release fallback) and the one-tick fire delay live inside the core, because eDRF recordings store exactly those raw booleans and replay must run through the same logic. The frontend supplies the booleans; the `input-controls` requirements on resolution are then proven end to end by tests in this change.
2. `StepOutcome { frame, player_out, finished }` reports the kind of frame (`Tick`, `Wait`, `Paused`), a "player n is out" result on the frame a player's game ends (each digger's displayed six-digit score and full total), and, once the whole game is over, the summary (reason, scores, level, tick count). There is no pacing hint: every frame is one full frame time (the core reveals three level-draw items per frame).
3. The core **does** show `GAME OVER` (104, 0) and `TIME UP` (120, 0) itself, for 50 waiting frames, in every game. It does not know about score tables: initials entry is the shell's job and hangs off `player_out` (D2, `Initials`). On the frame that reports `player_out` the shell checks eligibility and qualification; if an entry is due it suspends the play scene, runs `InitialsScene` once per qualifying digger, then resumes stepping, and the core's 50-frame notice follows. This differs from upstream only in that `GAME OVER` is also shown after an initials entry (a presentation deviation recorded by phase 3).
4. The core exposes drawing on a free-standing screen: clear, title picture, opaque text at (x, y) in a colour, erase text cells, draw and erase a sprite frame by id, palette selection.
5. Recording and playback are phase 4's library: `DrfRecorder`/`EdrfRecorder` attached to a live game through the observer seam, with `take_pending()` and the initials line; `Replay::new(recording, ReplayOptions)`, `Replay::step(input, extra)`, `Replay::finish() -> ReplayReport` with `Outcome { Verified, Completed, Failed, Stopped, Truncated, TakenOver }`, `result_line()` and `exit_status()` (0–4); `verify(bytes, options)` for headless runs. `ReplayOptions::take_over_after` implements `--take-over-at`.
6. `digger-sound` exposes `sound_link(config, tables) -> (SoundHandle, AudioSource)`: the handle takes each frame's `SoundCommand`s (including the simulation's `Pause`/`Resume`) and the frontend's `SoundControl`s (`ToggleSound`, `ToggleMusic`, `SetVolume`, `SetFilter`), reports `status()`, and `AudioSource::fill(&mut [i16])` runs in the device callback. `NullDriver` stands in when no device is open. Master volume is the engine's own.

**As merged (checked in task 1.1).** Points 1–6 hold in substance; the real names, signatures and the differences that change how this design uses them are:

1. `digger_core::Game::new(GameConfig, base_seed: u64) -> Box<Game>`; the frontend chooses the base seed (a live game draws one from the clock in `main`/`platform`, tests pass a fixed one). `Game::step(&FrameInput) -> StepOutcome` and `Game::step_observed(&FrameInput, &mut dyn Observer) -> StepOutcome`; the frame's sound commands are taken separately with `Game::drain_sound()` (an iterator, queue of 64), the picture with `Game::framebuffer() -> &video::Framebuffer`, the state with `Game::state() -> StateView`. `FrameInput { slots: [Controls; 2], pause, quit }` and `Controls { up, down, left, right, fire }` are as assumed, with `FrameInput::NONE` and `Controls::NONE`. *Consequence:* the play scene drains sound after each step and owns the boxed game; the recorder is passed to `step_observed`.
2. `StepOutcome { frame: FrameKind, player_out: Option<PlayerResult>, finished: Option<GameSummary> }`, `FrameKind { Tick, Wait, Paused }`. **Difference:** `PlayerResult { player: PlayerId, score: i32 }` carries only the out player's visible score, and in a two-digger game it names the one player (both diggers end together) with digger 1's score. `GameSummary { reason: FinishReason { AllPlayersOut, TimeUp, Quit }, scores: [ScoreSummary { visible, total }; 2], level: LevelNumber, ticks: u32 }` is set from the finishing frame on and repeated on every later step (`Game::summary()` returns it too). *Consequence:* `shell::game_over` reads each digger's visible score from `game.state().scores().visible(ScoreId)` on the `player_out` frame, rather than from the outcome, to decide qualification for the second digger.
3. As assumed: `GAME OVER` (104, 0) and `TIME UP` (120, 0) in text colour 3 for 50 waiting frames; `PRESS ANY KEY` at (80, 0) in colour 1 while paused. **Detail:** the notice is already drawn into the framebuffer on the frame that reports `player_out`, so the 50 notice frames follow after the shell resumes stepping, and the picture under a suspended play scene already shows the notice.
4. The free-standing screen is `video::Framebuffer` itself (`Framebuffer::new()`, `clear()`, `pixels() -> &[u8; 256000]`, `crc32()`): `draw_title(&assets::TitlePicture)` (the picture from `TitlePicture::decode() -> Result<_, AssetError>`), `draw_text(&str, Px, TextColor)`, `draw_char`, `draw_number`, `erase_text(cells, Px)`, `blit_masked(ImageId, Px)` for a sprite frame, `set_palette(Palette { Zero, One })` and `set_intensity(Intensity { Normal, High })`; the 16 colours of a palette come from `video::colors(Palette, Intensity) -> &[Rgb; 16]`. **Difference:** there is no "erase sprite" primitive on the framebuffer: erasing is `capture(Px, ImageSize) -> SavedRect` before drawing and `restore(Px, &SavedRect)` after, or the `sprite::SpriteLayer` (`reset`, `place`, `move_to`, `erase`), which keeps the saved backgrounds. *Consequence:* the title animation drives a `SpriteLayer` over its own `Framebuffer`.
5. `replay::{DrfRecorder, EdrfRecorder, Recorder}` all implement `Observer`; `Recorder::new(RecordingKind, &GameConfig, drf_playback)` picks the format and `Recorder::take_pending() -> String` (eDRF: lines since the last call; DRF: the whole text so far). **Difference:** the initials line exists only in DRF (`DrfRecorder::add_initials(&str)`, reached through `Recorder::Drf`); eDRF has none, as the game-recording spec says. Playback: `Replay::new(&Recording, ReplayOptions)`, or `Replay::recording_as(&Recording, ReplayOptions, RecordingKind)` to re-record (`recorder_mut()`, `finish_with_recording() -> (ReplayReport, Option<String>)`), with `recording::parse(&[u8]) -> Result<Recording, RecordingError>`. **Difference:** `Replay::step(&FrameInput, &mut dyn Observer) -> ReplayStep { Running(FrameKind), Finished(GameSummary) }`, not a `StepOutcome`, so a playback reports no `player_out` (it never enters scores, `ReplayReport::high_score_eligible()` is `false`); the picture and sound come from `Replay::game()` and `Replay::drain_sound()`. `ReplayOptions { stop_at_first_failure, max_ticks, take_over_after }`; `request_take_over()` and `taken_over()` report the hand-over. `Replay::finish() -> ReplayReport { kind, outcome: Outcome, scores, two_scores, level, ticks, last_good_checkpoint }` with `result_line()`, `diagnostics(file)` and `exit_status() -> u8` (0 also for `TakenOver`); a file that cannot be parsed gives its status through `RecordingError::exit_status()` (1, or 2 for a bad record). `replay::verify(&[u8], ReplayOptions) -> Result<ReplayReport, RecordingError>` is the headless run. *Consequence:* `AppError::Verify` carries the core's `u8` status (and a message only for status 1) rather than an enum of its own.
6. `digger_sound::sound_link(SoundConfig, &TuneTables) -> Result<(SoundHandle, AudioSource), ConfigError>`, tables from `TuneTables::embedded()`. **Difference:** commands and controls go through the `SoundSink` trait (`send(SoundCommand)`, `control(SoundControl)`), which `SoundHandle`, `NullDriver` and `ManualDriver` all implement; `SoundControl { ToggleSound, ToggleMusic, SetVolume(Volume), SetFilter(bool) }`; `SoundHandle::status() -> SoundStatus { sound_on, music_on, dropped }`, `poll_event()`. `SoundConfig { sample_rate: SampleRate, sound_on, music_on, volume: Volume, filter }`: the rate is fixed per engine, so a device reopened at another rate needs a new `sound_link`. `AudioSource::fill(&mut [i16])` as assumed. *Consequence:* the shell writes sound to a `&mut dyn SoundSink`, so tests use `ManualDriver` or `NullDriver` and the platform its `SoundHandle`.

### D4. winit application handler and frame pacing

`platform::app::App` implements `ApplicationHandler`:

- `resumed`: create the window and softbuffer surface (required order on Wayland and for the later browser build), open audio, start the pacer.
- `window_event`: translate `KeyboardInput` (physical key → `KeyName`, ignoring `repeat`; `text` → letters for initials), `ModifiersChanged` (Alt state for the Alt+Enter chord, which is consumed here and never reaches `InputState`), `Focused(false)`, `CloseRequested`, `Resized`/`ScaleFactorChanged` (recompute the viewport), `RedrawRequested` (present).
- `about_to_wait`: drain gilrs events; ask the pacer how many frames are due; for each, build `ShellInput`, call `shell.frame`, forward sound commands, apply platform requests; `request_redraw()` if a frame ran; finally `event_loop.set_control_flow(ControlFlow::WaitUntil(pacer.next_deadline()))`.

The event loop therefore sleeps in the operating system between frames and wakes for input at once; nothing blocks inside a callback.

`Pacer` is a pure struct over a `Clock` trait (`fn now(&self) -> Duration`; real implementation `Instant`, tests use a settable fake):

```
anchor: Duration, frames_since_anchor: u64, frame_time: Duration
deadline(k) = anchor + frame_time * k          // absolute: no accumulated drift
due(now)    = number of deadlines passed, capped at 3
if now - deadline(frames_since_anchor) > 3 * frame_time  → re-anchor at now
set_frame_time(t) → re-anchor at the current deadline
```

Re-anchoring also happens on pause end, on focus return and after a scene change that alters the frame length. Upstream's phase-locked loop (research 05 §6.2) is not reproduced; research 05 §6.3 states the timer is not part of the game's behaviour.

Unpaced mode (speed 0): `ControlFlow::Poll`; each `about_to_wait` runs frames for a budget of 8 ms, then returns to the event loop; a redraw is requested when 16 ms have passed since the last one. Paced presentation is always once per frame.

*Alternatives considered:* a dedicated game thread with a sleep loop (needs `Game` behind a mutex or a channel of pictures, and macOS requires the window on the main thread anyway); pacing from the audio callback's sample count (precise, but ties the game to a device that may not exist).

### D5. Presentation with softbuffer

`present::scale::viewport(win_w, win_h, mode) -> Rect` is pure:

- `integer`: `k = min(win_w / 640, win_h / 400)`; if `k == 0` fall through to `fit`; else `w = 640k`, `h = 400k`.
- `fit`: if `win_w * 400 <= win_h * 640` then `w = win_w`, `h = max(1, win_w * 400 / 640)`, else `h = win_h`, `w = max(1, win_h * 640 / 400)`.
- `stretch`: `w = win_w`, `h = win_h`.
- Offsets `x = (win_w - w) / 2`, `y = (win_h - h) / 2`. All in physical pixels, integer arithmetic.

On resize, `scale.rs` also builds `col_map: Vec<u16>` (`sx = dx * 640 / w`) and `row_map: Vec<u16>`. Presenting a frame: resize the softbuffer surface to the window's physical size, fill with 0 (black), and for each destination row either expand the source row through a 16-entry `[u32; 16]` palette table using `col_map` or copy the previous destination row when `row_map` repeats. Pixel format is softbuffer's `0x00RRGGBB`. At 4K this is about 8 million writes 12.5 times a second, far below one core.

HiDPI: sizes are physical pixels throughout; the default window is requested as `PhysicalSize(1280, 800)` with minimum `PhysicalSize(640, 400)`. Fullscreen is `Fullscreen::Borderless(None)`; the cursor is hidden while fullscreen. The icon is the 64×64 RGBA asset from phase 2 (`assets/icon.rgba`), set through winit where the platform supports it (Windows, X11); Wayland and macOS take icons from packaging (phase 7).

### D6. Audio with cpal

`platform::audio::Audio` is an enum: `Closed { reason }`, `Open { stream, handle, rate }`.

- Opening: default host and output device; look for a supported configuration with the configured rate, preferring one channel, else the fewest channels; request the configured buffer size as `BufferSize::Fixed`, falling back to `Default` on error; if the rate is unsupported use the device default configuration. Each fallback prints one `digger: notice:` line.
- `sound_link` is called for the rate actually obtained and its `AudioSource` is **moved into the stream callback**, which owns it exclusively. Per callback: `fill` a mono `i16` buffer (the engine drains its command queue and applies the volume itself), convert to the device sample format (`i16`, `u16` or `f32`), write the same value to every channel. No locks, no allocation, no I/O in the callback.
- The shell's sound commands of each frame are pushed to the `SoundHandle` by the platform; when `Audio` is `Closed` they go to a `NullDriver`. Pause and resume need nothing extra: the core emits those commands itself. Scene changes that upstream silenced (`soundstop`, `killsound`) are explicit commands from the scene.
- The stream's error callback sets an `AtomicBool`; the next frame turns `Open` into `Closed`, prints a notice, and the game carries on.
- `--quiet` and `sound.enabled = false` only set the engine's initial sound switch to off; the device is still opened if present, so the toggle works at once (this is the fix for upstream's "cannot re-enable after `/Q`"). When `Audio` is `Closed`, toggling sound on returns `RetryAudio`, and the platform tries to open again.
- The stream is created, kept and dropped on the main thread (cpal streams are not `Send` on every host). Dropping it on exit stops the device.

### D7. Input mapping layer

```
winit key events ─┐
gilrs events ─────┼─► InputState ──(once per frame)──► ShellInput ─► Shell
typed text ───────┘
```

- `Bindings` maps `KeyName → Action` (a `HashMap` built from the resolved settings; uniqueness is validated in `config`) and `PadButton → Action`.
- `InputState` holds, for each of the ten player controls, `held: bool` and `pressed_since_frame: bool`. A key-down sets both; a key-up clears `held` only. `take_frame()` returns `held || pressed_since_frame` for each control and clears the sticky flags. That is the "sticky flag or asynchronous state" sampling of research 05 §1.5–1.6 without the 30-entry FIFO.
- Function actions are edge-triggered: a key-down that is not a repeat pushes its `Action` once into a small queue drained by `take_frame()`. This removes upstream's "action repeats for every further key drained" defect by construction.
- `Focused(false)` and pad disconnection clear all `held` flags and push `FocusLost` / `PadRemoved` so the play scene can pause.
- Pads: `gamepad.rs` keeps `slots: [Option<GamepadId>; 2]`. D-pad buttons and the left stick feed the same ten controls as the keyboard of that slot. The stick is turned into four virtual buttons with hysteresis: pressed above `dead_zone`, released below `0.8 * dead_zone`, so a stick resting on the threshold does not chatter. Diagonals press two virtual buttons; the order in which they crossed the threshold decides, by the same resolution rules.
- In alternating two-player mode the core reads slot 0 for both players; the frontend does nothing special.
- Text for initials comes from the key event's text (layout-aware), filtered to ASCII letters and upper-cased in `InitialsScene`; physical key names are not used there, which is what removes the arrow-keys-type-letters defect.

### D8. Command line: legacy pre-pass in front of clap

`cli::legacy::translate(args) -> Result<Vec<OsString>, CliError>` runs before clap and rewrites only tokens that match the upstream grammar; everything else passes through untouched.

- A token is a legacy option if it is `/L` or `-L` followed by nothing, by `:value`, or (numeric options only) directly by digits, where `L` is one of `S I G 2 U Q M F L R P E O K V ? H C N X T` in either case (`/T:n` becomes `--take-over-at n`). `/KA` is the one two-letter form. `-h` and `-V` are left to clap (help and version); `/V` and `/v` are dropped (upstream's no-op).
- File-valued options (`L R P E`) require the colon. This keeps Unix absolute paths such as `/Library/x.dlf` or `/Users/...` from being read as `/L` or `/U`.
- `C N X` produce `CliError::Unsupported { option, reason }` immediately.
- Each rewritten token remembers its original spelling in a side table so that range and conflict errors can quote what the user typed (`/I:abc`, not `--start-level`).
- Tokens after `--` are not translated.

clap then parses a derive struct with long options only (plus `-h`, `-V`), one optional positional (`all digits → speed`, otherwise level file) and value parsers with ranges. clap's own exit code 2 is not used: errors are rendered with clap's formatter but returned as `AppError::Usage` (status 64), because status 2 means "diverged" in verification runs.

Details settled in task group 3 (no spec conflict; each is a reading of the `configuration` spec):

- The combinations are checked after clap, in `cli::convert`, not with clap's `conflicts_with` / `requires`: the first one depends on a value (`--players 2`, not `--players 1`), and every message must quote each option as typed, which the side table gives for both sides of a pair. In a clap value error (range, not a number, missing value) the long option is replaced by the alias that carried the bad value.
- A bare speed together with `--speed`, or a bare level file together with `--levels`, is refused (status 64) rather than one silently winning; a second bare argument is an unexpected argument. A repeated option takes its last value (`/Q /Q` is not an error).
- An upstream flag with a value (`/Q:x`, `/K:5`) is refused with status 64; `/V`, `/v` and `-v` are dropped; `-H` and `/h` are help like `/H`.
- A level file that does not exist is retried as `<name>.dlf` only when the name has no extension; the error names the file as given.

### D9. Settings: layers, validation, saving

- `Layer` is a struct of `Option`s mirroring every setting. Three layers are built: defaults (all `Some`), file, command line. `Settings::resolve(defaults, file, cli)` takes the last `Some` per field; the mode is taken from the command line as a whole when any mode option is present.
- The file is parsed into a `toml::Table` and walked against a static schema table (`path`, type, range, default). Walking by hand rather than deriving gives errors that carry the full key path and the offending value, lets unknown keys be warnings, and keeps range rules in one place that the default-file writer and the manual's table are generated from (a test compares `docs/manual.md`'s settings table with the schema).
- `GameMode` is a five-variant enum; "two players and two diggers" cannot be represented after resolution. This is the configuration-time rejection the porting plan asks for in place of upstream's out-of-bounds indexing.
- Saving: `Shell` keeps a `Layer` of runtime changes (only fields the player changed with actions). On save, the file layer is re-read (or defaults if absent), the runtime layer is applied on top, and the complete file is written with a one-line header comment. Writes go to `config.toml.tmp` in the same directory followed by a rename. Comments a user added by hand are lost on save; the manual says so.

Details settled in task group 2 (no spec conflict; each is a reading of the `configuration` and `input-controls` specs):

- A key bound to two actions is checked on the file's bindings *together with the defaults of the actions the file leaves out*: `p1_fire = ["Space"]` alone is refused, because `pause` keeps its default `Space`; the message names the key, both actions and the key path of the action the file set.
- The same rule applies to the `[gamepad]` button lists (one button, one pad action). A key or button listed twice under one action, and an empty `[keys]` list ("one or more key names"), are refused; an empty button list is allowed (`quit = []` is the default).
- Key and button names are matched exactly, then ignoring letter case (`space` reads as `Space`); the file is always written with the canonical spelling.
- The directional pad is not a bindable button name (non-goal: no d-pad remapping); bindable names are `South East North West C Z LeftTrigger LeftTrigger2 RightTrigger RightTrigger2 Select Start Mode LeftThumb RightThumb`.
- A whole number is accepted for `gamepad.dead_zone`; a fraction is refused for an integer setting.
- A save whose complete text equals the file already on disk writes nothing; a file that has become invalid since start is left untouched and the save is reported as a notice.
- With no home directory and no `--config` / `--data-dir`, the program stops with status 78 naming both options.

Config file schema (all keys, defaults shown):

```toml
[game]
speed = 40                  # 0–500; frame time = speed × 2000 µs; 0 = unpaced
mode = "one-player"         # one-player | two-players | two-diggers | gauntlet | two-digger-gauntlet
gauntlet_time = 120         # seconds, 1–3599
start_level = 1             # 1–1000
unlimited_lives = false

[sound]
enabled = true
music = true
sample_rate = 44100         # 8000–192000
buffer_size = 512           # frames, 64–8192
volume = 100                # percent

[video]
fullscreen = false
scale = "integer"           # integer | fit | stretch
reduce_flashing = false

[keys]
p1_right = ["ArrowRight"]
p1_up = ["ArrowUp"]
p1_left = ["ArrowLeft"]
p1_down = ["ArrowDown"]
p1_fire = ["F1"]
p2_right = ["KeyS"]
p2_up = ["KeyW"]
p2_left = ["KeyA"]
p2_down = ["KeyZ"]
p2_fire = ["Tab"]
speed_up = ["NumpadAdd", "Equal"]
speed_down = ["NumpadSubtract", "Minus"]
toggle_music = ["F7"]
toggle_sound = ["F9"]
quit = ["F10"]
pause = ["Space"]
change_mode = ["KeyN"]
save_recording = ["F8"]
redefine_keys = ["KeyK"]
fullscreen = ["F11"]

[gamepad]
enabled = true
dead_zone = 0.5
fire = ["South", "West"]
pause = ["Start"]
change_mode = ["Select"]
quit = []
```

### D10. Score tables and score file

- `ScoreTable` is `[Entry; 10]` with `Entry { initials: [u8; 3], score: u32 }`. `qualifies(score) = score > entries[9].score`. `insert` finds the first index whose score is `<=` the new score, shifts the rest down and drops the last; this reproduces upstream's "ties go above" without its 1-based, off-by-one arrays.
- `TableKey { levels: LevelSet, kind: TableKind }` with `LevelSet::Builtin | Custom(String)` and `TableKind::{Normal1, Gauntlet1, Normal2, Gauntlet2}` (the file's `kind` names; `gauntlet()` and `two_diggers()` read it back), so the four tables are one enum rather than two flags; the game mode maps to a key in one function (`two-players` → one-Digger normal table). The custom set is named by the level file's name without directory (`hard.dlf`). `Scores` holds the tables read or written (a table it does not hold is fresh); `ScoreStore` owns `Scores`, the file path and whether the old file must still be kept as `.bad`.
- File schema, version 1:

```toml
format = 1

[[table]]
levels = "builtin"          # or the level file's name, e.g. "hard.dlf"
kind = "normal-1"           # normal-1 | gauntlet-1 | normal-2 | gauntlet-2
entries = [
  { initials = "MSX", score = 27700 },
  # … exactly ten entries, highest first
]
```

- Loading returns `(Scores, LoadStatus)` where `LoadStatus` is `Fresh | Loaded | Recovered { reason }`. A table with the wrong entry count, non-letter initials other than `...`, a score above 999999 or unsorted entries makes the whole file `Recovered`. `Recovered` remembers that the old file must be renamed to `scores.toml.bad` before the next save.
- Saving writes `scores.toml.tmp` and renames it over `scores.toml`. Failure is a notice, not an error. A file that could not be read for a reason other than its absence (permissions, a directory in its place) is treated like an unparsable one: fresh tables, one notice, renamed to `.bad` before the next save. The `format` key is checked before the rest of the file, so a newer format is reported as such even when its structure is unknown; unknown keys in a format 1 file are a wrong structure. Each table may appear at most once per level set.

### D11. Recordings in the shell

- `PlayScene` owns the recorder for a live game. When the game ends (any way), the finished bytes and metadata (initials if entered, score, format) become `Shell.last_game`.
- With `--record`, `last_game` is written to that path at once and cleared. Otherwise `Action::SaveRecording` on the title writes it to `<data dir>/recordings/<initials><score>.drf` and clears it. Initials are those just entered, else `___`; if the name exists, `-2`, `-3`, … is appended before the extension. Upstream's 8.3 squeezing and its current-directory target are dropped.
- `--verify`: `PlaybackScene` with `verify = true`; on end the scene returns `Exit(code)` with phase 4's `ReplayReport::exit_status()` (0 verified or completed, 1 bad file, 2 failed, 3 stopped, 4 truncated) and, for 0, `ReplayReport::result_line()` is printed by `main` just before returning. `headless.rs` calls phase 4's `verify` without creating an event loop.
- `--take-over-at <tick>` (alias `/T:n`), valid with `--play` only: the scene passes the tick as `ReplayOptions::take_over_after`. After that tick the play controls are live, the game continues as an ordinary game that is never eligible for a high score, and a recording made with `--record` covers both parts.

### D12. Errors and messages

```rust
enum AppError {
    Usage(String),                 // 64
    LevelFile { path, source },    // 66
    NoDisplay(String),             // 69
    Config { file, key, problem }, // 78
    Verify { status, message },    // 1–4 (`ReplayReport::exit_status`), message only for 1
}
```

`main` returns `ExitCode`; it prints `digger: error: <message>` for errors, and the modules print `digger: notice: …` or `digger: warning: …` through one small `report` module so wording and prefixes stay uniform (and can be redirected to the browser console in phase 7). Nothing derived from user input is unwrapped. Conditions that must not stop the game (no audio device, unwritable score file, unreadable playback file in a `--play` list) are notices.

Windows: the binary is built for the console subsystem in this change, so `--help`, notices and the verification line work from a terminal and in scripts. The cost is a console window when started by double-click; phase 7 decides whether release builds switch to the GUI subsystem.

### D13. How the design stays idiomatic Rust

- **Ownership instead of globals.** `App` owns the window, surface, audio and pads; `Shell` owns the scene, settings, score tables and the last recording; `PlayScene` owns its `Game`. The audio callback owns the synthesiser outright. The only shared state is one command queue and one `AtomicBool`.
- **Enums and state machines instead of flag integers and blocking loops.** `Scene`, `Transition`, `Action`, `GameMode`, `ScaleMode`, `Audio`, `LoadStatus`; every `match` is exhaustive, so adding a scene or action forces each handler to consider it.
- **Types for units.** `FrameTime(Duration)` versus `Speed(u16)`; `KeyName` and `PadButton` newtypes that only exist for valid names; physical-pixel `Rect`.
- **`Result` and `ExitCode` instead of `exit()` inside callbacks.** Upstream calls `exit(0)` from its event filter; here a close request is data in `ShellInput` and the single exit path runs the saves.
- **Injected time and files.** `Clock` for pacing; score and config stores take a directory path, so tests use temporary directories; no test reads the real user profile.

### D14. Compatibility quirks reproduced, and upstream defects fixed

Reproduced (each named in code, listed in `docs/compat-quirks.md`, with the test that pins it):

| Quirk | Where isolated | Test |
|---|---|---|
| Same-frame presses resolve right > left > down > up; release fallback in the same order | core resolver (D3.1), fed by `InputState` | `input_resolution_same_frame`, `input_resolution_release_fallback` |
| A tap shorter than a frame moves for exactly one tick | `InputState` sticky flags + core pulse | `input_tap_one_tick` |
| Fire sampled on tick N acts on tick N+1 | core fire latch | `fire_takes_effect_next_tick` |
| Alternating players both use player-1 controls | core slot mapping | `alternating_uses_p1_controls` |
| Speed keys step 10000 µs; no decrease at or below 10000 µs | `shell::play` | `speed_floor` |
| Pause consumes no ticks (gauntlet clock stops) | core pause phase, requested through `FrameInput::pause` | `pause_consumes_no_ticks` |
| High-score test is strict `>` tenth place on the wrapped six-digit score; ties insert above | `scores::ScoreTable` | `qualify_strict`, `insert_tie_above` |
| Alternating two-player games use the one-Digger table | `TableKey::from_mode` | `table_key_mapping` |
| Game-over order: entry per qualifying digger on `player_out`, then the core's `GAME OVER` / `TIME UP` notice for 50 frames (always shown; phase 3 deviation) | `shell::game_over` | `game_over_sequences` |
| Title cycle of 287 frames with the documented key frames | `shell::title` script table | `title_script_keyframes`, golden frame CRCs |
| Verification statuses 0–4 and the result line format | phase 4's `ReplayReport`, surfaced by `main` | `tests/verify_cli.rs` |

Fixed, as the porting plan's "presentation or usability bug" class (each also listed in `docs/compat-quirks.md` as a deliberate difference): sound can be switched on after a quiet start; arrow keys do not type letters in initials entry and only letters are accepted; the volume setting works; function actions fire once per press; aborting key redefinition restores the old keys; closing the window saves instead of calling `exit`; a bad level file or invalid mode combination is an error instead of being ignored or indexing out of bounds; the result line no longer needs an environment variable.

### D15. Testing: automated versus manual

Automated, no window or device (CI on all three operating systems):

- `present::scale` — viewport tables for the spec's sizes, odd sizes, 1×1; column-map monotonicity.
- `present::blit` — a 640×400 test pattern scaled 1×, 2×, non-integer; palette expansion.
- `pacing` — fake clock: no drift over 7500 frames with jitter, stall re-anchor, speed change, unpaced budget.
- `input` — binding lookup, sticky/held sampling, once-per-press actions, repeat suppression, focus loss, stick hysteresis and dead zone, pad slot assignment and removal.
- `shell` — scripted `ShellInput` sequences against every scene: title start/exclusions, mode cycle, pause and resume, focus-loss pause, quit paths, game-over sequences, initials entry (keyboard and pad), redefinition including conflicts and abort; golden CRCs of the title frame and the initials screen.
- `cli` — table tests for every alias, absolute-path cases, unsupported options, conflicts, ranges.
- `config` — defaults, each key's range, unknown keys, precedence, save rules, round-trip of the default file.
- `scores` — qualify/insert, mapping, load/recover/rename, atomic save.
- Integration tests of the built binary: `--help`, `--version`, usage errors, config errors, `--verify --headless` against small upstream recordings for statuses 0, 1, 2 and 4.

Manual, from `docs/testing/desktop-checklist.md`, once per platform (Linux X11, Linux Wayland, Windows, macOS) before the change is archived: window title, icon, resize and letterboxing; Alt+Enter and F11; pointer hidden in fullscreen; HiDPI display; pacing by stopwatch (750 frames per minute shown by a debug counter); sound on default device, with no device, device removed while playing, F9 after `--quiet`; one and two gamepads, hot-plug, stick drift; close button in each scene; files created in the documented directories; `--verify` status 3 by pressing F10; `--play <file> --take-over-at 200` hands the keys over after tick 200.

### D16. Alternatives rejected for the platform layer

- **SDL (rust-sdl2 or sdl3-rs)** — what research 07 recommended, mainly for PortMaster handhelds. Rejected by the porting plan: it links a C library, and PortMaster is a non-goal.
- **`pixels` / wgpu** — needs a Vulkan/GL-class GPU and a large dependency tree to show a 640×400 indexed picture; fails on machines with broken or absent GPU drivers (virtual machines, remote desktops) where a CPU blit always works.
- **`minifb`** — window, input and framebuffer in one crate, but no control over the event loop for deadline pacing, limited fullscreen and HiDPI handling, and it pulls C code on some platforms.
- **`macroquad`** — brings its own GL renderer and main loop; its audio is sample playback, not a synthesis callback, so the sound engine would not fit; it would also dictate the application structure the shell is meant to own.
- **`rodio`** on top of cpal — a mixer this program does not need; the synthesiser already produces the final signal.

## Risks / Trade-offs

- [The interfaces assumed in D3 differ from what phases 3–5 delivered] → Task group 1 reconciles them first and updates this file; the shell depends on them only through `shell::play` and `shell::recording`.
- [softbuffer or winit misbehaves on one compositor or driver] → The platform layer is about four small modules; the manual checklist runs on X11 and Wayland separately; `--scale` and `--fullscreen` give users work-arounds.
- [Timer granularity on Windows makes `WaitUntil` wake late] → Deadlines are absolute, so lateness does not accumulate; the checklist measures the frame rate; if needed the pacer can spin for the last millisecond.
- [Audio underruns with a 512-frame buffer on some hosts] → The buffer size is a setting; the device default is used when the request is refused.
- [gilrs reports odd mappings for unknown pads] → Button names are configurable; keyboard always works; `gamepad.enabled = false` turns pads off.
- [Saving the configuration file drops hand-written comments] → Saves happen only after an explicit player action; the manual documents it.
- [A full-screen palette flash at 12.5 Hz can be uncomfortable] → `video.reduce_flashing`; flashing is limited to the initials screen in this change.
- [Console subsystem on Windows shows a console window] → Accepted here; phase 7 owns release packaging.
- [The platform crates bring `cc` and an unmaintained crate into the graph] → Found in task 1.2: `wayland-backend` (under winit and softbuffer on Linux) build-depends on `cc` but compiles C only with its `log` feature, so `deny.toml` allows `cc` solely as a dependency of `wayland-backend` and bans that feature with `[[bans.features]]`; `ttf-parser` (RUSTSEC-2026-0192, unmaintained, no vulnerability) arrives through winit's default `wayland-csd-adwaita` title-bar text and is listed in `advisories.ignore` with its reason. Linux builds need the ALSA and udev development packages (pkg-config only, no C compiled).
- [Exit statuses 64/66/69/78 differ from upstream's blanket 1] → Needed to keep 1–4 unambiguous for verification scripts; documented in the manual.

## Open Questions

- Should release builds on Windows use the GUI subsystem? Decided in `add-web-build-and-releases`; it changes no requirement here.
- None beyond the Windows subsystem question above; the core gives no sub-frame pacing hint (D3.2).
