# Proposal

## Why

After phases 1 to 5 the port has a deterministic simulation, a renderer, a recording and verification library and a sound engine, but nothing a person can play: no window, no keyboard, no audio device, no title screen, no saved scores. This change wires those libraries to real devices and delivers the playable `digger` program for Linux (X11 and Wayland), Windows and macOS.

## What Changes

- Add the game binary `digger`: a resizable window titled `D I G G E R` that shows the 640×400 picture with integer scaling and letterboxing, a fullscreen toggle (Alt+Enter, F11) and a start-fullscreen option.
- Add frame pacing on absolute deadlines at the configured frame time (default 80000 µs, speed n = n × 2000 µs), with runtime speed keys and an unpaced mode for speed 0.
- Add the program shell around the simulation: title screen with the 287-frame animation, mode selection, pause requests, initials entry at the end of a qualifying game and the return to the title. `PRESS ANY KEY`, `GAME OVER` and `TIME UP` are drawn by the simulation itself.
- Add the audio device lifecycle: open at 44100 Hz mono, run silent with a notice when no device exists, toggle sound and music at runtime. Sound can be switched on again after a quiet start (upstream bug fixed) and a master volume setting is honoured (upstream ignored it).
- Add keyboard input with a 20-action table, upstream's default keys, direction resolution and fire latching as recorded games expect, interactive key redefinition, and letters-only initials entry (upstream's arrow-keys-type-letters bug fixed).
- Add gamepad support (new; upstream's joystick code is dead): d-pad and left stick with a dead zone, buttons for fire, pause and start, hot-plug, two pads for two diggers.
- Add the command line: long options, upstream's `/X:value` and `-X` single-letter forms as aliases, record, play, play-and-verify with exit statuses 0/1/2/3/4 and the `score=… level=… ticks=…` line, loop playback. Taking over a playback at a chosen tick is offered as `--take-over-at <tick>` (alias `/T:n`) on top of the library support from `add-recording-and-conformance`. `/C` (CGA), `/N` (NetSim) and `/X` (X11 embedding) are rejected with a not-supported message.
- Add a TOML configuration file in the platform configuration directory, with precedence command line > file > defaults and precise error reports. Upstream's `.digger.rc` is not read.
- Add high-score storage: four tables (normal and gauntlet, one and two diggers) in the port's own versioned file in the platform data directory. Upstream's `.digger.sco` is not read.
- Add the user manual `docs/manual.md` and a per-platform manual test checklist.

Out of scope:

- Browser build and release archives for Linux, Windows and macOS: picked up by `add-web-build-and-releases` (phase 7). The shell introduced here is kept free of window, audio and file-system code so that phase 7 can reuse it.
- PortMaster handhelds: a non-goal of the porting plan (those devices offer only SDL2 on KMS/DRM, which the Rust-only window stack cannot drive). No later change picks it up.
- Networked play, CGA mode, X11 window embedding and the cheat key (including take-over by key): not ported by any planned change.
- Changes to the simulation, the recording codecs, the replay verifier or the sound synthesis: owned by phases 3, 4 and 5. This change only consumes their interfaces.

## Capabilities

### New Capabilities

- `desktop-app`: the window and its presentation of the picture, fullscreen, frame pacing, the title screen and mode selection, pause, focus and close behaviour, the audio device lifecycle, on-screen playback of recordings, and the game-over and return-to-title flow.
- `input-controls`: the action table and default keyboard bindings, direction resolution and fire latching, once-per-press function keys, gamepad mapping and hot-plug, interactive key redefinition, and text entry for initials.
- `configuration`: the command line (long options, upstream aliases, rejected options, exit statuses), the TOML configuration file with every key and default, precedence, validation and saving.
- `high-scores`: the four score tables, eligibility, qualification and insertion rules, the initials entry screen, the table display, and the versioned score file with its recovery rules.

### Modified Capabilities

None.

## Impact

- Code: `crates/digger` grows from a skeleton into the full desktop frontend (shell, input, pacing, presentation, audio, configuration, score storage, command line). No change to `digger-core` or `digger-sound` beyond using their public interfaces; section "Interfaces assumed from earlier changes" in `design.md` lists what this change relies on.
- Dependencies added to `crates/digger`: `winit` 0.30, `softbuffer` 0.4, `cpal` 0.18, `gilrs` 0.11, `clap` 4, `serde`, `toml`, `directories`. All are Rust-only; no C library is linked.
- Files on the user's machine: `config.toml` in the platform configuration directory; `scores.toml` and a `recordings` folder in the platform data directory.
- Documentation: `docs/manual.md` (new), `docs/testing/desktop-checklist.md` (new), additions to `docs/compat-quirks.md` for the presentation bugs fixed here.
- CI: the workspace test job gains the frontend's unit and integration tests, all of which run without a display or an audio device. Linux CI needs the ALSA and udev development packages to build the audio and gamepad crates.
