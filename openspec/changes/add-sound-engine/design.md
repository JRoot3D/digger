# Design

## Context

See `proposal.md` for motivation and `specs/sound/spec.md` for the behaviour contract. This document decides how the crate `digger-sound` is built.

What exists when this change starts:

- `digger-core` (from `add-simulation-core`) steps the game one frame at a time and exposes the sound commands produced by each frame (`Game::drain_sound()`). It never reads anything back from sound.
- `xtask import-assets` (from `add-assets-and-renderer`) reads upstream's C data tables as data and writes blobs plus a generated index into `assets/`.
- There is no audio device code. `add-desktop-app` will open a `cpal` stream and call into this crate from the stream callback.

What upstream does that shapes the design (research 04):

- A command queue feeds an audio-thread state machine that still thinks in PC timer divisors and runs every 606th sample. Twelve effect updaters run in a fixed order and overwrite one shared variable (`t2val`); the last writer wins. Music writes another (`t0val`). Edge-triggered flags (`soundt0flag`, `spkrmode`, `wavetype`) decide which of two generator bands is unmuted.
- The generator is floating-point, takes a spinlock per sample, and several backend variables are written from both threads without synchronisation.
- Two game-thread loops block in real time until the audio thread acknowledges the end of the level-done jingle and of the dirge.

Constraints: no C and no C reference harness; no `unsafe`; no allocation, locking or system calls on the audio thread; fidelity is audible (tick-level), not sample-identical.

## Goals / Non-Goals

**Goals:**

- A library with one job: commands in, `i16` mono samples out, usable from a real-time callback and from a plain test with no device.
- Every number in the spec is checkable by a test that never looks at samples (tick traces).
- Sample output that is bit-identical on every platform with the filter off, so golden hashes are stable.
- No shared mutable state between threads other than two lock-free queues and a few atomics.

**Non-Goals:**

- Opening devices, choosing buffer sizes, converting to `f32` or stereo (`add-desktop-app`).
- Reproducing upstream's generator arithmetic, band muting flags, ack id sets or spinlocks.
- Band-limited synthesis. The square wave is deliberately naive, as upstream's is.
- Persisting settings or binding keys.

## Decisions

### D1. Crate and module layout

```
crates/digger-sound/
  src/
    lib.rs        crate docs, #![forbid(unsafe_code)], re-exports
    units.rs      Divisor, Hertz, SoundTicks, SampleRate, Level, Volume newtypes
    command.rs    re-export of digger_core::SoundCommand; SoundControl; SoundEvent
    config.rs     SoundConfig, ConfigError
    noise.rs      NoiseRng (sound-only LCG)
    effects/
      mod.rs      Effects: all effect slots, tick() in priority order
      fall.rs wobble.rs breaking.rs fire.rs explode.rs bonus.rs
      emerald.rs (click + scale) gold.rs eat.rs death.rs extra_life.rs
    jingle.rs     LevelDone state machine and its note table
    music/
      mod.rs      MusicPlayer, Tune parameters, envelope
      tables.rs   decoding of assets/music.bin into TuneTables
    voice.rs      Voice: what sounds on one tick; TickTrace
    osc.rs        SquareOsc (integer phase accumulator), Gate
    filter.rs     FirstOrder, OutputFilter (HP 1 kHz then LP 4 kHz)
    synth.rs      Synth: apply(), tick(), render(); owns all of the above
    link.rs       sound_link(): SoundHandle (game side) + AudioSource (audio side)
    driver.rs     ManualDriver (tests, xtask), NullDriver (no device)
  tests/
    effects_trace.rs  music_trace.rs  switches.rs  golden.rs  generator_props.rs
    golden/hashes.toml
```

`Synth` is the pure core and has no queue in it: `apply(&mut self, input)`, `tick(&mut self) -> TickTrace`, `render(&mut self, out: &mut [i16])`. `link.rs` is the only module that knows about threads.

Alternative rejected: one `sound_backend.rs` mirroring upstream's file. It would carry over the shared-variable style this port exists to remove.

### D2. Commands, controls and events

Three closed enums; none carries an acknowledgement id.

`SoundCommand` is what the simulation emits. It is defined in `digger-core` by `add-simulation-core` and re-exported here; `digger-sound` depends on `digger-core` for this type only. The required variants, and the spec names they correspond to:

| Variant | Spec name | Upstream call |
|---|---|---|
| `StopAll` | stop-all | `soundstop` |
| `FallStart`, `FallStop` | fall-start, fall-stop | `soundfall`, `soundfalloff` |
| `WobbleStart`, `WobbleStop` | wobble-start, wobble-stop | `soundwobble`, `soundwobbleoff` |
| `Break` | break | `soundbreak` |
| `Fire(FireballId)`, `FireStop(FireballId)` | fire-start, fire-stop | `soundfire`, `soundfireoff` |
| `Explode(FireballId)` | explode | `soundexplode` |
| `BonusPulseStart`, `BonusPulseStop` | bonus-start, bonus-stop | `soundbonus`, `soundbonusoff` |
| `EmeraldClick` | emerald-click | `soundem` |
| `EmeraldNote(EmeraldStep)` | emerald | `soundemerald` |
| `Gold` | gold | `soundgold` |
| `EatMonster` | eat-monster | `soundeatm` |
| `DiggerDeath` | death | `soundddie` |
| `ExtraLife` | extra-life | `sound1up` |
| `LevelDone` | level-done | `soundlevdone` |
| `Music { tune: Tune, tempo: Tempo }` | play-music | `music`, `musicwithack`, `musicrefresh` |
| `MusicOff` | stop-music | `musicoff` |
| `Pause`, `Resume` | pause, resume | `soundpause`, `soundpauseoff` |

Supporting types: `FireballId` (`Zero`, `One`, or a newtype over the digger index that cannot hold other values), `EmeraldStep` (0..=7, constructed with a checked function, so upstream's unchecked table index cannot occur), `Tune { Main, LastLife, Bonus, Dirge }` (the spec's normal, battle, bonus and dirge tunes), `Tempo { Standard, Quick }` (`Quick` is upstream's `dfac = 0.7`; a two-valued enum replaces the `f64` argument because those are the only two values ever passed, and it keeps the command `Copy + Eq` and free of floats).

The simulation chooses the tune (`LastLife` on the last life) because it owns the lives count; upstream's `music_request_to_tune` and `musicrefresh` logic therefore lives in `digger-core`. The names in this table are those of `add-simulation-core` (its design D8), which owns the enum; this change does not rename anything in `digger-core`, and the first task group only checks that the table still matches. `digger-core` declares `SoundCommand` `#[non_exhaustive]`, so a match on it in `digger-sound` needs a wildcard arm; the check (`command_boundary_is_complete`) therefore names every variant in a match and asserts that a list of all 22 maps to 22 distinct spec names. `FireballId` and `EmeraldStep` are `digger-core`'s checked index newtypes (`new(n) -> Option`, `ALL`), holding 0..=1 and 0..=7.

`SoundControl` is what the frontend sends: `ToggleSound`, `ToggleMusic`, `SetVolume(Volume)`, `SetFilter(bool)`. Pause is not among them: the pause phase belongs to the simulation, which emits `Pause` and `Resume` itself, so there is exactly one source for the pause state.

`SoundEvent` is what the engine reports: `LevelDoneFinished`, `DirgeFinished`.

The queue item is `enum Input { Game(SoundCommand), Control(SoundControl) }`.

Alternative rejected: the toggles inside `SoundCommand`. They would then appear in the simulation's deterministic output although they do not come from it.

### D3. Effects are state machines that offer a divisor

Upstream's "flag plus loose statics, updaters overwrite `t2val` in call order" becomes:

```rust
pub struct Effects {
    emerald: Option<EmeraldScale>, wobble: Option<Wobble>, death: Option<Death>,
    breaking: Option<Break>, gold: Option<Gold>, click: Option<Click>,
    explode: [Option<Explode>; 2], fire: [FireSlot; 2],
    eat: Option<EatMonster>, fall: Option<Fall>,
    extra_life: Option<ExtraLife>, bonus: Option<Bonus>,
    fire_turn: FireballId, explode_turn: FireballId, noise: NoiseRng,
}
```

- "Inactive" is `None`, not a flag next to stale fields. Starting an effect constructs a fresh state, which is what makes restarts deterministic. The exceptions are the wobble and the bonus siren: the simulation repeats their start command for as long as the cue lasts (every other frame for a wobbling bag, every frame of the bonus flashing), and upstream's start only raises a flag, so a start keeps a running state and only the stop resets it; a fresh state per start would never let the wobble reach its first blip on tick 16.
- Each state type has `fn tick(&mut self) -> Step`, where `Step { offer: Option<Divisor>, done: bool }`. `done` on the same tick as an offer covers the effects that sound on the tick they end (gold, death).
- `Effects::tick` calls the slots in upstream's updater order (emerald, wobble, death, break, gold, click, explode, fire, eat, fall, extra life, bonus) and keeps the last `Some` offer. The order is one array literal of method calls in one function, with the spec's priority list in its doc comment; a test asserts every adjacent pair.
- `FireSlot { flight: Option<Fire>, cadence: bool }`: the cadence bit lives outside the `Option` because a restart must not reset it (kept quirk SQ4), while `FireStop` resets both.
- Death needs to stop the music on its first tick. `Death::tick` returns that as a flag in an effect-specific result, `Effects::tick` returns it next to the heard offer (`EffectsTick { offer, stop_music }`), and `Synth::tick` acts on it. Effects never hold a reference to the music player.
- Stop-all empties every slot but keeps the noise generator and the two round-robin turns, as upstream's stop-all only switches each effect off.
- All arithmetic is on `u16`/`u32` with plain operators; the sequences stay far below overflow (largest value 30500). The only wrapping arithmetic is `NoiseRng` (`wrapping_mul`, `wrapping_add`), named as such.

### D4. Level-done jingle and music

`LevelDone { note: u8, ticks_left: u8 }` in `Option`. While it is `Some`, `Synth::tick` skips effects and music entirely (upstream's `soundpausedflag` side effect, kept as an explicit branch) and produces `Voice::Jingle { carrier: Divisor, beat: Hertz }`. On completion or `StopAll` it pushes `SoundEvent::LevelDoneFinished`. The user pause is a separate `bool`; upstream overloads one flag for both.

`MusicPlayer { current: Option<Playing>, level: Level }` with `Playing { tune, tempo, pos, ticks_left, note_ticks, gate, divisor, stage: EnvelopeStage, dirge_reported: bool }` and `enum EnvelopeStage { Attack, Decay, Release }`. Per-tune constants come from one `const fn params(Tune) -> TuneParams` (attack, sustain, decay, release, ticks per unit, gate rule). `level` sits outside `Playing` because it is carried across tunes (upstream's `musvol` is never reset); the jingle sets it to 50 when it runs, as upstream does.

The music switch is consulted by `Synth::tick`: the player advances when `music_on || tune == Dirge`. This replaces upstream's "force the flag on and restore it later" (`restore_musicflag_after_death`) with a rule that has no hidden state (fixed quirk F5).

### D5. One voice per tick, then an oscillator

`Synth::tick` reduces everything to one value:

```rust
pub enum Voice {
    Silent,
    Effect(Divisor),                      // full amplitude; silent if outside 41..=16383
    Music { divisor: Divisor, level: Level },
    Jingle { carrier: Divisor, beat: Hertz },
}
pub struct TickTrace { pub voice: Voice, pub output: Voice, pub effect_offer: Option<Divisor>, pub music_level: Level }
```

`voice` is what the effects, music and jingle chose; `output` is what is sent to the oscillator: `voice`, or `Silent` while sound is off or paused (D7 step 5).

`tick()` returns the `TickTrace`. This is the seam for tests: every spec scenario about divisors, levels and tick counts is asserted on traces, with no samples involved. The divisor gate (41..=16383), the music clamp (>= 1000) and "level 1 is silent" are applied in one function, `Voice::tone(rate) -> Option<Tone { hz, peak }>` (the sample rate is needed for the Nyquist guard of D6), so the death effect's shape (kept quirk SQ1) is isolated there.

Upstream's `spkrmode`, `wavetype`, `soundt0flag` and per-band mute flags are not ported. Their whole observable effect is "which voice sounds", plus the silent-jingle bug, which is fixed.

### D6. Square-wave generator: integer phase accumulator

Upstream's generator divides a sample counter by the sample rate in `f64` with `fmod` and several folding counters. Since sample identity with the C build is not required, the port uses an exact integer design:

```rust
pub struct SquareOsc { acc: u32 }          // 0 <= acc < rate
// per sample, for a tone of `hz`:
//   high = 2 * acc < rate;  acc += hz;  if acc >= rate { acc -= rate }
```

- Frequencies are whole Hz (integer divisor division) and the rate is an integer, so `acc / rate` is the exact phase. There is no drift, the waveform is exactly periodic over one second, and no floating point is used: unfiltered output is bit-identical on every platform.
- Changing `hz` keeps `acc`, which is the phase continuity the spec requires. When the voice becomes silent the accumulator is reset to 0.
- Three accumulators: effect tone, music tone, jingle gate. The jingle carrier uses the music accumulator; the gate is "on while `2 * gate_acc < rate`", and a beat of 0 Hz means ungated.
- Sample value: `+peak` when high, `-peak` otherwise, with `peak = ((level - 1) * 32767 / 49) / 2` for music and 16383 for effects and jingle (integer arithmetic; equals upstream's `(i16)(amp * 32767) / 2` for all 50 levels, asserted by a table test).
- Tones at or above half the sample rate cannot occur for rates >= 8000 with the music clamp (1193 Hz) and effect divisors in the spec (highest is 2997 Hz); `Voice::tone()` still silences any tone with `2 * hz >= rate` rather than aliasing.

Tolerance, stated once and used in tests: against upstream, each edge may fall up to one sample earlier or later, and the phase after a silent tick restarts at 0. Against the port itself there is no tolerance: tick traces and unfiltered samples are exact.

Alternatives rejected: (a) a literal port of `soundgen.c` — buys nothing without a C reference to compare against, and drags in float folding logic that is hard to test; (b) an `f32`/`f64` phase accumulator — drifts and makes golden hashes platform-dependent; (c) band-limited (polyBLEP) synthesis — changes the PC-speaker character and is not what upstream outputs.

### D7. Tick scheduling and rendering

`SampleRate` is a validated newtype (8000..=192000); `samples_per_tick = (rate * 10 + 364) / 728`, which is `round(rate / 72.8)` in integers (606 at 44100, 659 at 48000, 303 at 22050; table test).

`Synth::render(out)` runs, per sample: if `(step + 1) % samples_per_tick == 0`, call the tick hook; then produce one sample from the current `Voice`; then filter and volume. The tick hook in `AudioSource` drains the input queue into `Synth::apply` and then calls `Synth::tick`; in `ManualDriver` it does the same from a `Vec`. The loop is restructured to process whole runs between ticks so the modulo is not evaluated per sample.

Order inside `Synth::tick`: (1) inputs have been applied; (2) if paused, return `Silent` without advancing; (3) if the jingle is active, advance it; (4) otherwise advance music if allowed, advance effects, choose the voice by "any effect offer beats music"; (5) if sound is off, the returned trace keeps the chosen voice but the output voice is `Silent` (the trace has both, so mute tests can assert that state advanced).

Output stage: `raw -> (filter on? round(lp(hp(raw))) clamped to i16 : raw) -> * volume / 100`. The filter uses `f64` and `f64::tan` for coefficients, computed once per rate in `OutputFilter::new`. Filter state is reset whenever the filter is switched on, so toggling is deterministic.

### D8. Thread boundary: `rtrb` single-producer queues

```rust
pub fn sound_link(config: SoundConfig, tables: &TuneTables)
    -> Result<(SoundHandle, AudioSource), ConfigError>;
```

- `SoundHandle` (game/frontend thread): `send(SoundCommand)`, `control(SoundControl)`, `poll_event() -> Option<SoundEvent>`, `status() -> SoundStatus { sound_on, music_on, dropped }`.
- `AudioSource` (moved into the device callback; `Send`, not `Sync`): `fill(&mut [i16])`. It owns the `Synth`.
- Game to audio: `rtrb::RingBuffer<Input>` with capacity 4096 (upstream's queue length). `rtrb` is a wait-free single-producer single-consumer ring with no allocation after construction. On a full ring the input is dropped and a counter is incremented; the sender never blocks. The counter is a plain `u32` field of `SoundHandle`, because only the sending thread writes and reads it (`status().dropped`). All commands are drained at each tick (upstream's batch limit of 64 is not kept; the ring bounds the work).
- Audio to game: a second `rtrb` ring of `SoundEvent`, capacity 16; if full, the event is dropped.
- Status: two `AtomicBool`s (sound on, music on) written by the audio side after each tick, read by the frontend for display. They are needed because "sound on forces music on" is decided on the audio side.
- The `unsafe` inside `rtrb` is in the dependency; `digger-sound` itself keeps `#![forbid(unsafe_code)]`.

This removes upstream's data races by construction: every variable upstream touched from both threads (`spkrmode`, `pulsewidth`, `volume`, the backend statics during `initsound`/`killsound`) is a field of `Synth`, which exactly one thread owns. Initial state is passed in `SoundConfig` before the source is moved to the audio thread.

Alternatives rejected: `Mutex<Synth>` (priority inversion on the audio thread; a lock per callback); `std::sync::mpsc` (allocates per send); `crossbeam-queue::ArrayQueue` (multi-producer machinery not needed); `ringbuf` (equivalent; `rtrb` has the smaller API and is aimed at exactly this use).

### D9. Completion reporting without waiting

Upstream allocates an ack id, the game thread polls for it in a real-time loop, and cancel/complete races are handled with two id sets. The port has none of that:

- The simulation waits a fixed number of frames in both places. Those numbers belong to the `game-flow` spec of `add-simulation-core`: the dirge wait is upstream's 89 game frames; the level-done wait must cover 231 ticks (3.17 s), which is 40 frames at the default 80 ms frame time.
- The engine pushes `SoundEvent::LevelDoneFinished` / `DirgeFinished` to the event ring. The frontend may read them (for example to hold the picture a moment longer at non-default game speeds, a pacing decision `add-desktop-app` owns) or ignore them. `digger-core` has no parameter through which an event could enter the simulation, so the guarantee is structural, and a test replays a game with and without a `ManualDriver` attached and compares state hashes.

### D10. Drivers

- `ManualDriver`: owns a `Synth` and a pending-input list; `send`/`control`, `advance_ticks(n) -> Vec<TickTrace>`, `render(n_samples) -> Vec<i16>`, `events()`. Used by all tests and by `xtask render-sound`. It has no queue and no thread.
- `NullDriver`: implements the same sending interface as `SoundHandle` and discards everything; `status()` reports sound off. The frontend uses it when no device can be opened. A small trait `SoundSink { fn send(&mut self, SoundCommand); fn control(&mut self, SoundControl); }` is implemented by `SoundHandle`, `ManualDriver` and `NullDriver` so the frontend's frame loop is written once.

### D11. Music tables as imported data

`xtask import-assets` is extended (a new `xtask/src/import/music.rs` on phase 2's table reader, with the same `--upstream`, `--out` and `--check` flags and validate-then-write rule) to read four initialiser lists from upstream's `sound_backend.c` as data (integer tokens only; the file is never compiled or translated): `bonusjingle`, `backgjingle`, `dirge`, `battlejingle`. It writes `assets/music.bin`:

```
magic "DGMU", version u8 = 1, tune count u8 = 4
per tune, in the order normal, bonus, dirge, battle:
  note count u16 LE, then note count x (divisor u16 LE, duration u16 LE)
```

The `0x7d64` end marker is checked and stripped (the note count replaces it). Battle notes are stored as upstream stores them, untransposed; the transposition is applied when the blob is decoded, so the blob stays a faithful copy and the formula is tested in Rust. The importer verifies shapes while writing: normal 145 notes, bonus 160, dirge 24 with the first 12 summing to 50 units and the last 12 being rests; the battle length is measured, printed and stored in the blob's own header (364 notes at the pinned commit). Because `sound_backend.c` is mostly code (declarations of other types, `l`-suffixed constants), the table reader gains 16-bit element types and a mode that reads only the named arrays and skips every other declaration unread; suffixed integers are lexed so that code can be skipped but are never accepted as table values. The blob is listed in `assets/manifest.txt` like every other asset, so phase 2's manifest test and `--check` cover it; `assets/index.rs` is not changed.

`digger-sound` embeds the blob with `include_bytes!` (the same relative path scheme `digger-core` uses for `assets/`) and decodes it once, off the audio thread, into `TuneTables` (four boxed slices of `Note { divisor, units }`); `AudioSource` holds them, so the audio path never allocates. Decoding returns `Result<TuneTables, TableError>` (bad magic, unsupported version, a tune count other than 4, truncated, bytes after the last tune, zero-length tune, zero-duration note); the manifest digest pins the blob.

The two tiny tables given in full in research 04 (the 8 emerald divisors and the 11 jingle divisors) are `const` arrays in the code; importing 19 numbers would add machinery for no benefit.

Alternative rejected: generating a Rust source file with the tables. The project's asset convention is "blobs plus index", and a blob can be hash-guarded uniformly.

### D12. Test strategy

1. **Tick-trace tests** (`tests/effects_trace.rs`, `tests/music_trace.rs`, `tests/switches.rs`): one test per spec scenario, driving `ManualDriver::advance_ticks` and asserting `TickTrace` sequences against the numbers in the spec. These are the fidelity tests; they are exact.
2. **Unit tests** next to the code: `NoiseRng` first values, `samples_per_tick` table, divisor gate, peak table for levels 1..=50, envelope stage transitions, table decoding errors.
3. **Property tests** (`proptest`, `tests/generator_props.rs`): for any rate in range and any `hz` with `2 * hz < rate`, one second of output has exactly `2 * hz` sign changes and every half period is within one sample of `rate / (2 * hz)`; a frequency change mid-stream adds no transition at the change point; output is always `+peak` or `-peak`; `render` in arbitrary chunk sizes equals `render` in one call.
4. **Golden sample hashes** (`tests/golden.rs`, `tests/golden/hashes.toml`): about ten scripted renders at 44100 Hz, filter off, volume 100 (each effect alone; normal tune 10 s; bonus tune 10 s; battle tune 10 s; dirge to its finish; jingle; a mixed script with effects over music). The SHA-256 of the little-endian sample bytes is stored. The hashes are generated by the port (`UPDATE_GOLDEN=1 cargo test -p digger-sound --test golden`), and before they are committed each render is written to WAV with `cargo xtask render-sound` and listened to once and checked against the notes, durations, effect shapes and priorities documented in research 04 (no upstream build is run); the review is recorded in `tests/golden/REVIEW.md` (date, reviewer, what was compared). After that the hashes guard against unintended change.
5. **Filter tests**: coefficient values at 44100 Hz against the closed form to 1e-12; step response decays to 0; a 2 kHz sine passes within 3 dB; output never leaves the `i16` range for a full-amplitude 79 Hz square. No hashes of filtered output (the coefficients depend on the platform's `tan`).
6. **Non-interference test**: run a scripted `digger-core` game twice, once forwarding commands to a `ManualDriver` and once dropping them, and compare final state hashes.
7. **Queue tests**: overflow drops and counts; events arrive once; a two-thread smoke test pushes commands while another thread calls `fill`.

### D13. Idiomatic Rust in place of upstream's structure

| Upstream | Port |
|---|---|
| ~80 `static` variables in `sound_backend.c` | fields of `Synth` and its parts; one owner |
| flag + value statics per effect | `Option<State>`; start = construct, stop = `None` |
| updaters overwriting `t2val`, sentinel 40 = "none" | `tick()` returns `Option<Divisor>`; priority is an ordered fold |
| sentinel `0x7d00` for rest, `0x7d64` for end | `Note` with a rest predicate; table length |
| `spkrmode` 0/1/2, `wavetype`, edge-trigger latch | `enum Voice` computed fresh each tick |
| `musicstage` 0/1/2 | `enum EnvelopeStage` |
| function pointers `timer0`/`timer2`/`s0*`/`s1*` | direct calls; "no device" is `NullDriver` |
| ack ids, completed and cancelled sets, blocking waits | `SoundEvent` ring; fixed frame counts in the simulation |
| spinlocks, cross-thread statics | `rtrb` rings, atomics for status, `Synth` moved to the audio thread |
| unchecked `emfreqs[n]`, fireball index | `EmeraldStep`, `FireballId` types |
| `int16_t` everywhere | `Divisor(u16)`, `Level(u8)`, `SoundTicks(u32)`, `Hertz(u32)` newtypes |
| division by zero if `t0rate == 0` | beat frequency computed from two valid divisors; 0 Hz = ungated |

Errors: `ConfigError` (sample rate out of range) and `TableError` (blob decoding) are small enums implementing `std::error::Error`. Nothing on the tick or sample path can fail, and nothing there uses `unwrap`, indexing that can panic on input-derived values, or allocation.

### D14. Compat quirks: kept and fixed

None of these affects the simulation; they are listed because `docs/compat-quirks.md` is the project's single register. Each gets an entry there and the named test.

Kept (audible shape of the original):

| # | Quirk | Isolated in | Test |
|---|---|---|---|
| SQ1 | Divisors >= 0x4000 are silent, so the death effect is heard only on ticks 4..22 of 51 while displacing music on all 51 | `Voice::tone()` | `death_is_audible_on_ticks_4_to_22` |
| SQ2 | Frequency is `1193181 / divisor` with integer division | `Divisor::hertz()` | `divisor_division_truncates` |
| SQ3 | Effect priority is upstream's updater order; effects that are silent on a tick let lower ones through | `Effects::tick` | `effect_priority_pairs` |
| SQ4 | Fire restart resets the divisor but not the two-tick cadence | `FireSlot` | `fire_restart_keeps_cadence` |
| SQ5 | Gold and death still sound on the tick on which they end (31 and 51 ticks) | `Step { offer, done }` | `gold_lasts_31_ticks`, `death_lasts_51_ticks` |
| SQ6 | Music level is carried across notes and tunes, starts at 8, and is left at 50 by the jingle | `MusicPlayer::level` | `level_carries_across_tunes` |
| SQ7 | Normal tune uses a fixed 2-unit gate; a note occupies duration x units + 1 ticks | `TuneParams` | `normal_tune_two_unit_note_levels` |
| SQ8 | Quick tempo is 0.7 of the standard ticks per unit, truncated (4, 2, 7, 1) | `Tempo::ticks_per_unit` (integer table) | `tempo_table` |
| SQ9 | Sound-only LCG with wrapping 32-bit arithmetic, seed 0, never reseeded by stop-all | `NoiseRng` | `noise_first_values` |
| SQ10 | Switching sound on switches music on | `Synth::apply` | `sound_on_forces_music_on` |
| SQ11 | Music switched off freezes the tune position | `Synth::tick` | `music_off_freezes_position` |
| SQ12 | The dirge plays its silent rests after its audible part and then loops to its first note until stopped; its finish is reported once per start | `MusicPlayer` | `dirge_finishes_on_tick_513`, `dirge_event_reported_once` |

Fixed (presentation bugs, with the reason):

| # | Upstream behaviour | Port | Test |
|---|---|---|---|
| SF1 | Level-done jingle silent until some music has played | Always audible | `jingle_audible_before_any_music` |
| SF2 | Volume setting read and ignored | Master volume 0..=100 % | `half_volume_peak` |
| SF3 | Sound cannot be enabled after starting quiet | Engine always exists; sound off is a mute | `started_quiet_can_be_enabled` |
| SF4 | With sound off, effect state freezes and resumes stale; music requests are dropped | State advances silently; requests are honoured | `mute_does_not_freeze_state` |
| SF5 | Dirge with music off flips the music flag and restores it later | Dirge ignores the music switch; the switch is never changed by it | `dirge_plays_with_music_off` |
| SF6 | Fall effect's on/off phase survives stop and restart | Restart always begins the same cycle (at most 2 ticks, 27 ms, different) | `fall_first_three_tones` |
| SF7 | Game thread blocks until jingle/dirge acknowledgements | Fixed frame counts; events are informational | `simulation_unaffected_by_sound` |
| SF8 | Cross-thread unsynchronised state; per-sample spinlock | Single owner, lock-free queues | `two_thread_smoke` |
| SF9 | Unchecked table index for the emerald step; division by zero hazard in the jingle gate | Checked types; 0 Hz beat = ungated | `emerald_step_range`, `zero_beat_is_ungated` |
| SF10 | Filter is a compile-time choice that differs between upstream build systems | Runtime switch, on by default (as in upstream's CMake build) | `filter_toggle` |

## Risks / Trade-offs

- [Research 04 was derived by reading code, not by tracing a run, so a tick sequence could be off by one] → The spec's sequences were recomputed from the documented update rules; the one-off listening review of the golden renders against research 04 and the reviewer's knowledge of the original game is the check. A disagreement found there is fixed in spec, test and `docs/compat-quirks.md` together.
- [The music tables are not in the research report, so tune content cannot be reviewed from the spec] → Shape checks in the importer (note counts, dirge unit sum), a blob hash test, and the listening review.
- [Fixed frame counts versus real-time sound: at non-default game speeds the level-done wait is shorter or longer than the jingle] → Accepted. A faster game cuts the jingle with the next round's stop-all; `add-desktop-app` may use `LevelDoneFinished` for pacing if that proves unpleasant.
- [Replay at maximum speed with a device attached floods the queue; a dropped stop command could leave an effect running] → Overflow is counted and exposed; `add-desktop-app` is told (in the crate docs) to use `NullDriver` during fast-forward and send `StopAll` when returning to real time.
- [Filtered output differs in the last bit between platforms] → No hashes on filtered output; tolerance tests only.
- [`digger-sound` depends on `digger-core` for one enum] → Accepted to avoid a duplicate type and a conversion layer; the dependency direction (core knows nothing of sound) is unchanged.
- [Naive square waves alias at high pitches] → Same as upstream; the default filter softens it. Not addressed further.

## Migration Plan

The crate skeleton from phase 1 is filled and one asset is added; nothing existing changes behaviour. Rollback is emptying the crate again and removing `music.bin` and its manifest line.

## Open Questions

- Whether `add-desktop-app` uses the completion events for pacing at non-default speeds. Either answer leaves this crate unchanged.
