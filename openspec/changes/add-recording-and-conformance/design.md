# Design

## Context

See proposal.md for motivation. The state this design starts from:

- `add-simulation-core` has delivered `digger-core` with `Game::new(config,
  seed_source)` and `Game::step(controls)`: a deterministic state machine
  advanced one frame at a time, with per-round seeds, raw control bits per
  input slot, the direction resolver, the one-tick fire latch and
  `docs/compat-quirks.md`. Nothing has yet checked it against the original.
- Upstream's recordings are the only upstream file format the port reads and
  writes. Its recorder and player are woven through the game as global hooks
  (`recputdir`, `playgetdir`, `edrf_input`, `edrf_tick`, …) that read and set
  global flags (`playing`, `escape`, `edrf_failed`). The port needs the same
  observable behaviour with none of that structure.
- `digger-core` has no I/O, no clock and no threads, and may not allocate
  inside `Game::step`. Files are read and written by `xtask`, by tests, and
  later by the `digger` binary.
- The research reports were produced by reading code. This change is the
  first time their claims meet real data, so it must expect to find and fix
  simulation defects, and must make that work systematic.

## Goals / Non-Goals

**Goals:**

- Codecs that are total functions over bytes: every input yields a value or a
  typed error; nothing panics.
- A state hash that is a file format in its own right, defined in one place,
  independent of how the simulation lays out its structs.
- One replay driver used unchanged by tests, `xtask` and (later) the desktop
  binary, with pacing left entirely to the caller.
- A triage loop that turns "recording X fails" into "tick N, block B, field
  F" quickly, and a tracked path from there to a fix plus a quirk entry.
- All 50 upstream recordings pass. That is the definition of done.

**Non-Goals:**

- Byte-identical DRF output compared with upstream's writer. DRF output must
  be valid and replay identically; only eDRF output is compared with
  upstream's twins, and semantically (see D9).
- Reproducing upstream's lazy eDRF reader, its 80- and 511-byte line limits,
  its 256 KB recording buffer, or its exit-status timing for bad records.
- A general fuzzing harness (`cargo-fuzz`). It needs a nightly toolchain;
  deterministic mutation tests on stable are enough here.
- Cross-compiling to a big-endian target in CI. Endianness independence is
  obtained by construction (explicit little-endian encoding) and unit-tested
  against fixed byte strings.

## Decisions

### D1. Module layout in `digger-core`

```
crates/digger-core/src/
  recording/
    mod.rs        Recording (enum Drf | Edrf), parse(bytes), shared types
    error.rs      RecordingError, Location
    header.rs     first line, version line, mode line, bonus score
    drf.rs        DrfRecording, DrfCursor (lazy body), DrfWriter
    edrf.rs       EdrfRecording, Record, ControlRuns, EdrfWriter
    naming.rs     default file name
  state_hash/
    mod.rs        StateView, walk(), hash(), hash_at_round_end()
    layout.rs     the field table (names, widths, order, conditions)
    codes.rs      enum -> wire-code mappings
  replay/
    mod.rs        Replay, ReplayOptions, ReplayReport, Outcome, verify()
    director.rs   PlaybackDirector (DRF and eDRF variants), take-over
    verifier.rs   checkpoint comparison, Divergence
    recorder.rs   DrfRecorder, EdrfRecorder
    trace.rs      per-tick trace lines, decoded dump
```

The codecs live in the core because the porting plan places them there and
because the DRF body cannot be decoded without running the game. They work on
`&[u8]` and produce `String`; no file handle crosses the crate boundary.

*Alternative rejected:* a separate `digger-recording` crate. It would need
the simulation's control, direction, mode and map types and the hash needs
read access to simulation state, so the split would be a circular
conversation with no second consumer to justify it.

### D2. Hand-written parsers returning typed errors

Both formats are small, line- or character-oriented and full of
compatibility rules (bytes below 0x20 dropped, `atol`-style date at byte
offset 7, short map rows). They are parsed by hand over byte slices with a
small cursor type. Errors are one enum:

```rust
pub enum RecordingError {
    NotARecording,
    UnsupportedVersion { version_line: String },          // pre-1998
    BadHeader { line: u32, reason: HeaderProblem },
    BadRecord { line: u32, text: String },                // eDRF
    BadDrfBody { offset: usize, reason: DrfProblem },     // found during play
}
```

`RecordingError` implements `std::error::Error` and `Display` with the
messages the specs quote. No `unwrap`, `expect`, slice indexing or integer
parsing that can panic is used on input-derived data; counts are parsed with
checked arithmetic.

*Alternatives rejected:* `nom`/`winnow` (a dependency in the core for two
trivial grammars, and combinator errors are harder to map to "line 57");
`serde` (the formats are not self-describing); regular expressions (another
dependency, and slower on the 240 KB bodies).

### D3. Data model: eager eDRF, lazy DRF, runs never expanded

- `EdrfRecording` is parsed eagerly into per-kind queues: control runs per
  slot, seeds, level records, `C` checkpoints, `E` checkpoints, optional quit
  slot, optional end record, and a `truncated` flag. Research 05 §5.2 notes
  this is equivalent to upstream's lazy reader except that a bad record fails
  before play starts; the specs accept that.
- `DrfRecording` holds the header, the eight maps and the filtered body.
  `DrfCursor { pos, run: Option<(Record, remaining)> }` reads it during play.
  The cursor is per-playback state, replacing upstream's never-reset static
  `rlleft`.
- Control runs are stored as `(value, count)` pairs and consumed by
  decrementing, so `I @4000000000` costs sixteen bytes. This is what makes
  the "no memory in proportion to a repeat count" requirement hold.
- Closed sets are enums: `RecordingKind`, `DrfRecord { direction:
  Option<Direction>, fire: bool }`, `Record::{Input, Level, Seed, Checkpoint,
  RoundEnd, Quit, End}`, `LevelMap::{Inline(Map), Reuse(index)}`.

### D4. The simulation seam: a `Director` supplies, an `Observer` watches

Upstream injects and records through global hooks. The port gives the
simulation two narrow traits, used with static dispatch:

```rust
pub trait Director {
    fn round_seed(&mut self, round: u32) -> u32;
    fn level_map(&mut self, level: u16, plan: PlanIndex) -> LevelMapSource;   // BuiltIn | Recorded(Map) | Mismatch
    fn raw_controls(&mut self, slot: Slot) -> Controls;                       // before direction resolution
    fn unread_slot(&mut self, slot: Slot);                                    // slot not read this tick
    fn control_read(&mut self, digger: DiggerIndex, resolved: Resolved) -> ControlRead;
        // ControlRead::Use(Resolved) | ControlRead::EndOfRecording
    fn quit_requested(&mut self) -> bool;                                     // polled once per frame
}

pub trait Observer {
    fn level_started(&mut self, level: u16, map: &Map);
    fn round_started(&mut self, seed: u32);
    fn slot_controls(&mut self, slot: Slot, controls: Controls);
    fn control_read(&mut self, digger: DiggerIndex, resolved: Resolved);
    fn tick_ended(&mut self, tick: u32, state: &dyn StateView);
    fn round_ended(&mut self, tick: u32, state: &dyn StateView);
    fn quit(&mut self, slot: Slot);
    fn game_ended(&mut self, ticks: u32, state: &dyn StateView);
}
```

`Game::step_with(&mut self, director: &mut impl Director, observer: &mut impl
Observer)` is the general form; the existing `Game::step(controls)` becomes a
thin wrapper using a live director and a no-op observer, so phase 3's tests
and the phase 5/6 interface are unaffected. `()` implements `Observer`;
`(A, B)` implements it by forwarding to both, which is how "play back while
verifying while re-recording" is composed without dynamic dispatch or shared
mutable state.

Mapping to upstream's hook points:

| Upstream hook | Port |
|---|---|
| `playgetrand` / `edrf_getrand` / `roundseed` | `Director::round_seed` |
| `edrf_input` on playback | `Director::raw_controls` |
| `edrf_tick` consuming unread slots | `Director::unread_slot` |
| `playgetdir` in `getdirect` | `Director::control_read` |
| `edrf_level` on playback | `Director::level_map` |
| `edrf_quitting`, `playend → escape` | `Director::quit_requested`, `ControlRead::EndOfRecording` |
| `recputrand`, `recputdir`, `recputeol`, `recputeog` | `Observer` callbacks → `DrfRecorder` |
| `edrf_input`/`edrf_movement`/`edrf_tick`/`edrf_roundend`/`edrf_gameend` when recording | `Observer` callbacks → `EdrfRecorder` |
| checkpoint comparison | `Observer` callbacks → `Verifier` |

If phase 3 delivered a different seam, task group 1 adapts it to this shape
before anything else; the simulation's behaviour does not change. Neither
trait allocates on the hot path: recorders append to buffers they own, which
is outside `Game`'s own no-allocation rule but is kept amortised (run-length
state plus one `String`).

*Alternative rejected:* pre-decoding a recording into a per-tick control
array and calling `step(controls)`. That works for eDRF but not DRF, whose
records are per control read and can only be placed by running the game.
*Alternative rejected:* callbacks as boxed closures stored in `Game` — it
puts borrowed, non-deterministic state inside the simulation value and
defeats `Clone`/snapshotting.

### D5. The state hash is a table-driven module, independent of struct layout

The hash is a wire format shared with 27 files. It is defined in
`state_hash/layout.rs` as an ordered table of entries
`{ block, name, width, repeat, condition }` (for example `Digger /
"bag_push_pause" / I16 / per digger`, `Monster / "sprite_x" / I16 / per slot
/ when body present`). The simulation implements one trait:

```rust
pub trait StateView {
    fn field_cell(&self, index: usize) -> u16;
    fn digger(&self, slot: usize) -> DiggerHashFields;     // plain integers and bools
    fn monster(&self, slot: usize) -> MonsterHashFields;   // body: Option<MonsterBodyFields>
    fn bag(&self, slot: usize) -> BagHashFields;
    // … shared bonus/emerald values, rng state, scores, gauntlet clock
}
```

The `*HashFields` structs are plain data built for the purpose, with every
enum already converted to its wire code by `codes.rs` (exhaustive `match`,
so adding a variant is a compile error there). A single `walk(view, &mut impl
Visitor)` visits `(block, name, value)` in table order. Three visitors use
it: the MD5 sink (the hash), a per-block MD5 sink (trace lines), and a
collector (decoded dump). The hash, the trace and the documentation table in
`docs/recording-formats.md` therefore cannot disagree.

The optional monster part is `Option<MonsterBodyFields>`, not a flag plus
garbage values. Values the original keeps but never changes (`firepressed`,
per-digger `levdone`) are fields of `DiggerHashFields` with a comment naming
the quirk; the simulation returns the constant.

Guards: a golden test of the table itself (names, widths, order, total
lengths 903/908/951 bytes) so that a reorder is a visible diff; unit tests
feeding a hand-built `StateView` and comparing against a byte string written
out in the test.

MD5 comes from the `md-5` crate (pure Rust, already in the porting plan).

*Alternatives rejected:* `#[derive(Hash)]`/`bytemuck` over the simulation's
structs (ties a file format to field order, padding and enum
representation); hashing inside each entity module as upstream does (three
places to keep in step, and no single table to review against research
01 §8).

### D6. Replay driver API

```rust
pub struct ReplayOptions { pub stop_at_first_failure: bool,   // default true
                           pub take_over_after: Option<u32>,
                           pub max_ticks: u32 }               // safety net, default 10_000_000
pub enum Outcome { Verified, Completed, Failed(Failure), Stopped, Truncated, TakenOver }
pub struct ReplayReport { pub outcome: Outcome, pub scores: [i32; 2], pub two_scores: bool,
                          pub level: u16, pub ticks: u32, pub last_good_checkpoint: Option<u32> }

impl Replay {
    pub fn new(recording: &Recording, options: ReplayOptions) -> Result<Replay, RecordingError>;
    pub fn step(&mut self, observer: &mut impl Observer) -> ReplayStep;   // one frame; Running | Finished
    pub fn game(&self) -> &Game;                                          // framebuffer, sound commands
    pub fn finish(self) -> ReplayReport;
}
pub fn verify(bytes: &[u8], options: ReplayOptions) -> Result<ReplayReport, RecordingError>;
impl ReplayReport { pub fn result_line(&self) -> String; pub fn exit_status(&self) -> u8; }
```

`Replay::step` advances exactly one frame, so the desktop app can pace and
present a playback and a test can loop as fast as possible; the driver never
sleeps and never reads a clock. `verify` is `parse` + loop + `finish`.
`Outcome` replaces upstream's five global flags (`playend`, `edrf_failed`,
`edrf_stopped`, `edrf_truncated`, `edrf_checked`); the precedence in the spec
is one `match` in `finish`. Exit statuses are a method on the report so the
mapping is tested in the library and reused by `xtask` now and the binary
later. `RecordingError` maps to status 1, except `BadRecord`/`BadDrfBody`,
which map to 2.

### D7. Configuration isolation by construction

`Replay::new` builds the `GameConfig` from the recording's header with a
function whose only input is the header. There is no parameter through which
a user setting could arrive, and unlimited lives is set to off explicitly.
The driver does not own or see the high-score table. The settings in force
"before" a playback are simply the caller's own `GameConfig`, which the
driver never touches; upstream's save-and-restore of globals has no
counterpart. A test replays a recording once per corner of a deliberately
hostile caller configuration and asserts identical reports.

### D8. Take-over is a director state change

`PlaybackDirector` is an enum-like state machine `Playing { cursor } |
HandedOver { live }`. When the tick count passes `take_over_after` (or on
`request_take_over()`), it switches state; from then on it forwards to the
live director supplied by the caller, the `Verifier` stops comparing and the
outcome becomes `TakenOver`. Recorders are observers and do not notice.
The decision to support this now, rather than leave it to phase 6, is that
it costs one state and three tests here, whereas adding it later would
reopen the driver. The key binding and option that trigger it belong to
`add-desktop-app`.

### D9. Writers

- `DrfRecorder` implements `Observer`: header at construction, seed on
  `round_started`, run-length state on `control_read`, `EOL`/`EOG` on
  `round_ended`/`quit`. It reproduces upstream's 999 run cap and 60-column
  wrap so that files look familiar, but grows without limit (upstream's
  buffer silently wrapped at 256 KB — fixed).
- `EdrfRecorder` implements `Observer`: 32-tick chunks per slot, each entry
  `Read(bits) | Unread`; chunk form chosen as in research 05 §5.2 (no slot
  list when all slots agree, `Unread` agreeing with anything). It keeps the
  list of distinct maps for `L n m`. Finished lines accumulate in a `String`
  that the caller drains with `take_pending()`, which is how a frontend
  streams to disk and a killed game leaves a playable prefix.
- During DRF playback `EdrfRecorder` takes a slot's bits from
  `Observer::control_read` (direction and fire the DRF supplied) and 0
  otherwise, instead of from `slot_controls`; this is upstream's
  `edrf_movement` overwrite and is selected by the recorder's constructor,
  not by a global.
- Version line written by the port: `RS DIG 20261111` — two fields and an
  8-digit date at byte offset 7, later than 19981125. The date is a constant
  naming the recording-format revision, not the build date, so output is
  reproducible.
- Conversion output is compared with upstream's 23 twins **semantically**:
  both files are parsed and their models compared (version line excluded).
  The writer nevertheless follows upstream's run-splitting rule (`Unread`
  and 0 are different raw values that both print `@`), and a second,
  informational test reports how many twins are also byte-identical after
  replacing line 2. Byte identity is not a gate because research 05 §10.4
  could not confirm it.

### D10. Developer commands (`xtask`)

| Command | Purpose |
|---|---|
| `cargo xtask import-recordings --from <upstream checkout> [--allow-other-commit]` | Copy `tests/data/*.drf`, `*.edrf` and `tests/results/*.out` into `tests/recordings/{data,results}/`, write `MANIFEST.sha256` and check the checkout's `HEAD` is `638125e…` (read from `.git`, no `git` subprocess needed) |
| `cargo xtask verify <file>` | The headless verify run: result line on stdout, diagnostics on stderr, exit status 0/1/2/3/4 |
| `cargo xtask conformance [--tier quick\|full] [--filter <substr>] [--jobs N]` | Verify the corpus, one line per recording, summary table, non-zero exit if any fails; writes `target/conformance/report.md` |
| `cargo xtask convert <in> <out>` | Play `<in>` and record it in the format selected by `<out>`'s suffix |
| `cargo xtask dump-state <file> --from <tick> --to <tick> [--fields] [--out <path>]` | Per-tick trace lines; with `--fields` the decoded value of every hashed field |
| `cargo xtask cut <file.edrf> --at <tick> --out <path>` | Write an eDRF holding the same game up to that tick followed by `Q 0` |
| `cargo xtask bisect <file.edrf> [--oracle <command>]` | Report the failing 256-tick window; with an oracle, binary-search the first diverging tick inside it (D12) |

`xtask` owns all file and process I/O. It parses its own arguments by hand
or with `clap` as phase 1 set it up; nothing here constrains the game
binary's command line.

### D11. Corpus layout, licence and test wiring

```
tests/recordings/
  README.md          origin (sobomax/digger @ 638125eecc2a…), GPL-2.0-or-later,
                     how to re-import, what each mode covers, known gaps
  MANIFEST.sha256    one line per file
  data/              23 *.drf + 27 *.edrf, byte-for-byte (CRLF file kept as is)
  results/           27 *.out
```

A `.gitattributes` entry marks `tests/recordings/**` as `-text` so that no
checkout converts line ends and the manifest stays valid on Windows.

Integration tests in `crates/digger-core/tests/`:

- `conformance.rs` — one `#[test]` per recording, generated by a declarative
  macro from an explicit list (`recording!(sobodig, drf, quick)`); the 32
  full-tier-only files carry `#[ignore = "full tier"]`. A separate test
  compares the list with the directory so a file can be neither forgotten
  nor silently dropped. No build script and no test-harness dependency.
- `corpus_integrity.rs` — SHA-256 of every file against the manifest (the
  `sha2` crate as a dev-dependency), counts 23/27/27.
- `codec_roundtrip.rs`, `conversion_twins.rs`, `malformed_input.rs`,
  `isolation.rs`, `takeover.rs`.

CI: the existing test job runs `cargo test --workspace` (quick tier, debug
build, overflow checks on — useful, since a wrapping operation that was
written as plain arithmetic panics here). A new `conformance` job runs
`cargo test --release -p digger-core --test conformance -- --include-ignored`
on Linux, and uploads `target/conformance/report.md` from `cargo xtask
conformance --tier full`. The quick tier also runs on Windows and macOS
through the existing matrix.

*Alternative rejected:* `datatest-stable`/`libtest-mimic` for per-file tests
— a custom harness for 50 files that a 50-line macro list covers.

### D12. Divergence triage

The suite will fail at first; the design budgets for that.

1. **Order of attack.** eDRF before DRF, short before long, one-digger
   before two-digger before gauntlet: `dror0`, `sobodig`, `thevoid(s)`,
   `ramon`, `muu27700`, then `2test01..03`, `2riptest01`, `gauntlet01`, then
   the 16 long games. A DRF is attempted only after its eDRF twin passes;
   after that a DRF failure can only come from control-read placement, the
   two-digger map patch or DRF-specific end handling.
2. **Localise to a window.** The failure report names the first failing
   checkpoint and the last matching one: at most 256 ticks, usually fewer
   because round-end checkpoints fall in between.
3. **Localise inside the window, port only.** `dump-state --fields` over the
   window gives every hashed field per tick. Since the state at the window's
   start is known to be correct, the fault is an event in the window; the
   trace is read against the research reports (which events occur: deaths,
   bag falls, bonus mode, spawn, level end) and candidate fixes are compared
   by diffing traces before and after. First diverging *block* between two
   candidate traces narrows it to digger, monster, bag or playfield logic.
4. **Localise inside the window, with an oracle (optional).** A developer
   who has an upstream binary built from `638125e` elsewhere may pass it as
   `--oracle`. `bisect` then binary-searches tick N in the window: `cut`
   the recording at N with `Q 0`, let the oracle re-record it (`/Q /S:0
   /R:out.edrf /E:cut.edrf`, the `quit_rec` recipe of research 05 §7.4) and
   read the `Z N <hash>` it writes; compare with the port's own `Z` for the
   same cut. Eight runs find the first tick whose post-quit state differs.
   The oracle is an external black box used for measurement: it is never
   linked, vendored, required by CI or read as source.
5. **Fix and record.** Every fix lands in `digger-core` with a scenario or
   unit test that fails without it. If the research report was wrong or
   silent, `docs/compat-quirks.md` gets an entry (class, description,
   recording and tick that exposed it, test name). If a simulation spec
   requirement was wrong, a MODIFIED delta is added to this change.
6. **Track.** `docs/conformance.md` holds a table with one row per recording
   (50 rows): status, first failing tick, cause, fixing commit. The rows are
   mirrored as checkboxes in tasks.md group 12.

### D13. Staying idiomatic

| Original | Port |
|---|---|
| Globals `playing`, `playend`, `escape`, `edrf_failed`, `edrf_stopped`, `edrf_truncated`, `edrf_checked`, `playtaken` | `Outcome` enum computed once; `ControlRead::EndOfRecording`; director state enum |
| Hooks called from inside game code reading global buffers | `Director`/`Observer` traits passed to `step_with`, borrowed for the call only |
| Static cursor `plp`, static `rlleft` never reset | `DrfCursor` owned by one playback |
| Direction as int with −1, fire encoded in letter case | `DrfRecord { direction: Option<Direction>, fire: bool }` |
| Control bits as `int`, `0x80` meaning "unread" | `Controls` newtype over 5 bits; `SlotSample::{Read(Controls), Unread}` |
| `sscanf`/`atoi` with silent garbage | checked parsing into `Result`, location carried in the error |
| Fixed 256 KB buffer that wraps | growable `String`, drained by the caller |
| `sizeof`-driven hashing of struct members | explicit table and `to_le_bytes` |
| Exit codes scattered through `main` | `ReplayReport::exit_status` |

Wrapping or truncating conversions needed by the hash (`i32` score, `u32`
clock) are done by the simulation with its named helpers; the hash module
only encodes what it is given.

## Compat quirks this change must reproduce

Each gets an entry in `docs/compat-quirks.md`, a named constant or function
in code, and the listed test.

| # | Quirk (class) | Isolated in | Test |
|---|---|---|---|
| R1 | One DRF record per control read, not per tick; none while dead, invisible or in bag-push pause; two diggers interleave in one stream (simulation) | `Director::control_read` call site in the digger update; `DrfCursor` | `drf_record_consumed_only_on_control_read`, `drf_two_diggers_interleave` |
| R2 | A control read that finds `E`/end of data ends the game as quit, but the tick completes and is counted (simulation) | `ControlRead::EndOfRecording` handling | `drf_end_marker_counts_the_tick` |
| R3 | On a DRF tick without a control read the fire input for the next tick is "not pressed" (simulation) | playback director's raw controls are all-zero during DRF playback | `drf_fire_does_not_linger_over_pause` |
| R4 | eDRF injects raw bits before direction resolution; resolver state resets each round (simulation) | `Director::raw_controls` | `edrf_controls_pass_through_resolver` |
| R5 | A slot not read on a tick still uses up one eDRF value (simulation) | `Director::unread_slot` | `edrf_unread_slot_consumes_value` |
| R6 | Two-digger `H` patch on plans 3 and 4 applies live and for DRF, not for eDRF maps (simulation) | `LevelMapSource::Recorded` bypasses the patch | `two_digger_patch_drf_only` |
| R7 | Short DRF map rows and non-map bytes mean earth (upstream stores a NUL) (format) | `drf.rs` map reader | `drf_short_map_row_is_earth` |
| R8 | DRF body ignores every byte below 0x20, including inside seeds and counts (format) | body filter in `drf.rs` | `drf_body_ignores_control_bytes` |
| R9 | `U` in the mode line is parsed and ignored; playback never has unlimited lives (simulation) | `header.rs`, `GameConfig` from header | `mode_u_ignored`, `isolation_unlimited_lives` |
| R10 | Hash covers both digger slots, all six monster slots and all seven bag slots regardless of use, includes never-changing flags, and the monster body part only while a body exists (format) | `state_hash/layout.rs` | `layout_golden`, `monster_block_is_variable_length` |
| R11 | Round-end hash is taken after bag clean-up, playfield save and monster removal, and appends the tick (format) | `Observer::round_ended` call site | `round_end_hash_after_cleanup` |
| R12 | Tick checkpoints at multiples of 256 of the post-increment count; a round ending on such a tick writes `C` then `E` (format) | `EdrfRecorder`, `Verifier` | `checkpoint_then_round_end_order` |
| R13 | The wrapped gauntlet clock is hashed as its unsigned 32-bit value (simulation) | simulation helper + `StateView` | `gauntlet_clock_wrap_hashed` |
| R14 | eDRF `I` records: no slot list when slots agree with "unread" as wildcard; unread and 0 split runs but both print `@` (format) | `EdrfRecorder` chunk writer | `edrf_chunk_forms`, twin conversion test |
| R15 | DRF writer: runs capped at 999, line wrap when the column counter reaches 60, seed as `%08X` (format) | `DrfRecorder` | `drf_writer_run_cap_and_wrap` |
| R16 | Level records are produced only for participating players; in a one-player game the unused player two's level set-up produces none (format) | `Observer::level_started` call site | `one_player_game_has_single_level_record` |

Deliberate deviations (class "presentation or usability bug" or "dropped
feature"), each also listed in `docs/compat-quirks.md`:

- Pre-1998 "kludge" recordings are refused instead of emulated.
- DRF reading validates (bad seed digit, unknown run letter, out-of-step at
  round end) where upstream reads garbage silently. If any upstream recording
  turns out to rely on the lenient behaviour, the recording wins: the check
  is relaxed and the case documented.
- No 256 KB recording limit; no 80/511-byte line limits; LF line ends on
  every platform.
- Verification stops at the first failure by default instead of playing on.
- A bad eDRF record fails before play starts, not when reached.

## Risks / Trade-offs

- [The suite exposes many simulation defects and triage dominates the
  change] → It is planned for: a dedicated task group, ordering from simplest
  recording to hardest, per-recording tracking, and tooling built before the
  first full run. Fixes found on short recordings usually clear long ones.
- [The seam assumed in D4 differs from what phase 3 built] → Group 1 adapts
  the simulation to the seam first and re-runs phase 3's tests; the seam is
  additive (`step` keeps working).
- [The "monster body exists" condition of the hash is under-specified in the
  research: it does not say exactly when a body is released] → Treated as the
  first suspect when only the monster block length differs; the trace prints
  the stream length per block so this shows at a glance; the answer is fixed
  by the recordings and noted in `docs/compat-quirks.md`.
- [A divergence cannot be localised inside a window without an oracle] →
  Round-end checkpoints usually cut windows well below 256 ticks; the decoded
  dump shows every hashed field; the optional oracle bisect exists for the
  stubborn cases.
- [Stricter DRF validation rejects a genuine old recording] → All 23
  upstream DRFs are the acceptance test for the strictness; "recording wins".
- [Full tier is too slow in debug builds] → Only the quick tier runs in
  debug; the full tier runs in release with a 10-minute budget, measured and
  recorded in `docs/conformance.md`.
- [Corpus files altered by line-end conversion on checkout] →
  `.gitattributes -text` plus the manifest test.
- [Expected results describe upstream at one commit] → The import command
  checks the commit; the README records it.
- [Upstream never recorded an alternating two-player game or a non-default
  start level] → The codecs and replay are unit-tested for `2` and `I<n>`;
  port-made recordings of both modes are added as regression fixtures under
  `tests/recordings/port/` once the upstream 50 pass, clearly marked as not
  upstream-verified.

## Migration Plan

Additive. No existing behaviour or file format changes; `Game::step` keeps
its signature. Rollback is reverting the change's commits; the corpus and
the new modules have no dependents until `add-desktop-app`.

## Open Questions

- Are the port's converted eDRFs byte-identical to upstream's twins once the
  version line is normalised? Measured by an informational test; the answer
  changes nothing above.
- How long does the full tier take on CI hardware? If well under a minute, a
  later change may drop the tiering; the 10-minute bound stands either way.
