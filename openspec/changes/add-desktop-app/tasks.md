# Tasks

Conventions for every group: work from the specs in this change and `docs/research/`; do not open upstream C source. The last task of each group runs the three workspace checks and commits with a Conventional Commits message.

## 1. Interfaces and crate scaffolding

- [x] 1.1 Compare design.md D3 (points 1–6) with the public interfaces of `digger-core` and `digger-sound` as merged; write any difference and its consequence into design.md D3. Verify: D3 names the real types and methods, and `cargo doc -p digger-core -p digger-sound` builds.
- [x] 1.2 Add `winit` 0.30, `softbuffer` 0.4, `cpal` 0.18, `gilrs` 0.11, `clap` 4 (derive), `serde`, `toml`, `directories` to `crates/digger/Cargo.toml`. Verify: `cargo build -p digger` succeeds and `cargo deny check` passes. Add the ALSA and udev development packages to the Linux CI job; CI builds the crate on Linux, Windows and macOS.
- [x] 1.3 Create the module tree of design.md D1 with empty modules and `#![forbid(unsafe_code)]`. Verify: `cargo build -p digger` and the new test `platform_imports_are_confined` (no `winit|softbuffer|cpal|gilrs` import outside `src/platform/`) pass.
- [x] 1.4 Add the `report` helpers (`error`, `notice`, `warning` with the `digger:` prefix) and `AppError` with `exit_code()` (64, 66, 69, 78, 1–4). Verify: unit test `app_error_exit_codes` passes.
- [x] 1.5 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`; commit `chore(digger): scaffold desktop frontend modules`.

## 2. Settings model and configuration file

- [x] 2.1 Define `Settings`, `Layer`, `GameMode`, `ScaleMode`, `Speed`/`FrameTime` and the static schema table (path, type, range, default) for every key of the `configuration` spec. Verify: unit test `schema_defaults_match_spec` checks each default (speed 40, sample rate 44100, buffer 512, volume 100, dead zone 0.5, …).
- [x] 2.2 Implement `KeyName` (W3C code names) and `PadButton` name parsing and printing, and the default `[keys]` and `[gamepad]` bindings. Verify: unit tests `key_names_round_trip` and `default_bindings_unique` (20 actions, no key twice).
- [x] 2.3 Implement reading the file by walking the TOML table against the schema: type, range, unknown names, duplicate key bindings, unknown keys as warnings. Verify: unit tests `config_out_of_range_names_key`, `config_duplicate_binding`, `config_unknown_key_warns`, `config_invalid_toml`.
- [x] 2.4 Implement `Settings::resolve` (defaults < file < command line; mode taken from the command line as a whole). Verify: unit tests `precedence_cli_over_file` and `mode_option_replaces_file_mode`.
- [x] 2.5 Implement saving: runtime-change layer over the re-read file, complete file with header comment, write to `config.toml.tmp` and rename, directory creation. Verify: unit tests `first_save_writes_all_keys`, `cli_values_not_saved`, `no_change_no_write` in a temporary directory. Include platform paths (`config.toml`, data directory) with the `--config` and `--data-dir` overrides, verified by `paths_overrides` and per-platform expectations under `#[cfg(target_os)]`.
- [x] 2.6 Run the three workspace checks; commit `feat(digger): settings model and TOML configuration file`.

## 3. Command line

- [x] 3.1 Define the clap derive struct with every long option of the `configuration` spec, value ranges, the positional argument, `-h` and `-V`. Verify: unit tests `cli_long_options` and `cli_bare_number_is_speed`, `cli_bare_word_is_level_file`.
- [x] 3.2 Implement the legacy alias pre-pass (`cli::legacy::translate`) with the original-spelling side table. Verify: table test `legacy_aliases` covering every letter in both cases, `/S40`, `-s0`, `/G`, `/G:120`, `/KA`, `/V`, tokens after `--`, and `legacy_absolute_paths_untouched` (`/Users/a/x.dlf`, `/Library/x`, `/home/u/l.dlf`). Reject `/C`, `/N`, `/X` with the not-supported message and status 64, verified by `legacy_unsupported_options`; map `/T:n` to `--take-over-at n`, verified by `legacy_take_over_alias`.
- [x] 3.3 Implement combination checks (`--players 2` with `--two-diggers`/`--gauntlet`, `--verify` with `--play`/`--loop`, `--headless` without `--verify`, `--loop` without `--play`, `--take-over-at` without `--play` or with `--verify`) and map all usage errors to status 64 quoting the spelling the user typed. Verify: unit tests `cli_conflicts` and `cli_range_errors_quote_alias`.
- [x] 3.4 Implement level-file checking (existence, `.dlf` retry, 1202-byte minimum) returning status 66 on failure. Verify: unit tests `level_file_missing`, `level_file_dlf_retry`, `level_file_too_short`.
- [x] 3.5 Wire `main`: parse, load file, resolve, print errors, return `ExitCode`; implement `--help` text with aliases and `--version`. Verify: integration test `tests/cli_basics.rs` runs the binary for `--help` (0, lists `/S:N`), `--version` (0), `--turbo` (64), `/C` (64), a bad config file (78).
- [x] 3.6 Run the three workspace checks; commit `feat(digger): command line with upstream option aliases`.

## 4. High-score tables and storage

- [x] 4.1 Implement `ScoreTable` with `qualifies` and `insert`, and `TableKey::from_mode`. Verify: unit tests `qualify_strict`, `qualify_zero_against_empty`, `insert_tie_above`, `insert_drops_tenth`, `table_key_mapping`. Include the row text formatter (initials, two spaces, six-character right-aligned score), verified by `score_row_text` (`ABC   12345`, `...       0`).
- [x] 4.2 Implement the `scores.toml` version 1 schema: serialise and parse, default tables for absent ones, custom level-set keys. Verify: unit tests `scores_round_trip` and `scores_custom_level_set_separate`.
- [x] 4.3 Implement loading with recovery (`Fresh`, `Loaded`, `Recovered`) for missing, unparsable, structurally wrong and newer-format files. Verify: unit tests `scores_missing_is_fresh`, `scores_garbage_recovers`, `scores_newer_format_recovers`.
- [x] 4.4 Implement atomic saving, the rename to `scores.toml.bad` after recovery, and the notice on write failure. Verify: unit tests `scores_save_atomic`, `scores_bad_file_preserved`, `scores_unwritable_keeps_memory`.
- [x] 4.5 Run the three workspace checks; commit `feat(digger): high-score tables and score file`.

## 5. Input layer

- [x] 5.1 Implement `Action` and `Bindings` (key → action, pad button → action) built from `Settings`. Verify: unit test `bindings_lookup`.
- [x] 5.2 Implement `InputState` for the ten player controls with held and pressed-since-frame flags and `take_frame()`. Verify: unit tests `input_tap_is_seen_once`, `input_held_persists`, `input_release_clears`.
- [x] 5.3 Implement once-per-press function actions with repeat suppression, the any-key flag with modifier and excluded-action handling, and the Alt+Enter chord filter. Verify: unit tests `action_once_per_press`, `two_actions_same_frame`, `alt_enter_not_any_key`.
- [x] 5.4 Implement focus-loss handling (release everything, emit the focus event) and the typed-letter queue. Verify: unit tests `focus_loss_releases_all` and `text_queue_letters_only_passthrough`.
- [x] 5.5 Add end-to-end resolution tests that feed `InputState` output into the core for scripted press/release timelines. Verify: `input_resolution_last_pressed`, `input_resolution_same_frame`, `input_resolution_release_fallback`, `input_tap_one_tick`, `fire_takes_effect_next_tick`, `alternating_uses_p1_controls` pass (the resolver and fire latch are the core's; these tests prove the frontend feeds them correctly). Add the quirk entries for direction resolution, tap pulse, fire delay and once-per-press actions to `docs/compat-quirks.md`, each naming its test.
- [x] 5.6 Implement pad slot assignment, d-pad mapping and the stick-to-virtual-button conversion with dead zone and hysteresis, as pure functions over an abstract pad event type. Verify: unit tests `stick_dead_zone`, `stick_hysteresis`, `stick_roll_up_then_right`, `pad_slots_assign_and_free`, `pad_removed_releases_controls`.
- [x] 5.7 Run the three workspace checks; commit `feat(digger): input mapping layer for keyboard and gamepads`.

## 6. Pacing and presentation maths

- [x] 6.1 Implement `Clock` and `Pacer` with absolute deadlines, the cap of 3 due frames and re-anchoring. Verify: unit tests `pacer_no_drift_7500_frames` (jittered fake clock, 600 s → 7500 ± 1 frames), `pacer_stall_reanchors`, `pacer_speed_change`. Include unpaced mode (8 ms step budget, 16 ms redraw interval), verified by `pacer_unpaced_budget`.
- [x] 6.2 Implement `present::scale::viewport` for `integer`, `fit`, `stretch` and the column and row maps. Verify: unit tests `viewport_integer_1920x1080` (1280×800 at 320,140), `viewport_integer_1300x1300`, `viewport_fit_1920x1080` (1728×1080), `viewport_integer_small_falls_back` (500×312), `viewport_stretch`, `maps_monotonic`.
- [x] 6.3 Implement `present::blit` (indices through a 16-entry colour table into an XRGB buffer with black borders). Verify: unit tests `blit_scale_1`, `blit_scale_2_replicates`, `blit_borders_black`, `blit_palette_switch`.
- [x] 6.4 Run the three workspace checks; commit `feat(digger): frame pacer and scaling maths`.

## 7. Shell core and the play scene

- [x] 7.1 Implement `Shell`, `Scene`, `Transition`, `ShellInput`, `FrameOutput` and a test harness that feeds scripted frames and captures pictures, sound commands and requests. Verify: unit test `shell_starts_on_title` and the harness compiles into the other shell tests.
- [x] 7.2 Implement `PlayScene` for a live game: build `Game` from settings and the selected mode, build the core's `FrameInput` from `ShellInput`, step once per frame, forward sound commands, attach the phase 4 recorder. Verify: shell test `play_scene_steps_one_frame_per_call` (tick counter equals frames for simulated ticks).
- [x] 7.3 Implement pause through the core: the pause action sets `FrameInput::pause` (kept set until the game reports that it is paused, `Game::is_paused()`; the pausing frame itself reports `Tick`), any key or button while paused sets it again to resume and is consumed, and the picture presented is always the core's own (`PRESS ANY KEY` at 80,0 comes from the core, as do the pause and resume sound commands). Verify: shell tests `pause_consumes_no_ticks`, `pause_text_comes_from_core` (presented picture CRC equals the game framebuffer CRC), `resume_key_not_a_control`. Pause in the same way on focus loss and pad removal, for live games only, verified by `focus_loss_pauses_live_game` and `focus_loss_ignored_on_title_and_playback`.
- [x] 7.4 Implement speed actions (±10000 µs, floor rule, 1000000 µs cap, pacer notified, not saved) and sound/music toggles with their runtime-change layer entries. Verify: shell tests `speed_floor`, `speed_cap`, `toggles_recorded_for_save`, `quiet_start_then_toggle_enables_sound`.
- [x] 7.5 Implement quit from a game or pause (back to the title, recording finalised with its end marker) and close-request handling in the play scene. Verify: shell tests `quit_returns_to_title_without_banner` and `close_during_game_finalises_recording`.
- [x] 7.6 Run the three workspace checks; commit `feat(digger): shell state machine and play scene`.

## 8. Title screen

- [x] 8.1 Implement the static title scene: title picture, `D I G G E R` at (100,0), mode label lines with their x positions, high-score table rows. Verify: shell test `title_static_layout` and golden CRC `title_frame0_crc`.
- [x] 8.2 Encode the 287-frame animation script as a data table and its interpreter (spawns, moves, labels, shots, cycle reset). Verify: unit test `title_script_keyframes` (labels present from frames 83, 156, 163, 183, 203, 223; cast panel cleared at frame 0) and golden CRCs at frames 164, 240 and 286.
- [x] 8.3 Implement mode cycling in upstream order with table reload, and recording the change for saving. Verify: shell tests `mode_cycle_order` and `mode_cycle_wraps_and_swaps_table`.
- [x] 8.4 Implement start detection (any key minus excluded actions and modifiers, pad fire/pause buttons, keys held from before ignored) and the quit action on the title; the redefine and save-recording actions are recognised there as title actions that never start a game, and what they open or write is wired by the tasks that build them (10.1 and 10.3 for redefinition, 9.6 for saving). Verify: shell tests `title_space_starts`, `title_f7_does_not_start`, `title_held_key_does_not_start`, `title_quit_exits_zero`.
- [x] 8.5 Run the three workspace checks; commit `feat(digger): title screen with animation and mode selection`.

## 9. Game over, initials entry and recordings

- [x] 9.1 Implement the end-of-game hand-over (`shell::game_over`): on the frame whose outcome carries `player_out`, decide eligibility and qualification; suspend the play scene for one `InitialsScene` per qualifying digger, then resume stepping so the core shows its 50-frame `GAME OVER` or `TIME UP` notice; continue with the other alternating player, or return to the title when the outcome reports the game finished. Verify: shell test `game_over_sequences` covering one player, gauntlet with and without qualifying score, two diggers, first alternating player out.
- [x] 9.2 Implement `InitialsScene` layout and keyboard entry: letters only, upper case, Backspace/Delete, 20 closing frames, sound stopped. Verify: shell tests `initials_three_letters`, `initials_arrow_keys_ignored`, `initials_backspace`, and golden CRC `initials_screen_crc`.
- [x] 9.3 Implement the per-frame palette alternation ending on the normal palette, and `video.reduce_flashing`. Verify: shell tests `initials_palette_alternates` and `initials_reduce_flashing_steady`.
- [x] 9.4 Implement gamepad letter entry (up/down cycle, fire accepts, left steps back). Verify: shell test `initials_with_pad_cab`.
- [x] 9.5 Connect eligibility and storage: skip entry for playback, taken-over and quit games, insert and save on completion, show the new table on the title. Verify: shell tests `playback_never_enters_scores`, `taken_over_game_never_enters_scores`, `unlimited_lives_is_eligible`, `entry_saved_before_title` (score file in a temporary directory). Add quirk entries (strict qualification, ties above, table mapping, game-over order, letters-only fix) to `docs/compat-quirks.md` with their test names.
- [x] 9.6 Implement recording hand-over: `last_game`, automatic save with `--record` (format by extension), F8 save into `recordings/` with the auto name and numeric suffix, initials passed to the recorder. Verify: shell tests `record_option_saves_each_game`, `f8_saves_once_with_auto_name`, `f8_without_game_does_nothing`, `initials_line_in_drf`.
- [x] 9.7 Run the three workspace checks; commit `feat(digger): game-over flow, initials entry and recording saves`.

## 10. Key redefinition

- [x] 10.1 Implement `RedefineScene` prompts and layout (`PRESS NEW KEY FOR`, `PLAYER 2:` for two-Digger modes, colour 2 → colour 1, second column when the list is long). Verify: shell test `redefine_prompts_in_order` and golden CRC `redefine_screen_crc`.
- [x] 10.2 Implement duplicate refusal, function-action conflict prompts, the "all" variant, Alt refusal and quit-key abort with full restore. Verify: shell tests `redefine_duplicate_refused`, `redefine_conflict_asks_pause`, `redefine_all_asks_every_action`, `redefine_abort_restores`.
- [x] 10.3 Save completed bindings to the configuration file at once and apply them to `Bindings`; support `--redefine-keys[=all]` as the opening scene. Verify: shell test `redefine_persists_and_applies` (file in a temporary directory re-loaded with the new keys).
- [x] 10.4 Run the three workspace checks; commit `feat(digger): interactive key redefinition`.

## 11. Playback and verification

- [x] 11.1 Implement `PlaybackScene` on phase 4's `Replay` (`Replay::step` once per frame): player controls ignored, function actions honoured, no score entry at the end, `--play` list in order, `--loop`. Verify: shell tests `playback_ignores_player_keys`, `playback_list_then_title`, `playback_loop_until_quit`, `playback_bad_file_skipped_with_notice`.
- [x] 11.2 Surface phase 4's `ReplayReport::exit_status()` (0, 1, 2, 3, 4) and `result_line()` in both forms, and re-saving with `--record`. Verify: unit tests `verify_status_mapping` and `result_line_formats` (`score=3100 level=2 ticks=1378`, `score=18100 score2=1100 level=4 ticks=4489`).
- [x] 11.3 Implement `headless.rs` and wire `--verify --headless` in `main`. Verify: integration test `tests/verify_cli.rs` runs the binary on small upstream recordings: `sobodig.drf` → status 0 and the expected line from `tests/recordings`; a text file → 1; a recording with one altered checkpoint → 2; an eDRF cut before its end marker → 4; `--record out.edrf --verify sobodig.drf` writes a file that verifies with status 0. Add the verification statuses and result line to `docs/compat-quirks.md`, naming this test.
- [x] 11.4 Implement `--take-over-at <tick>`: pass the tick to `ReplayOptions::take_over_after`, switch the scene's input from ignored to live once the replay reports the hand-over, keep `--record` running across it, and mark the game as not eligible for high scores. Verify: shell tests `take_over_switches_to_live_controls_after_tick`, `take_over_recording_replays_whole_game` and `taken_over_game_never_enters_scores`.
- [x] 11.5 Run the three workspace checks; commit `feat(digger): on-screen playback and play-and-verify`.

## 12. Window, event loop and presentation

- [ ] 12.1 Implement window creation in `resumed` (title, 1280×800 physical default, 640×400 minimum, resizable, icon where supported) and the `NoDisplay` error path. Verify: `cargo run -p digger` opens the window; on Linux with `DISPLAY` and `WAYLAND_DISPLAY` unset the binary prints the error and exits 69 (integration test `no_display_exits_69`, Linux only).
- [ ] 12.2 Implement key event translation (physical key → `KeyName`, repeat flag, text, modifiers, Alt+Enter chord) into `InputState`. Verify: unit test `winit_keycode_table_covers_all_key_names` and manual check that arrow keys move Digger.
- [ ] 12.3 Implement `about_to_wait` pacing (`WaitUntil` next deadline, due frames, `Poll` in unpaced mode) calling `shell.frame`. Verify: with the debug frame counter enabled, 750 ± 2 frames are counted in 60 s at the default speed; CPU use is near idle on the title screen.
- [ ] 12.4 Implement softbuffer presentation on `RedrawRequested` with viewport recomputation on resize and scale-factor change. Verify: manual check of letterboxing at several window sizes in each scale mode against the values of the `desktop-app` spec.
- [ ] 12.5 Implement fullscreen toggling (borderless, pointer hidden, previous size restored), `--fullscreen` start and the runtime-change entry. Verify: manual check of Alt+Enter and F11 on the title, in a game and during pause; `fullscreen = true` appears in the saved file.
- [ ] 12.6 Implement focus and close events, platform requests (`Exit`, `ToggleFullscreen`, `RetryAudio`) and the single shutdown path (stop audio, pending recording, settings save). Verify: closing the window in each scene exits with status 0 and leaves the files described in the `desktop-app` spec.
- [ ] 12.7 Run the three workspace checks; commit `feat(digger): window, event loop and softbuffer presentation`.

## 13. Audio device and gamepads

- [ ] 13.1 Implement audio opening (configured rate and buffer, mono preferred, fallbacks with notices) and the callback that owns the `AudioSource` from `sound_link`, converts sample format and fills all channels. Verify: unit tests `audio_config_selection` (over fake capability lists) and `audio_fill_duplicates_channels`; manual check that the main tune plays. Forward the shell's sound commands to the `SoundHandle` (the core's commands including its pause and resume, stop on scene changes) and the toggles and volume as `SoundControl`s; manual check: `digger --quiet`, start a game, F9 makes sound audible and F7 stops only the tune.
- [ ] 13.2 Implement the `Closed` state: no device at start, stream error at runtime, retry on the sound toggle. Verify: unit test `audio_state_transitions`; manual check with the sound device disabled, then enabled and F9 pressed.
- [ ] 13.3 Implement gilrs polling into the abstract pad events of task 5.6, with connect and disconnect. Verify: manual check with one pad (d-pad, stick, fire, Start, Select) and with two pads in two-player simultaneous mode; unplugging pauses the game.
- [ ] 13.4 Run the three workspace checks; commit `feat(digger): audio device lifecycle and gamepad polling`.

## 14. Documentation and platform verification

- [ ] 14.1 Write `docs/manual.md`: starting the game, modes, default keys and pads, redefinition, every option with its upstream alias, exit statuses, the full settings table, file locations per platform, recordings, differences from upstream. Verify: unit test `manual_settings_table_matches_schema` and `manual_option_list_matches_cli` pass.
- [ ] 14.2 Write `docs/testing/desktop-checklist.md` from design.md D15 with one results column per platform. Verify: the file lists every manual item of D15.
- [ ] 14.3 Run the checklist on Linux X11 and Linux Wayland; fix or file what fails. Verify: both columns filled in, no open failures.
- [ ] 14.4 Run the checklist on Windows and on macOS. Verify: both columns filled in, no open failures.
- [ ] 14.5 Play one full recorded game per mode (one player, two players, two simultaneous, gauntlet, two-player gauntlet), then verify each recording with `digger --verify <file> --headless`. Verify: every run prints its result line and exits 0.
- [ ] 14.6 Run `openspec validate add-desktop-app --strict`, then the three workspace checks; commit `docs: user manual and desktop test checklist`.
