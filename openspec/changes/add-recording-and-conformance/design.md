# Design

## Context

See proposal.md for motivation. The state this design starts from:

- `add-simulation-core` has delivered `digger-core` with `Game::new(config,
  base_seed)`, `Game::step(&FrameInput)` and `Game::step_observed(&FrameInput,
  &mut dyn Observer)`: a deterministic state machine advanced one frame at a
  time, with per-round seeds, raw controls per input slot, the direction
  resolver, the one-tick fire latch, the `Observer` hooks, the read-only
  `StateView` and its additions to `docs/compat-quirks.md`. Nothing has yet
  checked it against the original.
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
    mod.rs        HashFields from the simulation's StateView, walk(), hash(), hash_at_round_end()
    layout.rs     the field table (names, widths, order, conditions)
    codes.rs      enum -> wire-code mappings
    view.rs       the *HashFields values built from StateView
  replay/
    mod.rs        Replay, ReplayOptions, ReplayReport, Outcome, verify()
    playback.rs   PlaybackSource (DRF and eDRF variants), ReplayObserver, take-over
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
- `DrfRecording` holds the header, the eight maps and the body bytes.
  `DrfCursor { pos, run: Option<(Record, remaining)> }` reads it during play,
  skipping every byte below 0x20 as it reads; this is the same as reading a
  filtered copy, and lets errors give the byte offset in the file. A
  separate `OutOfStep` value (run repeats pending, or no `EOL`) is returned
  by `end_round` for the playback to report with its tick number.
  The cursor is per-playback state, replacing upstream's never-reset static
  `rlleft`.
- Control runs are stored as `(value, count)` pairs and consumed by
  decrementing, so `I @4000000000` costs sixteen bytes. This is what makes
  the "no memory in proportion to a repeat count" requirement hold.
- Closed sets are enums: `RecordingKind`, `DrfRecord { direction:
  Option<Direction>, fire: bool }`, `Record::{Input, Level, Seed, Checkpoint,
  RoundEnd, Quit, End}`, `LevelMap::{Inline(Map), Reuse(index)}`.

### D4. The simulation seam: phase 3's `Observer`

Upstream injects and records through global hooks. `add-simulation-core`
already provides the seam for this: `Game::step_observed(&FrameInput, &mut
dyn Observer)`, whose hooks are called at the points the original consumed
or produced each value and may replace it (its design D8):

```rust
pub trait Observer {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32;
    fn level_plan(&mut self, player: PlayerId, level: LevelNumber, layout: &LevelLayout) -> Option<LevelLayout>;
    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls;   // once per slot per tick
    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement;           // once per control read
    fn tick_end(&mut self, tick: u32, state: StateView<'_>);
    fn round_end(&mut self, state: StateView<'_>);                                // after clean-up
    fn game_end(&mut self, state: StateView<'_>);
}
```

This change adds no trait and no entry point to the simulation. It supplies
one implementation, `ReplayObserver`, owned by `Replay`. It holds the
`PlaybackSource`, an optional `Verifier`, an optional recorder and an
optional trace sink as plain fields and handles each hook in a fixed order:
the playback source replaces the value, then the recorder, verifier and
trace see the value actually used. That is how "play back while verifying
while re-recording" is composed, with no shared mutable state. A live game
being recorded uses the same struct without a playback source.

Mapping to upstream's hook points:

| Upstream hook | Port |
|---|---|
| `playgetrand` / `edrf_getrand` / `roundseed` | `Observer::round_seed` returns the recorded seed |
| `edrf_input` on playback | `Observer::controls` returns the recorded raw controls |
| `edrf_tick` consuming unread slots | the same hook: it is called for every slot on every tick, read or not |
| `playgetdir` in `getdirect` | `Observer::movement` returns the DRF record's direction and fire |
| `edrf_level` on playback | `Observer::level_plan` returns the recorded layout, or records a level-number mismatch |
| `edrf_quitting`, `playend → escape` | the source sets an end flag; `Replay::step` puts `quit` into the next frame's `FrameInput` |
| `recputrand`, `recputdir`, `recputeol`, `recputeog` | the same hooks, observed → `DrfRecorder` |
| `edrf_input`/`edrf_movement`/`edrf_tick`/`edrf_roundend`/`edrf_gameend` when recording | the same hooks plus `tick_end`/`round_end`/`game_end` → `EdrfRecorder` (a slot counts as read on every tick on which a digger plays from it, as upstream's `readdirect` runs for every digger in play on every tick, dead, paused or not; corrected in task 8.2 from "when `movement` was called for it", which `edrf.c` does not support) |
| checkpoint comparison | `tick_end`/`round_end`/`game_end` → `Verifier` |

Seeds are `i32` in the simulation; the codecs convert from and to the files'
unsigned hex. Recorders append to buffers they own, which is outside
`Game`'s own no-allocation rule but is kept amortised (run-length state plus
one `String`). Task group 1 checks each row against the simulation as
merged; if a hook is not called at the point a row needs, that is a
simulation defect fixed there with a test, not a second seam.

*Alternative rejected:* pre-decoding a recording into a per-tick control
array and calling `step`. That works for eDRF but not DRF, whose records are
per control read and can only be placed by running the game.
*Alternative rejected:* separate "director" and "observer" traits with a
second `step` entry point — it duplicates what phase 3's hooks already do
and gives the simulation two seams to keep in step.
*Alternative rejected:* callbacks as boxed closures stored in `Game` — it
puts borrowed, non-deterministic state inside the simulation value and
defeats `Clone`/snapshotting.

### D5. The state hash is a table-driven module, independent of struct layout

The hash is a wire format shared with 27 files. It is defined in
`state_hash/layout.rs` as an ordered table of entries
`{ block, name, width, repeat, condition }` (for example `Digger /
"bag_push_pause" / I16 / per digger`, `Monster / "sprite_x" / I16 / per slot
/ when body present`). Its input is phase 3's read-only `StateView<'_>`
(typed getters for every hashed variable). One function per block turns the
view into plain data:

```rust
fn digger_fields(view: StateView<'_>, slot: usize) -> DiggerHashFields;     // plain integers and bools
fn monster_fields(view: StateView<'_>, slot: usize) -> MonsterHashFields;   // body: Option<MonsterBodyFields>
fn bag_fields(view: StateView<'_>, slot: usize) -> BagHashFields;
// … field cells, shared bonus/emerald values, rng state, scores, gauntlet clock
```

The `*HashFields` structs are plain data built for the purpose, with every
enum converted to its wire code by `codes.rs` (exhaustive `match`, so adding
a variant is a compile error there). A single `walk(view, &mut impl
Visitor)` visits `(block, name, value)` in table order. Three visitors use
it: the MD5 sink (the hash), a per-block MD5 sink (trace lines), and a
collector (decoded dump). The hash, the trace and the documentation table in
`docs/recording-formats.md` therefore cannot disagree.

The optional monster part is `Option<MonsterBodyFields>`, not a flag plus
garbage values. It is `Some` from the tick a monster is spawned into the
slot until the round's monster reset at the next round start: a kill marks
the upstream object dead and erases its sprite but does not release it
(research 02 §2 and §4.7; slots are cleared only at round start, §4.2), and
research 01 §8 hashes the extra values whenever the object exists. The
condition is `StateView`'s "spawned this round" value for the slot, as
settled in `add-simulation-core` (its design, Open Questions). Values the original keeps but never changes (`firepressed`,
per-digger `levdone`) are fields of `DiggerHashFields` with a comment naming
the quirk; the simulation returns the constant.

Guards: a golden test of the table itself (names, widths, order, total
lengths 903/908/951 bytes) so that a reorder is a visible diff; unit tests
feeding hand-built `*HashFields` values and comparing against a byte string written
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
    pub fn step(&mut self, input: &FrameInput, extra: &mut dyn Observer) -> ReplayStep;   // one frame; Running | Finished
    pub fn game(&self) -> &Game;                                          // framebuffer, sound commands
    pub fn finish(self) -> ReplayReport;
}
pub fn verify(bytes: &[u8], options: ReplayOptions) -> Result<ReplayReport, RecordingError>;
impl ReplayReport { pub fn result_line(&self) -> String; pub fn exit_status(&self) -> u8; }
```

`input` carries the caller's pause and quit requests and, after a take-over,
the live controls; before that its controls are ignored. `extra` is an
additional observer called after the built-in ones (`()`-like no-op for
plain replays). `Replay::step` advances exactly one frame, so the desktop app can pace and
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

### D8. Take-over is a playback-source state change

`PlaybackSource` is a state machine `Playing { cursor } | HandedOver`. When
the tick count passes `take_over_after` (or on `request_take_over()`), it
switches state; from then on the `ReplayObserver` hooks return the
simulation's own values (derived seeds, built-in or configured levels, the
live controls of the caller's `FrameInput`), the `Verifier` stops comparing
and the outcome becomes `TakenOver`. Recorders only observe; the one that
is told is an eDRF recorder converting a DRF playback, which switches from
the DRF's records to the live raw controls (otherwise the live part would
be recorded as resolved directions, not as the bits that were pressed).
The `max_ticks` safety net applies only while the recording plays. A
request is refused once the game has finished, the recording has run out
or the playback is being stopped. No playback, taken over or not, is
eligible for the high-score table; the report says so for the frontend. The decision to support this now, rather than leave it to phase 6,
is that it costs one state and three tests here, whereas adding it later
would reopen the driver. `add-desktop-app` exposes it as the option
`--take-over-at <tick>` (alias `/T:n`); there is no take-over key.

### D9. Writers

- `DrfRecorder` is fed by `ReplayObserver`: header at construction, seed on
  `round_seed`, run-length state on `movement`, `EOL` on `round_end`, `EOG`
  on `game_end` of a quit game. It reproduces upstream's 999 run cap and 60-column
  wrap so that files look familiar, but grows without limit (upstream's
  buffer silently wrapped at 256 KB — fixed).
- `EdrfRecorder` is fed the same way: 32-tick chunks per slot, each entry
  `Read(bits) | Unread`; chunk form chosen as in research 05 §5.2 (no slot
  list when all slots agree, `Unread` agreeing with anything). It keeps the
  list of distinct maps for `L n m`. Finished lines accumulate in a `String`
  that the caller drains with `take_pending()`, which is how a frontend
  streams to disk and a killed game leaves a playable prefix.
- During DRF playback `EdrfRecorder` takes a slot's bits from
  `Observer::movement` (direction and fire the DRF supplied) and 0
  otherwise, instead of from `controls`; this is upstream's
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
| `cargo xtask bisect <file.edrf>` | Report the failing 256-tick window, the last matching checkpoint, the round-end checkpoints inside the window and the `dump-state` command to run (D12) |
| `cargo xtask trace-diff <a> <b>` | First differing tick and block between two `dump-state` outputs |

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
3. **Localise inside the window.** `dump-state --fields` over the
   window gives every hashed field per tick. Since the state at the window's
   start is known to be correct, the fault is an event in the window; the
   trace is read against the research reports (which events occur: deaths,
   bag falls, bonus mode, spawn, level end) and candidate fixes are compared
   by diffing traces before and after. First diverging *block* between two
   candidate traces narrows it to digger, monster, bag or playfield logic.
   No upstream binary is built or run at any point (porting plan, "No C"):
   the only evidence is the recording's own checkpoints, the port's per-tick
   dumps and the research reports. `cut` shortens a recording to the window
   so that a candidate fix is re-checked in milliseconds.
4. **Fix and record.** Every fix lands in `digger-core` with a scenario or
   unit test that fails without it. If the research report was wrong or
   silent, `docs/compat-quirks.md` gets an entry (class, description,
   recording and tick that exposed it, test name). If a simulation spec
   requirement was wrong, a MODIFIED delta is added to this change.
5. **Track.** `docs/conformance.md` holds a table with one row per recording
   (50 rows): status, first failing tick, cause, fixing commit. The rows are
   mirrored as checkboxes in tasks.md group 12.

### D13. Staying idiomatic

| Original | Port |
|---|---|
| Globals `playing`, `playend`, `escape`, `edrf_failed`, `edrf_stopped`, `edrf_truncated`, `edrf_checked`, `playtaken` | `Outcome` enum computed once; an end flag in the playback source; `PlaybackSource` state enum |
| Hooks called from inside game code reading global buffers | One `Observer` implementation passed to `step_observed`, borrowed for the call only |
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
| P1 | One DRF record per control read, not per tick; none while dead, invisible or in bag-push pause; two diggers interleave in one stream (simulation) | `Observer::movement` call site in the digger update; `DrfCursor` | `drf_record_consumed_only_on_control_read`, `drf_two_diggers_interleave` |
| P2 | A control read that finds `E`/end of data ends the game as quit, but the tick completes and is counted (simulation) | end flag set in `movement`, `quit` on the next frame | `drf_end_marker_counts_the_tick` |
| P3 | On a DRF tick without a control read the fire input for the next tick is "not pressed" (simulation) | the playback source returns all-zero raw controls from `controls` during DRF playback | `drf_fire_does_not_linger_over_pause` |
| P4 | eDRF injects raw bits before direction resolution; resolver state resets each round (simulation) | `Observer::controls` | `edrf_controls_pass_through_resolver` |
| P5 | A slot not read on a tick still uses up one eDRF value (simulation) | `Observer::controls`, called for every slot on every tick | `edrf_unread_slot_consumes_value` |
| P6 | Two-digger `H` patch on plans 3 and 4 applies live and for DRF, not for eDRF maps (simulation) | a layout returned from `Observer::level_plan` is used verbatim | `two_digger_patch_drf_only` |
| P7 | Short DRF map rows and non-map bytes mean earth (upstream stores a NUL) (format) | `drf.rs` map reader | `drf_short_map_row_is_earth` |
| P8 | DRF body ignores every byte below 0x20, including inside seeds and counts (format) | body filter in `drf.rs` | `drf_body_ignores_control_bytes` |
| P9 | `U` in the mode line is parsed and ignored; playback never has unlimited lives (simulation) | `header.rs`, `GameConfig` from header | `mode_u_ignored`, `isolation_unlimited_lives` |
| P10 | Hash covers both digger slots, all six monster slots and all seven bag slots regardless of use, includes never-changing flags, and the monster body part only while a body exists (format) | `state_hash/layout.rs` | `layout_golden`, `monster_block_is_variable_length` |
| P11 | Round-end hash is taken after bag clean-up, playfield save and monster removal, and appends the tick (format) | `Observer::round_end` call site | `round_end_hash_after_cleanup` |
| P12 | Tick checkpoints at multiples of 256 of the post-increment count; a round ending on such a tick writes `C` then `E` (format) | `EdrfRecorder`, `Verifier` | `checkpoint_then_round_end_order` |
| P13 | The wrapped gauntlet clock is hashed as its unsigned 32-bit value (simulation) | simulation helper + `StateView` | `gauntlet_clock_wrap_hashed` |
| P14 | eDRF `I` records: no slot list when slots agree with "unread" as wildcard; unread and 0 split runs but both print `@` (format) | `EdrfRecorder` chunk writer | `edrf_chunk_forms`, twin conversion test |
| P15 | DRF writer: runs capped at 999, line wrap when the column counter reaches 60, seed as `%08X` (format) | `DrfRecorder` | `drf_writer_run_cap_and_wrap` |
| P16 | Level records are produced only for participating players; in a one-player game the unused player two's level set-up produces none (format) | `Observer::level_plan` call site | `one_player_game_has_single_level_record` |

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
- [A hook of phase 3's `Observer` is not called exactly where a recording
  needs it] → Group 1 maps every D4 row onto the simulation as merged and
  fixes any gap in the simulation with a test before the codecs are written.
- [The "monster body exists" condition of the hash is derived from the
  research, not observed: body present from spawn until the round's monster
  reset (D5)] → Treated as the first suspect when only the monster block
  length differs; the trace prints the stream length per block so this shows
  at a glance; the first round-end checkpoint of `dror0.edrf` confirms or
  corrects it, and a correction is noted in `docs/compat-quirks.md`.
- [A divergence is hard to localise inside a window, and no upstream binary
  may be run for comparison] → Round-end checkpoints usually cut windows
  well below 256 ticks; the decoded dump shows every hashed field per tick;
  `cut` and `trace-diff` make comparing candidate fixes cheap.
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
  changes nothing above. *Answered in task 8.6:* yes, 23 of 23
  (`twin_byte_identity_full` in `conversion_twins.rs`, release build).
- How long does the full tier take on CI hardware? If well under a minute, a
  later change may drop the tiering; the 10-minute bound stands either way.
