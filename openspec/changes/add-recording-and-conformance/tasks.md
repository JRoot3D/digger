# Tasks

Conventions for every group: work from the specs, design.md and
`docs/research/`; do not open upstream C source. Tests and documentation land
in the group that introduces the code. The last task of each group is the
quality gate and the commit (Conventional Commits, at least one commit per
group). "The gate" below means `cargo fmt --check`, `cargo clippy
--all-targets -- -D warnings` and `cargo test --workspace` all passing.

Conformance tests are introduced in group 7 while the simulation is still
unproven. Until a recording passes, its test carries `#[ignore = "pending
triage"]` so the gate stays green; group 12 removes those markers one cluster
at a time and group 13 forbids any that remain.

## 1. Simulation seam and module scaffolding

- [x] 1.1 Create empty modules `recording/`, `state_hash/`, `replay/` per design D1 with module docs, add `md-5` to `digger-core` and `sha2` as a dev-dependency, and start `docs/conformance.md` with a section "Simulation seam" mapping every row of design D4's hook table onto the `Observer` hooks of `Game::step_observed` as merged; verify `cargo build --workspace` and `cargo doc -p digger-core --no-deps` succeed and no D4 row is left unmapped
- [x] 1.2 Add `ReplayObserver` (`replay/playback.rs`) implementing phase 3's `Observer` with optional playback source, recorder, verifier and trace fields, all absent so far, forwarding in the order of design D4; verify all phase 3 tests still pass unchanged (`cargo test -p digger-core`) and a new test `empty_replay_observer_changes_nothing` (a scripted game gives the same summary and trace through `step` and through `step_observed` with it)
- [x] 1.3 Pin the hook call points the recorders and verifier rely on; verify with a new test `observer_event_order_one_round` that records the hook sequence of a scripted one-life game with a logging observer and compares it with the expected list (level layout, seed, per-tick controls for both slots, movement per control read, tick end, round end after clean-up, game end)
- [x] 1.4 Pin the hook call points playback relies on; verify with tests `movement_hook_only_when_alive_visible_unpaused`, `controls_hook_called_for_unread_slot` and `level_plan_hook_only_for_participating_players` using a counting observer, and fix any gap in the simulation with its own test
- [x] 1.5 Run the gate and commit (`test(core): pin the observer seam used by recording and playback`)

## 2. Upstream recording corpus

- [x] 2.1 Implement `cargo xtask import-recordings --from <dir> [--allow-other-commit]`: check the checkout's HEAD is `638125eecc2a36629b886f58d7b75253894cf3d3`, copy `tests/data/*.drf`, `*.edrf` to `tests/recordings/data/` and `tests/results/*.out` to `tests/recordings/results/` byte-for-byte, write `MANIFEST.sha256`; verify with an xtask unit test against a temporary fake checkout (right commit copies, wrong commit refuses and changes nothing)
- [x] 2.2 Run the import against an upstream checkout at `638125e`, add the files, add `.gitattributes` (`tests/recordings/** -text`) and `tests/recordings/README.md` stating origin (repository, commit, upstream paths), licence (GPL-2.0-or-later, same as this repository), the import command, the modes covered (`1`, `M2`, `G120`) and the known gaps (no alternating two-player game, no start level, no pre-1998 file); verify by `ls` 23 `.drf`, 27 `.edrf`, 27 `.out`, that `muu27700.drf` still has CR LF line ends, and that the README's command is the one implemented in 2.1
- [x] 2.3 Add `crates/digger-core/tests/corpus_integrity.rs`: every file's SHA-256 equals the manifest, the counts are 23/27/27, every `.drf` has an `.edrf` twin and every base name has one `.out` consisting of exactly one line; verify `cargo test -p digger-core --test corpus_integrity`
- [x] 2.4 Run the gate and commit (`test: import upstream recordings at 638125e`)

## 3. Header and DRF reader

- [ ] 3.1 Implement `RecordingError`/`Location` with their `Display` texts (design D2), first-line detection and the version, mode and bonus lines (`recording/header.rs`) with LF and CR LF line ends; verify with unit tests asserting each error message and table-driven tests covering every scenario of the spec requirements "Recording kind is detected from the first line", "Common header lines" and "Mode line grammar" (accepted: `1`, `2`, `M2`, `G120`, `M2G300I5`, `2UI3`, empty; rejected: `3`, `M3`, `G0`, `G4000`, `I0`, `I1001`, `1X`)
- [ ] 3.2 Implement the pre-1998 check (decimal at byte offset 7 ≤ 19981125, missing date reads as 0) with the message quoting the version line; verify with tests `rejects_19981125`, `accepts_19990130`, `rejects_version_without_date`
- [ ] 3.3 Implement the DRF map block (80 lines, first 15 bytes, short rows and non-map bytes are earth); verify with tests `drf_short_map_row_is_earth`, `drf_overlong_map_row_truncated`, `drf_missing_map_rows_is_header_error`
- [ ] 3.4 Implement the DRF body filter and `DrfCursor` (seed with optional initials, runs with counts, `EOL`, `E`/end of data, checked count parsing, runs not expanded); verify with tests `drf_run_length_decoding`, `drf_body_ignores_control_bytes`, `drf_initials_before_seed`, `drf_bad_seed_reports_offset`, `drf_unknown_run_letter_reports_offset`, `drf_end_of_data_is_end_marker`
- [ ] 3.5 Parse all 23 corpus `.drf` headers and walk each body with the cursor alone (seed, runs, `EOL` repeatedly); verify with test `corpus_drf_bodies_tokenise` that every file tokenises to the end with no error, and record per file the version line, mode and number of rounds in `docs/conformance.md`
- [ ] 3.6 Run the gate and commit (`feat(core): read DRF recordings`)

## 4. eDRF reader

- [ ] 4.1 Implement the eDRF line reader (LF/CR LF, blank lines skipped outside maps, unterminated last line dropped and flagged as truncated) and the `C`, `E`, `R`, `Q`, `Z` records; verify with table-driven tests for each accepted form and each bad form of the spec scenario "Bad record" (`X 1234`, `C 12G4`, `C 1 2`, `Q 2`, `Z 100`), asserting the line number and quoted text
- [ ] 4.2 Implement `I` records: slot lists, run lists stored as `(value, count)` pairs, padding of unlisted slots, zero count and wrong run-list count rejected; verify with tests for every scenario of "eDRF control values and input records" and `edrf_huge_count_is_not_expanded` (parse `I @4000000000` and consume three values)
- [ ] 4.3 Implement `L` records: inline 10-row maps (leading space, padding, empty line as earth row) and reuse by 1-based index; verify with tests `edrf_inline_map`, `edrf_map_reuse`, `edrf_map_reuse_unknown_index`, `edrf_map_row_too_long`, `edrf_map_row_without_space`
- [ ] 4.4 Assemble `EdrfRecording` (queues per kind, quit slot, end record, truncated flag; nothing may follow `Z`) and write the "DRF" and "eDRF" sections of `docs/recording-formats.md` (grammar, semantics, the port's deviations from upstream's reader); verify with test `corpus_edrf_parse` that all 27 corpus files parse, each `Z` tick total equals the `ticks=` value of its `.out` file and `2riptest01` has `Q 0` and `Z 1024`, and that every production of the two spec grammar scenarios appears in the document
- [ ] 4.5 Run the gate and commit (`feat(core): read eDRF recordings`)

## 5. State hash

- [ ] 5.1 Implement `state_hash/layout.rs` (the ordered field table with block, name, width, repeat, condition) and `codes.rs` (direction → 0/2/4/6/−1 and the other enum wire codes, exhaustive matches); verify with test `layout_golden` comparing the rendered table with a checked-in text file and asserting block lengths 300, 263, 163–211, 165 and totals 903 / 908 / 951
- [ ] 5.2 Define the plain `*HashFields` structs, and implement `walk` with the MD5 visitor, `hash()` and `hash_at_round_end(tick)`; verify with tests on hand-built field values: `hash_is_md5_prefix_big_endian`, `values_are_little_endian`, `bool_is_one_byte`, `no_direction_is_ffff`, `round_end_appends_tick`, `gauntlet_appends_clock_and_timeout`
- [ ] 5.3 Build the `*HashFields` values from the simulation's `StateView` (live state of the current player only; never-changing flags as constants with a comment citing quirk P10; monster body present from spawn until the round's monster reset, per design D5); verify with tests `hash_stream_length_fresh_game` (903 bytes, 908 in gauntlet), `monster_block_is_variable_length` (grows by 8 when a monster spawns, keeps the 8 bytes after that monster is killed, and shrinks at the next round start) and `hash_ignores_unhashed_values`
- [ ] 5.4 Add the per-block and collecting visitors (block hashes, block byte lengths, decoded `(name, value)` list); verify with test `block_hashes_recombine` (concatenated block streams hash to the full hash) and `decoded_dump_matches_layout_order`
- [ ] 5.5 Add the "State hash" section to `docs/recording-formats.md`, generated from the layout table by a test that fails when the document is stale (`layout_doc_is_current`), and add quirk entries P10, P11, P13 to `docs/compat-quirks.md`; verify the test passes
- [ ] 5.6 Run the gate and commit (`feat(core): add state hash as a table-driven format`)

## 6. Playback

- [ ] 6.1 Implement building the game configuration from a recording header alone (mode, gauntlet time, start level, bonus interval, unlimited lives off) and reject combinations the simulation refuses; verify with tests `config_from_header_modes` and `mode_u_ignored`
- [ ] 6.2 Implement the eDRF playback source behind `ReplayObserver`: seeds from `R` in `round_seed`, raw controls per slot per tick in `controls` (an unread slot still uses up a value), level maps from `L` in `level_plan` with level-number check and no two-digger patch, quit (through `FrameInput::quit` on the next frame) when all values and records are used up; verify with scripted-recording tests `edrf_controls_pass_through_resolver`, `edrf_unread_slot_consumes_value`, `edrf_level_number_mismatch`, `two_digger_patch_drf_only`, `edrf_quits_when_exhausted`
- [ ] 6.3 Implement the DRF playback source: seeds from the cursor, all-zero raw controls, one record per `movement` hook, header maps with the two-digger patch, `EOL` at round end, end marker → end flag and quit on the next frame, out-of-step detection; add quirk entries P1–P9 to `docs/compat-quirks.md`, each naming its test; verify with tests `drf_record_consumed_only_on_control_read`, `drf_two_diggers_interleave`, `drf_end_marker_counts_the_tick`, `drf_fire_does_not_linger_over_pause`, `drf_out_of_step_at_round_end`, and that every test named in the quirk entries exists (`cargo test -p digger-core -- --list`)
- [ ] 6.4 Implement `Replay::new`, `Replay::step(input, extra)` (one frame through `Game::step_observed`, never sleeps) and `Replay::game`, with the `max_ticks` safety net; verify with test `replay_steps_one_frame_at_a_time` and by replaying `dror0.edrf` to completion without checking hashes (no panic in a debug build, tick total printed)
- [ ] 6.5 Run the gate and commit (`feat(core): play back DRF and eDRF recordings`)

## 7. Verification, outcome and the verify run

- [ ] 7.1 Implement the `Verifier` (fed by `ReplayObserver` from `tick_end`, `round_end` and `game_end`): compare `C` at multiples of 256, `E` at round end, `Z` at game end, first mismatch only, remember the last matching checkpoint, tolerate recorded checkpoints never reached; verify with tests on synthetic recordings: `checkpoint_mismatch_reports_tick_and_hashes`, `only_first_failure_reported`, `end_record_tick_mismatch`, `truncated_checked_up_to_cut`
- [ ] 7.2 Implement `Outcome` classification in `Replay::finish` with the spec's precedence, and `ReplayReport` (scores, two-score flag, level, ticks); verify with tests `outcome_verified`, `outcome_completed_drf`, `outcome_stopped_when_game_ends_early`, `outcome_truncated`, `outcome_failure_takes_precedence`
- [ ] 7.3 Implement `verify(bytes, options)`, `ReplayReport::result_line`, `ReplayReport::exit_status`, the divergence report texts, the error-to-status mapping and `cargo xtask verify <file>` (result line on stdout only on status 0, diagnostics on stderr); verify with tests `result_line_one_player` (`score=3100 level=2 ticks=1378\n`), `result_line_two_scores`, `exit_status_table`, `divergence_messages`, and an xtask integration test running the command on a missing path (status 1), a corpus eDRF with a line `X 1` inserted (status 2) and one cut after a checkpoint (status 4)
- [ ] 7.4 Add `crates/digger-core/tests/conformance.rs`: the `recording!` macro list of all 50 files with tier (`quick`/`full`) and state (`pending`/`passing`), comparing status 0 and the result line with `results/<base>.out`; plus `conformance_list_matches_corpus`; verify the list test passes and `cargo test -p digger-core --test conformance -- --ignored --list` shows 50 tests
- [ ] 7.5 Implement `cargo xtask conformance [--tier] [--filter] [--jobs]` writing `target/conformance/report.md` (one row per recording: outcome, status, first failing checkpoint, last matching tick, expected and actual result line); verify by running it and reading the report — failures are expected at this point and are the input to group 12
- [ ] 7.6 Run the gate and commit (`feat(core): verify replays against checkpoints and expected results`)

## 8. Recording, conversion and naming

- [ ] 8.1 Implement `DrfRecorder` (header with version line `RS DIG 20261111`, mode line, bonus, 80 rows, seeds, run-length with 999 cap, 60-column wrap, `EOL`, `EOG`, initials line, unbounded length); verify with tests `drf_writer_mode_lines`, `drf_writer_run_cap_and_wrap` (`sr999rL2`), `drf_writer_quit_appends_eog`, `drf_writer_exceeds_256k`
- [ ] 8.2 Implement `EdrfRecorder` (32-tick chunks of `Read`/`Unread` samples, chunk forms, flush before other records, `L` with map de-duplication, `R`, `C`, `E`, `Q`, `Z`, `take_pending`); verify with tests `edrf_chunk_forms` (`I @…`, `I0 D32`, `I01 D32 H32`), `edrf_map_reuse_written`, `checkpoint_then_round_end_order`, `one_player_game_has_single_level_record`, `edrf_prefix_at_chunk_boundary_is_playable`
- [ ] 8.3 Implement the DRF-playback mode of `EdrfRecorder` (bits from the supplied direction and fire at control reads, 0 otherwise); verify with test `conversion_bits_from_drf_records` on a scripted DRF
- [ ] 8.4 Add `codec_roundtrip.rs`: scripted live games in each mode (one player, alternating, two diggers, gauntlet, start level 5) recorded as DRF and as eDRF, replayed, and re-recorded; verify equal reports and byte-identical second recordings (`roundtrip_drf_*`, `roundtrip_edrf_*`), and `edrf_to_drf_same_result`
- [ ] 8.5 Implement the default file name and the `.edrf`-suffix format selection (`recording/naming.rs`); add quirk entries P12, P14–P16 and the "Writing" sections of `docs/recording-formats.md` (version line, wrap rule, chunk forms, deviations: no size limit, LF only); verify with test `default_names` covering `mmm27700.drf`, `ac1859075.drf`, `ab250000.drf`, `bc250000.drf`, `___3100.drf`, `DIGGER.DRF`, and that each quirk entry names an existing test
- [ ] 8.6 Implement `cargo xtask convert <in> <out>` and add `conversion_twins.rs` comparing each converted corpus DRF with its upstream twin semantically (marked `pending` like its conformance test until the DRF passes) plus the informational byte-identity count; verify `cargo xtask convert` on a hand-written short DRF produces an eDRF that `cargo xtask verify` accepts with status 0
- [ ] 8.7 Run the gate and commit (`feat(core): record games as DRF and eDRF`)

## 9. Take-over and configuration isolation

- [ ] 9.1 Implement take-over in the playback source (`take_over_after`, `request_take_over`; afterwards the hooks return the simulation's own values and the caller's live `FrameInput` drives the game, verifier disabled, outcome `TakenOver`, not eligible for high score) and document it under "Playback rules" in `docs/recording-formats.md` together with the library calls that `add-desktop-app`'s play, record, verify and take-over options map to; verify with `takeover.rs` tests `takeover_switches_after_tick`, `takeover_rerecorded_game_replays`, `takeover_not_eligible_for_high_score`
- [ ] 9.2 Add `isolation.rs`: replay one corpus recording under caller configurations with unlimited lives on, start level 5, gauntlet mode, two diggers and a different bonus interval, and assert identical `ReplayReport`s and an untouched caller configuration; verify tests `isolation_unlimited_lives`, `isolation_start_level`, `isolation_mode` pass (on a scripted recording until the corpus one passes)
- [ ] 9.3 Run the gate and commit (`feat(core): support take-over and isolate playback from user settings`)

## 10. Triage tooling

- [ ] 10.1 Implement the trace sink (`replay/trace.rs`, fed from `tick_end`): one line per tick with tick, full hash, four block hashes, block byte lengths, RNG state, both scores; optional decoded field list; and `cargo xtask trace-diff <a> <b>` reporting the first differing tick and block; verify with tests `trace_window_has_256_lines`, `tracing_is_passive` (same report with and without tracing) and a trace-diff test on two synthetic traces differing in the monster block at tick 700
- [ ] 10.2 Implement `cargo xtask dump-state <file> --from <tick> --to <tick> [--fields] [--out <path>]`; verify by dumping ticks 1–256 of `dror0.edrf` and checking 256 lines whose last full hash equals the hash the verifier computed for tick 256
- [ ] 10.3 Implement `cargo xtask cut <file.edrf> --at <tick> --out <path>` (same header, maps, seeds and controls up to the tick, then `Q 0`); verify with a test that the cut file parses, replays for exactly that many ticks and is classified truncated
- [ ] 10.4 Implement `cargo xtask bisect <file.edrf>`: print the failing window, the last matching checkpoint, the round-end checkpoints inside the window and the `dump-state` command to run; verify with an xtask test on a corpus eDRF with one altered `C` record that the reported window ends at that checkpoint's tick
- [ ] 10.5 Write the triage procedure in `docs/conformance.md` (order of attack, reading a failure report, window → block → field, comparing candidate fixes with `cut` and `trace-diff`, how to record a finding; the procedure uses only checkpoints, the port's own dumps and the research reports — no upstream binary is built or run) and add the 50-row tracking table generated from the first `xtask conformance` report; verify the table has 50 rows and every command shown runs as written
- [ ] 10.6 Run the gate and commit (`feat(xtask): add state dump, cut, bisect and trace diff for replay triage`)

## 11. Malformed-input hardening

- [ ] 11.1 Add `malformed_input.rs` with a deterministic mutation generator (fixed seeds, no new dependency): 10000 inputs from byte flips, deletions, insertions, line duplications and truncations of the quick-tier corpus files, each parsed and, if it parses, replayed with `max_ticks` set; verify no panic and every run ends with a report or an error (`mutated_recordings_never_panic`)
- [ ] 11.2 Add prefix and resource tests: every 97th-byte prefix of each quick-tier file is parsed and replayed (`prefixes_are_never_verified`: an eDRF prefix is failed, stopped, truncated or a header error unless it is the whole file; DRF prefixes end without panic); `I @4000000000`, the DRF run `s4000000000`, a 10 MB line and a file of 1 000 000 blank lines each complete in under one second without large allocation (`huge_counts_and_lines_are_cheap`); verify both tests pass and fix any finding in the codecs
- [ ] 11.3 Run the gate and commit (`test(core): harden recording codecs against malformed input`)

## 12. Divergence triage — make the upstream recordings pass

Procedure for every task in this group (design D12): run the cluster with
`cargo xtask conformance --filter …`; for the first failure take the window
from the report; localise with `bisect`, `dump-state --fields` and `trace-diff`; fix `digger-core` with a failing-first scenario or unit test;
add or correct the entry in `docs/compat-quirks.md` (class, description,
recording and tick that exposed it, test name) when the research was wrong
or silent; add a MODIFIED delta spec to this change if a simulation
requirement was wrong; update the recording's row in `docs/conformance.md`;
flip the recording from `pending` to `passing` in `conformance.rs`. A task is
done when every recording it names passes with status 0 and the expected
result line, and the gate is green.

- [ ] 12.1 Baseline: run `cargo xtask conformance --tier full`, fill the tracking table (status, first failing checkpoint, last matching tick per recording) and group the failures by first failing tick and block; verify the table in `docs/conformance.md` has an entry for all 50 recordings
- [ ] 12.2 First checkpoint: make tick 256 of `dror0.edrf` match — this proves the hash layout end to end; verify `cargo xtask dump-state` shows the port's tick-256 hash equal to the file's first `C` record, and confirm the monster-body lifetime rule of design D5 on the first round-end checkpoint; if the recording disagrees, correct the rule here and in the simulation's `StateView` and record the finding in `docs/compat-quirks.md`
- [ ] 12.3 One-digger quick eDRFs: `dror0`, `sobodig`, `thevoid`, `thevoids`, `ramon`, `muu27700`; verify `cargo test -p digger-core --test conformance` passes for the six `.edrf` tests with their `pending` markers removed
- [ ] 12.4 Their DRF twins (six `.drf` files, including the CR LF file `muu27700.drf` and the older `AJ DOS`/`TD WIN`/`POK SDL` recorder versions); verify the six `.drf` conformance tests and their `conversion_twins` tests pass unmarked
- [ ] 12.5 Two-digger eDRFs: `2test01`, `2test02`, `2test03`, `2riptest01` (quit at tick 1024); verify the four tests pass unmarked and print `score=… score2=…` lines
- [ ] 12.6 Two-digger DRF `2test01.drf` (interleaved control reads, start-cell patch on plans 3 and 4); verify its conformance and twin-conversion tests pass unmarked
- [ ] 12.7 Gauntlet eDRF `gauntlet01` (clock, time-out, wrapped clock in the hash, lives never decreasing); verify its test passes unmarked — at this point the whole quick tier (18 files) passes in `cargo test --workspace`
- [ ] 12.8 Long eDRFs, first half: the 8 long base names with the fewest ticks (take the order from the tracking table); verify each `.edrf` passes with `cargo test --release -p digger-core --test conformance -- --ignored <name>` and its marker is changed to `passing`
- [ ] 12.9 Long eDRFs, second half: the remaining 8 long base names up to `r3333300` (565330 ticks, level 353 — level-plan cycling, score roll-over at 1000000, level cap behaviour); verify as in 12.8
- [ ] 12.10 Long DRFs: the 16 `.drf` twins of the long games; verify all pass in release mode with markers changed, and all 23 `conversion_twins` tests pass
- [ ] 12.11 Review the findings: every fix made in this group has a test and, where it corrects or extends the research, a `docs/compat-quirks.md` entry citing the recording and tick; every relaxed codec check ("recording wins") is listed under "Deliberate deviations"; verify by reading the tracking table — no row without a cause for a recording that ever failed, no row still failing
- [ ] 12.12 Run the gate and commit per fixed defect or per cluster (`fix(core): …` naming the quirk; `test: mark <recordings> as passing`)

## 13. Continuous integration and definition of done

- [ ] 13.1 Add test `no_pending_recordings` asserting the `recording!` list has no `pending` entry and 18 quick + 32 full-tier entries; verify it passes — this is the definition of done: all 50 upstream recordings pass
- [ ] 13.2 Add the `conformance` CI job (Linux, release): `cargo test --release -p digger-core --test conformance -- --include-ignored`, then `cargo xtask conformance --tier full` uploading `target/conformance/report.md`; confirm the quick tier, codec, hash, malformed-input and isolation tests run in the existing Linux, Windows and macOS jobs; verify all jobs are green on the change's branch (Windows proves CR LF handling and the `-text` attribute) and note the full tier's duration in `docs/conformance.md` (must be under 10 minutes)
- [ ] 13.3 Add port-made fixtures for the modes upstream never recorded — one alternating two-player game and one start-level-5 game, recorded from scripted controls as DRF and eDRF under `tests/recordings/port/` with a README saying they are regression fixtures, not upstream-verified; verify tests `port_fixture_alternating` and `port_fixture_start_level` replay them to verified
- [ ] 13.4 Final documentation pass: `docs/recording-formats.md`, `docs/conformance.md` (final 50-row table, all passing) and `docs/compat-quirks.md` are consistent with the specs; update `docs/porting-plan.md` section 6 only if a stated fact changed; verify `openspec validate add-recording-and-conformance --strict` passes
- [ ] 13.5 Run the gate, then `cargo xtask conformance --tier full` one last time expecting 50 of 50, and commit (`ci: run the full replay conformance suite`)

## Workflow follow-up

- Archive the change once review is complete and the conformance job is green on the main branch.
- Tell `add-desktop-app` implementers that play, record, verify and take-over (`--take-over-at`, `/T:n`) map onto `Replay`, `verify`, `ReplayOptions::take_over_after`, `DrfRecorder`/`EdrfRecorder` and `ReplayReport::exit_status`.
