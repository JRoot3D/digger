# Design

## Context

See `proposal.md` for motivation and scope. After `add-assets-and-renderer`, `digger-core` contains the imported assets (sprites, font, title picture), a 640×400 indexed framebuffer addressed as 320×200 with text, masked blits and the pixel probe, and the sprite layer with its draw-time collision lists. This change adds the game on top of them.

Constraints that shape the design:

- The behavioural source is the research reports (01, 02, 03 §8, 05 §1.6–1.7 and §6). Their "suggested Rust design" sections are superseded by `docs/porting-plan.md`: one owning `Game`, advanced by `step`, enums and explicit state machines, no globals, no `unsafe`, no allocation in `step`.
- Three couplings from the original are behaviour and cannot be designed away: collision results exist only as the lists produced by a sprite draw; the fireball finds earth by reading pixels; the penalty counter turns draw and score calls into monster delays, which decide how many random numbers are drawn.
- The next change (`add-recording-and-conformance`) must be able to (a) feed recorded seeds and movement samples into the game at the exact points the original consumed them and (b) compute the upstream state hash, which covers a fixed list of variables (research 01 §8) **including stale ones** such as the death counters of a digger that is alive. This design must therefore keep every hashed variable as a distinct, readable value.
- Phase-2 items are used under the names `add-assets-and-renderer` gives them: `Framebuffer`, `SpriteLayer`, `SpriteId`, `Collisions` (the per-class collision lists, returned by value from `SpriteLayer::move_to`), `ImageId`, `Px` (logical pixel position), `ProbeBits`, `TextColor` and `LevelPlan` (the background plan number 1..=8). A level's 10×15 cell map is this change's `LevelLayout`, so the two are not confused.
- The built-in level layouts are not among the phase-2 assets. This change extends `xtask import-assets` with them (`assets/levels.bin`, listed in `assets/manifest.txt`), keeping phase 2's flags and validate-then-write rule.

## Goals / Non-Goals

**Goals:**

- A complete headless game in `digger-core`, driven only by `Game::step`, that meets the six capability specs.
- A small, stable public interface for the three phases that consume it (recording, sound, desktop app).
- Every simulation-affecting quirk isolated in one named place with one test, and listed in `docs/compat-quirks.md`.
- Defined, safe behaviour for every case that is undefined behaviour in the C original.

**Non-Goals:**

- Bit-exact proof against upstream recordings. That is the next change; this one must only make it possible (observer hook, state view, tick trace).
- Any sound synthesis, timing against a real clock, device input, files, or the title/high-score screens.
- Structural resemblance to the C source. Where this design keeps the original's granularity (hashed variables, draw-call order) it says why.
- Performance work beyond "no allocation in `step`"; a tick is a few hundred sprite blits at most.

## Decisions

### D1. Module layout

All new code lives in `crates/digger-core/src/sim/` (plus two small top-level modules for the public boundary types). No new crate: the simulation needs the framebuffer and sprite layer on every tick, and the recording codecs of the next change need the simulation.

| Module | Contents |
|---|---|
| `sim/mod.rs` | `Game`, `Phase`, `step`, `StepOutcome`, round start/end transitions |
| `sim/config.rs` | `GameConfig`, `GameMode`, `StartLevel`, `GauntletSeconds`, `ConfigError` |
| `sim/level.rs` | `LevelSet`, `LevelLayout`, `LevelNumber`, `Difficulty`, layout sequence, custom-level parser, two-digger start-cell substitution |
| `sim/geom.rs` | `Direction`, `CellPos`, playfield limits, cell/pixel conversion (pixel positions are phase 2's `Px`) |
| `sim/ids.rs` | Typed ids `PlayerId`, `DiggerId`, `SlotId`, `MonsterSlot`, `BagId` with range-checked constructors and array-index helpers (`FireballId` lives in `sound_cmd.rs`) |
| `sim/gfx.rs` | `Gfx` (framebuffer and sprite layer) and the entity-draw helpers returning `Collisions` |
| `sim/field.rs` | `Field`, `Cell` (earth slices), dig step, redraw normalisation, `EmeraldMap`, per-player saves |
| `sim/draw.rs` | Background, tunnel openings, carving shapes, emeralds, progressive level draw |
| `sim/status.rs` | Status line: scores, lives, gauntlet clock, top-line messages |
| `sim/rng.rs` | `GameRng`, `round_seed` (SplitMix64) |
| `sim/penalty.rs` | `Penalty`, `PenaltySite`, overflow rule |
| `sim/score.rs` | `Scores`, scoring call, bonus thresholds, rollover |
| `sim/input.rs` | `Controls`, `FrameInput`, direction resolver, fire latch, `Movement` |
| `sim/digger.rs` | Digger state, movement, emeralds, death state machine, respawn, lives |
| `sim/fireball.rs` | Fireball state, flight, earth probe use, explosion |
| `sim/bonus.rs` | Bonus appearance, bonus-mode timers and signalling |
| `sim/monster.rs` | Monster slots, spawning, decision, move, transformations, deaths |
| `sim/bags.rs` | Bag state machine, push, chain push, gold, round-end tidy |
| `sim/gauntlet.rs` | Gauntlet clock |
| `sim/state.rs` | `StateView` (read-only state for hashing, tests and traces), tick trace line |
| `sim/observer.rs` | `Observer` trait and the no-op observer |
| `sim/timing.rs` | `FrameLength`, speed mapping (pure functions; no clock) |
| `sound_cmd.rs` | `SoundCommand`, `Tune`, `Tempo`, `EmeraldStep`, `FireballId`, the fixed-capacity queue |
| `sim/testkit.rs` | Scenario builder and input scripts, behind the `testkit` feature |

*Alternative rejected:* mirroring the C files (`main`, `digger`, `monster`, `bags`, `drawing`, `scores`). It would put the round state machine, configuration and RNG in one "main" module and scatter the quirks; the layout above gives every spec capability one or two obvious homes.

### D2. Public interface

```rust
pub struct GameConfig {
    pub mode: GameMode,
    pub start_level: StartLevel,          // validated 1..=1000
    pub unlimited_lives: bool,
    pub levels: LevelSet,                 // eight plans, bonus step, "verbatim" flag
}
pub enum GameMode {
    OnePlayer, TwoPlayers, TwoDiggers,
    Gauntlet { time: GauntletSeconds }, GauntletTwoDiggers { time: GauntletSeconds },
}
impl GameMode {
    /// For settings that arrive as separate numbers (recording headers, config files).
    pub fn from_parts(players: u8, diggers: u8, gauntlet: Option<u32>) -> Result<Self, ConfigError>;
}
impl LevelSet {
    pub fn builtin() -> Self;
    pub fn from_custom_bytes(data: &[u8]) -> Result<Self, LevelError>;   // always verbatim
    /// For recordings. `verbatim` disables the two-digger start-cell substitution (Q07).
    pub fn from_plans(plans: [LevelLayout; 8], bonus_step: u16, verbatim: bool) -> Self;
}

impl Game {
    pub fn new(config: GameConfig, base_seed: u64) -> Box<Game>;
    pub fn step(&mut self, input: &FrameInput) -> StepOutcome;                       // no-op observer
    pub fn step_observed(&mut self, input: &FrameInput, obs: &mut dyn Observer) -> StepOutcome;
    pub fn framebuffer(&self) -> &Framebuffer;
    pub fn drain_sound(&mut self) -> impl Iterator<Item = SoundCommand> + '_;
    pub fn state(&self) -> StateView<'_>;
}
pub struct FrameInput { pub slots: [Controls; 2], pub pause: bool, pub quit: bool }
pub struct Controls { pub up: bool, pub down: bool, pub left: bool, pub right: bool, pub fire: bool }
pub struct StepOutcome {
    pub frame: FrameKind,                     // Tick | Wait | Paused
    pub player_out: Option<PlayerResult>,     // set on the frame a player's game ends
    pub finished: Option<GameSummary>,        // reason + scores + level + ticks
}
```

`Game::new` is the porting plan's `Game::new(config, base_seed)`: a recording replaces individual round seeds through the observer (D8). `Game` is boxed because it owns the 256 KB framebuffer; that box is the only allocation the simulation ever makes.

`GameMode` as an enum makes the two invalid combinations unrepresentable inside the simulation; `from_parts` is where they are rejected for callers holding raw settings. `StartLevel` and `GauntletSeconds` are validated newtypes (`TryFrom`), so `Game::new` itself is infallible.

### D3. The `Game` state machine

`step` advances exactly one frame. `Phase` is an enum; every waiting loop of the original is a variant with a frame counter, each count a named constant.

| Phase | Frames | Frame kind | Leaves to |
|---|---|---|---|
| `LevelDraw { cursor }` | ⌈items / 3⌉ (`DRAW_ITEMS_PER_FRAME = 3`; items = opened cells, then emeralds) | Wait | `PlayerFlash` (alternating mode, first round or after a turn change) or `Playing` |
| `PlayerFlash { frame }` | `PLAYER_FLASH_FRAMES = 45` | Wait | `Playing` |
| `Playing` | one tick per frame | Tick | `Paused`, `WindDown`, or round end directly on gauntlet time-out |
| `WindDown { left }` | 1..n ticks (`WIND_DOWN_MAX = 20`, `WIND_DOWN_QUIET_PENALTY = 8`) | Tick | round end |
| `LevelDone { left }` | `LEVEL_DONE_FRAMES = 40` | Wait | `LevelDraw` |
| `Notice { kind, left }` (`GAME OVER` / `TIME UP`) | `NOTICE_FRAMES = 50` | Wait | `LevelDraw` (other player's turn) or `Finished` |
| `Paused` | until a pause request | Paused | `Playing` |
| `Finished` | absorbing | Wait | — |

Round start (`begin_round`: seed, entity reset, score refresh, lives, main music, control reset) and round end (`end_round`: stop sound, remove fireballs and bonus, tidy bags, save field, remove monsters, decide outcome) are **transitions**, not phases: they take no frame. They run inside the `step` call whose frame ends the preceding phase, so a death on the same level goes from the last wind-down tick straight to the first tick of the next round on the following frame, as in the original. The full level draw applies the redraw normalisation to the whole field when the phase is entered and reveals the pixels progressively; no tick can observe the difference.

`LEVEL_DONE_FRAMES = 40` models the original's wait for the 3.17 s jingle (231 sound ticks) at the default 80 ms frame. The original's second audio wait, for the dirge, needs no frames of its own: its 89-tick in-game wait already covers the 7.05 s tune.

*Alternatives rejected:* (1) keeping the blocking structure and running the game on its own thread or as a generator/async task — hides state in a stack, cannot be hashed, stepped or serialised, and brings threads into a crate that must have none; (2) a callback "wait for frame" trait object passed into a blocking `run` — same problem, and the web build cannot block.

### D4. Ownership and cross-entity mutation

The original's routines call each other freely across modules in mid-update (a monster's move pushes a bag, which crushes a digger, which changes what the next monster does). Any design that defers these effects changes behaviour, because collision lists and the penalty counter depend on the exact order of draws.

```rust
pub struct Game { phase: Phase, sim: Sim, gfx: Gfx, sound: SoundQueue, .. }
struct Sim   { cfg, rng, penalty, ticks, round, turn, players: [PlayerState; 2], field, emeralds,
               diggers: [Digger; 2], monsters: Monsters, bags: Bags, bonus: Bonus,
               scores: Scores, gauntlet: Option<GauntletClock>, inputs: [SlotInput; 2] }
struct Gfx   { fb: Framebuffer, sprites: SpriteLayer }
struct Ctx<'a> { sim: &'a mut Sim, gfx: &'a mut Gfx, sound: &'a mut SoundQueue, obs: &'a mut dyn Observer }
```

- `step` splits `Game` into disjoint borrows once and builds a `Ctx`. Every system is an `impl Ctx<'_>` block in its own module (`digger.rs`, `monster.rs`, `bags.rs`, …), so `cx.push_bags(dir, hits)` can call `cx.kill_digger(id, cause)` and `cx.squash_monsters(bag, hits)` with ordinary `&mut self` re-borrows. Mutual recursion (`push_bag` ↔ `push_bags`) is plain recursion on `&mut self`, bounded by the seven bags.
- Entities are addressed by typed index (`DiggerId`, `MonsterSlot`, `BagId`), never by a reference held across a call that takes `cx`. Entity structs are small and `Copy`; a routine reads what it needs, calls out, then writes by index.
- A sprite draw returns its collision result **by value**: phase 2's `Collisions` (`Copy`, a bit set iterated per class in ascending order) from `SpriteLayer::move_to`. This replaces the original's global lists and its "copy the globals right after the draw" idiom. The one place the original read the globals late (gold collection) uses the `Collisions` returned by its own draw, which is the same data.
- Entity structs do not know about the framebuffer; drawing an entity is a `Ctx` method that reads the entity and calls `gfx`.

*Alternatives rejected:* an ECS or an event queue applied at the end of a system (changes draw and penalty order); `Rc<RefCell<_>>` per entity (runtime borrow panics, hides order); separate `&mut` parameters per subsystem (the call graph needs all of them almost everywhere, producing ten-argument functions).

### D5. Hash-visible state keeps the original's granularity

Every variable that the upstream state hash covers (research 01 §8) exists in `Sim` as a distinct field with the same value domain, even where a tidier model would merge them:

- A digger keeps `death: DeathStage` (a fieldless enum of the six original stages, where the first also is the value while alive) **and** separate `death_bag`, `death_frame`, `death_timer` counters, rather than enum payloads, because the counters keep stale values between deaths and the hash sees them.
- Monsters keep their cell/remainder fields as stored at the end of the previous move (decisions are specified against those stored values), their delay, counter, death fields and standstill, and keep position data after removal.
- `Cell` is a newtype over `u16` with the original bit layout (slices in bits 0–4 and 6–11, untouched in bit 13, the rest always set); `EmeraldMap` is a per-cell bit set with one bit per player. All access is through named methods (`has_tunnel`, `is_untouched`, `clear_h_slice`); no mask literals appear outside `field.rs`.

Within that constraint the types are idiomatic: `Option<Direction>` for "no direction" (monster direction, digger movement and — for the facing quirk — digger facing), enums for bag motion, monster kind and death stage, typed ids for slots, `u16`/`u32` wrapping arithmetic only where the spec says "wrap" (`GameRng`, gauntlet clock, score award, eat multiplier), each behind a named helper. `StateView` exposes typed getters; converting them to the legacy numeric encodings and byte widths is the hash module's job in the next change.

*Alternative rejected:* modelling state "cleanly" now and reconstructing stale values for the hash later. The stale values are not reconstructible.

### D6. Penalty counter

`Sim::penalty: Penalty` is reset at the top of each tick. The only way to raise it is `cx.charge(PenaltySite::…)`, where `PenaltySite` is an enum with one variant per site of the spec's penalty table (`DigStep`, `DiggerDraw`, `DiggerPushPause`, `DiggerBlockedRedraw`, `FireballFlight`, `ExplosionFrame`, `CarriedDigger`, `DeathArc`, `TombstoneFrame`, `EmeraldRedraw`, `EmeraldEaten`, `MonsterDraw`, `MonsterOverlap`, `MonsterBlockedRedraw`, `CarriedMonster`, `GoldFrame`, `WobbleFrame`, `BagLanding`, `FallingBagPushed`, `FallStepDraw`, `SlideStepDraw`, `PushRestore`, `GoldCollected`, `ScoreCall`). The dig step charges inside `Field::dig` and the scoring call charges three times inside `Scores::add`, so no caller can forget them.

Under `cfg(test)` (and the `testkit` feature) `Penalty` also keeps a per-site count for the current tick, so a scenario test can assert not just the total but which sites fired. This is the main defence against the most likely class of replay divergence (one missed or doubled increment).

The overflow rule is one function, `Monsters::apply_penalty_overflow(penalty)`, called only from the main-phase tick.

### D7. Random numbers and seeds

`GameRng` wraps an `i32` and has one method, `draw(modulus: Modulus) -> i16`, with `Modulus` a non-zero newtype built from `Difficulty` (`double_move_modulus`, `noise_modulus`), which makes division by zero unrepresentable. Only `monster.rs` can reach it (`pub(in crate::sim::monster)` accessor on `Ctx`). `round_seed(base, round)` is a free function in `rng.rs`. Tests can read a draw counter under `cfg(test)` to assert the draw pattern requirement exactly.

### D8. Boundary: controls, observer, sound

**Controls in.** `FrameInput` carries raw per-slot booleans. The frontend ORs "pressed since the last frame" into "held now" before calling `step`; the simulation ignores controls on non-tick frames (they could only matter through state the round start clears). The direction resolver and the fire latch are simulation state (`SlotInput`), because the original resolves directions inside the tick and eDRF recordings store raw controls.

**Observer.** One trait with default no-op methods lets the recording change observe or replace values at the exact points the original did, without the simulation knowing about files:

```rust
pub trait Observer {
    fn round_seed(&mut self, round: u32, derived: i32) -> i32 { derived }
    fn level_plan(&mut self, player: PlayerId, level: LevelNumber, layout: &LevelLayout) -> Option<LevelLayout> { None }
    fn controls(&mut self, tick: u32, slot: SlotId, raw: Controls) -> Controls { raw }      // once per slot per tick
    fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement { sample }            // once per movement sample
    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {}
    fn round_end(&mut self, state: StateView<'_>) {}       // after tidy/save/monster removal
    fn game_end(&mut self, state: StateView<'_>) {}
}
```

`Movement { direction: Option<Direction>, fire: bool }` is what a DRF direction record holds; a replaced `fire` overwrites the slot's pending fire state, which the latch moves to "active" at the next tick, reproducing the one-tick delay. Dynamic dispatch (`&mut dyn Observer`) is used so the simulation is compiled once; the handful of calls per tick are negligible.

What the hooks guarantee to `add-recording-and-conformance`, which builds playback, recording and take-over on them without any further change to the simulation: `round_seed` is called once at every round start; `level_plan` once each time a level is set up for a participating player (never for the Q06 placeholder state), and a returned layout is used verbatim, without the Q07 substitution; `controls` once per slot on every tick whether or not that slot's digger reads its controls (an eDRF value is used up either way); `movement` exactly once per control read (Q15), which is where a DRF record is placed and how a recorder tells a read slot from an unread one. An observer that stops replacing values (playback take-over) leaves the game running on its own derived seeds and the caller's `FrameInput`. A playback that runs out of data asks for the game to end through `FrameInput::quit` on the next frame, so the tick in progress completes and is counted.

**Sound out.** `SoundCommand` is defined in `digger-core` (`sound_cmd.rs`); `digger-sound` will depend on `digger-core` for it.

```rust
pub enum SoundCommand {
    FallStart, FallStop, WobbleStart, WobbleStop, Break, Gold,
    Fire(FireballId), FireStop(FireballId), Explode(FireballId),
    EmeraldClick, EmeraldNote(EmeraldStep /* 0..=7, checked */), EatMonster, DiggerDeath, ExtraLife,
    BonusPulseStart, BonusPulseStop,
    Music { tune: Tune, tempo: Tempo }, MusicOff,     // Tune: Main | LastLife | Bonus | Dirge; Tempo: Standard | Quick
    LevelDone, Pause, Resume, StopAll,
}
```

Commands are pushed in event order into a fixed-capacity ring (`SOUND_QUEUE_CAP = 64`) owned by `Game` and drained by the caller after each `step`. The worst realistic frame produces fewer than 30 commands; if the caller never drains, the oldest commands are dropped (sound is not simulation state) and a debug assertion fires in tests. The "last life" tune choice is resolved here, because it depends on lives, so the sound engine needs no access to game state. `Tempo::Quick` is the original's duration factor 0.7, used only for the dirge in two-digger games (research 04 §3.1; research 02 calls it "volume", which research 04 corrects). `Pause` and `Resume` are simulation commands because the pause phase is simulation state. This enum is the single definition; `add-sound-engine` consumes it under these names.

*Alternatives rejected:* a `Sound` trait object called synchronously (lets a frontend's audio code run inside `step` and tempts feedback); returning a `Vec` from `step` (allocation per frame).

### D9. No allocation in `step`

All game tables are arrays; `Collisions`, the sound queue and the draw cursor are fixed-size. The rule is enforced statically: the `sim` module carries `#![deny(clippy::disallowed_types, clippy::disallowed_methods)]` with `Vec`, `String`, `Box`, `Rc`, `Arc`, the hash collections and `format!`-family macros listed in `clippy.toml`; `Game::new` and the `testkit` module are the only places with a local `allow`. Status-line numbers are formatted into stack buffers.

*Alternative rejected:* a counting global allocator in a test — needs `unsafe impl GlobalAlloc`, which the workspace forbids.

### D10. Error handling

Fallible operations are at the edges and return `Result` with `ConfigError` / `LevelError` (crate error enums implementing `std::error::Error`): mode combination, start level range, custom level length. Nothing inside `step` can fail; there is no `unwrap`/`expect` on input-derived data, and index types are constructed only from values already in range. Internal invariants use `debug_assert!`, and the release behaviour of each violated invariant is the "defined behaviour" listed in D13.

### D11. Test strategy

- **Unit tests** beside each module: layout sequence, custom-level parsing, cell/pixel mapping, dig step in all four directions, redraw normalisation, RNG vectors, SplitMix64 vectors, scoring thresholds and rollover, penalty overflow table, direction resolver truth table, per-level parameter tables, speed mapping.
- **Scenario tests** in `crates/digger-core/tests/`, one file per capability, one test per spec scenario, named after it. They use `testkit`: `Scenario::new(mode).rows([...ten 15-character rows...]).seed(n).build()` makes a `Game` on a custom one-layout `LevelSet`; `Script::parse("R*5 . U*6 F")` yields per-tick inputs; helpers such as `run_ticks(n)`, `run_until(|s| …)` step through waiting frames transparently and assert on `StateView`. Scenario tests may place entities through testkit-only setters to reach a situation directly (a monster on a given corner with a given delay), so they do not depend on long input scripts.
- **Determinism test**: the same scripted game run twice, comparing a CRC of the framebuffer and the trace line after every frame.
- **Soak test**: every mode run for 50,000 frames on inputs from a test-local generator, in debug mode (overflow and bounds checks on), asserting no panic and that the game eventually finishes or keeps ticking.
- **Tick trace**: `StateView::trace_line` writes one fixed-format line per tick (tick, RNG state, penalty, scores, entity positions) into a caller's buffer. It is used by the determinism test now and by replay triage in the next change.
- Replay conformance against upstream recordings arrives with `add-recording-and-conformance`; where a recording later disagrees with a report-derived scenario test, the recording wins and the finding is recorded in `docs/compat-quirks.md`.

### D12. Compat quirks reproduced by this change

Each row becomes an entry in `docs/compat-quirks.md` (class "affects the simulation"), is implemented in exactly one named place, and has one test named `quirk_<id>`.

| Id | Quirk | Isolated in |
|---|---|---|
| Q01 | Penalty counter: every site of the table, three per scoring call even for 0 points | `PenaltySite`, `Scores::add` |
| Q02 | Overflow delays slots 1..n−1 only; slot 0 exempt; excess of 1 does nothing; unoccupied slots included | `Monsters::apply_penalty_overflow` |
| Q03 | Random draws taken even when unused (level ≥ 6 noise draw; hobbin double-move draw) | `monster::decide`, `monster::update_slot` |
| Q04 | Wind-down order bags → diggers → monsters; monsters and RNG keep running; 1..20 ticks rule | `Game::tick_wind_down` |
| Q05 | Redraw fully opens partly dug cells | `Field::normalise_for_redraw` |
| Q06 | Second player position initialised as level 0 in non-alternating modes | `Game::new` |
| Q07 | Layouts 3 and 4 opened at (6,9) and (8,9) for two diggers on built-in levels | `LevelSet::cell_char` |
| Q08 | Level sequence 1–8, 6–8, then 5–8 repeating; cap 1000; difficulty frozen at 10 | `level.rs` |
| Q09 | Second score's bonus is one point late | `Scores::add` |
| Q10 | Life cap 5 with threshold still advancing; one threshold step per scoring call | `Scores::add` |
| Q11 | Rollover at one million into a carried total; no bonus lives afterwards | `Scores::add` |
| Q12 | Score award truncated to signed 16 bits; eat multiplier doubles with 16-bit wrap | `score::award_i16`, `Digger::bump_eat_multiplier` |
| Q13 | Gauntlet clock in 1,193,181ths, −80,000 per tick with wrap; +17,897,715 bonus with wrap | `gauntlet.rs` |
| Q14 | Level advances when no emeralds remain even if the digger died; the dead digger loses a life | `Game::end_round` |
| Q15 | Movement samples only when alive, visible and not in push pause; fire latched one tick late | `input.rs`, `digger::update` |
| Q16 | Emerald thresholds per direction (redraw 8/12/16/6, eat 12/9/12/9) | `digger::EMERALD_STEPS` |
| Q17 | Push pause after every horizontal push attempt, successful or not | `digger::meet_bags` |
| Q18 | Blocked digger is redrawn (extra invincibility decrement) and its facing reversed | `digger::meet_bags` |
| Q19 | Fireball: earth by pixel probe with the per-direction sample patterns; ignored when touching any sprite, own digger included | `fireball::fly` |
| Q20 | Kill rule: re-kill allowed while carried by a bag or in the arc, not during the grave stages | `digger::kill` |
| Q21 | Crushed digger held at bag top + 6; squashed monster snapped to bag top without the 6 | `digger::die`, `monster::die` |
| Q22 | Monsters on a fresh grave removed without score | `digger::die` |
| Q23 | One-digger death waits 89 ticks from the first tombstone frame | `digger::DIRGE_TICKS` |
| Q24 | "All dead" tested over absolute digger indices; eat-multiplier reset and respawn facing likewise | `digger.rs` helpers |
| Q25 | Respawn does not restore the fire-ready look | `digger::respawn` |
| Q26 | Outside bonus mode only a monster's own move kills; in bonus mode either side's move eats | `digger::update`, `monster::step` |
| Q27 | Monster side effects (decision, draw, digging, collisions) persist when its move is undone | `monster::step` |
| Q28 | No passable priority → reverse direction; hobbin stuck at the edge; overlapped monster reversed between cells | `monster::decide`, `monster::step` |
| Q29 | Transformation test runs for dead monsters still carried | `monster::update_slot` |
| Q30 | Dead left-facing hobbin is 14 pixels tall | `monster::sprite_for` |
| Q31 | Any monster removal in bonus mode raises the round's total | `monster::remove` |
| Q32 | Shared push counter; failed push sets the bag's direction to the reverse; double restore on a failed chain | `bags::push_bag` |
| Q33 | Pushing a falling bag sideways succeeds without moving it and crushes | `bags::push_bag` |
| Q34 | Landing bag removes overlapped bags; gold over a dug cell expires early; break after two cells | `bags::land`, `bags::update_gold` |
| Q35 | Digger takes gold whenever it overlaps it, whoever triggered the collection | `bags::collect_gold` |
| Q36 | Bag wobble suppressed by a vertically moving digger in the bag's row or the one below | `digger::under_bag` |
| Q37 | Round-end bag tidy rules, including removal at cell (7,9) | `bags::tidy` |
| Q38 | Hobbin dig step charged even with no direction; emeralds destroyed by hobbins and bags without score | `monster::step`, `bags::fall_step` |
| Q39 | Zero-point scoring calls at round start, after pause and at game end perform the threshold check | `Scores::add` call sites |

### D13. Decisions for the original's undefined or accidental behaviour

| Case in the original | Decision here | Class |
|---|---|---|
| Firing with no facing aborts the program | Fire request ignored, fireball stays ready (spec: "Digger left without a facing") | UB → defined |
| Pixel probe beyond the screen reads outside the picture | The phase-2 probe's defined result is used as is (quirks R1, R2): earth beyond x 319 or y 199, row wrap near the right edge, 0 outside the buffer | UB → defined |
| Two alternating players with two diggers indexes past the digger array | Unrepresentable in `GameMode`; rejected by `from_parts` | UB → rejected |
| Start level ≤ 0 gives blank levels and a division by zero in the RNG | `StartLevel` accepts 1..=1000 only; `Modulus` is non-zero by construction | UB → rejected |
| Dig step indexes outside the field | No cell changes; penalty still charged; `debug_assert!` | UB → defined |
| Signed overflow in the generator and SplitMix64 | Wrapping arithmetic, as every recording assumes | UB → defined |
| Reads of unoccupied monster slots, removed bags and out-of-play diggers | Arrays are always fully initialised and keep last values | UB → defined |
| On-screen limit has no value for level < 1 | Unreachable (start level validated) | — |
| Second player's level left over from the previous game | Always reset to level 0 at game start (Q06 covers the fresh-process case recordings exercise) | Accidental → fixed |
| Custom level file read into 16-byte rows | Rows are 15 bytes, as evidently intended | Accidental → fixed |
| Quitting during the player announcement skips the end-of-game bookkeeping | Quit always finishes the game through the same path | Accidental → fixed |
| Timing kludge for recordings made on or before 1998-11-25 | Not ported; the recording change rejects such files | Dropped |

Presentation-only deviations, also listed in the quirks document: the level-complete pause is a fixed 40 frames instead of "until the jingle ends"; the dirge has no real-time hold beyond its 89 ticks; the level draw reveals three items per frame instead of pacing each item by a third of a frame; a pause request is honoured only on a main-phase tick; `GAME OVER` is always shown for 50 frames and initials entry is left to the desktop app, which acts on `StepOutcome::player_out`.

### D14. Additions to `docs/compat-quirks.md`

The file exists since `add-assets-and-renderer` (policy summary and entries R1–R7, S1–S7, A1). This change keeps those and adds:

1. **Purpose and policy** — the existing summary, extended by the rule "recordings win over reports".
2. **Simulation quirks** — the table of D12 with columns *Id, Behaviour, Research reference, Implemented in, Test*.
3. **Undefined and accidental behaviour** — the table of D13 with the same columns plus *Decision*.
4. **Presentation deviations** — the list above, each with the reason.
5. **Not ported** — CGA, networked play, pre-1998 kludge, with the change that rejects each at the user-facing edge.
6. **Findings from recordings** — empty heading, filled by `add-recording-and-conformance`.

A unit test (`quirks_doc_lists_every_quirk_test`) checks that every `quirk_<id>` test name appears in the document and vice versa, so the document cannot drift from the code.

## Risks / Trade-offs

- [The reports were written by reading code; a detail may be wrong] → Every numeric detail sits in one named constant or function with its research reference in a comment; scenario tests document the current reading; the next change's replay suite is the judge, and D11's tick trace and per-site penalty counts are there to localise a divergence.
- [Keeping hashed variables at original granularity makes some structs less tidy] → Accepted and confined to private fields; public and cross-module APIs use enums, `Option` and typed ids. The rule is written down (D5) so it is not "cleaned up" later.
- [`Ctx` is a wide mutable context; any system can touch anything] → This mirrors the real coupling. Discipline comes from module-private fields with narrow `pub(in crate::sim)` methods (RNG reachable only from `monster.rs`; penalty only through `charge`).
- [Uncertain presentation details: when the fall and wobble cues stop, wobble frame order] → They affect sound and pixels only, not simulation state; flagged in the quirks document for confirmation by ear and eye during `add-sound-engine` and `add-desktop-app`.
- [Scenario tests that place entities directly could construct impossible states] → Testkit setters validate alignment and bounds, and each capability also has at least one test that reaches its situation purely through inputs.
- [The 64-entry sound queue could overflow if a frontend forgets to drain] → Oldest entries dropped, debug assertion in tests, and documented on `drain_sound`.
- [Phase 2 derives a sprite's size from its image and has no collision borders] → Both match research 02 §1 for every call this change makes; if a recording disagrees, the fix is local to phase 2's `Sprite::rect` (its design, Risks).

## Open Questions

- Resolved — for how long a removed monster's "object" counts for the upstream state hash: from the spawn into a slot until the round's monster reset at the next round start. A kill only marks the object dead and erases its sprite (research 02 §2 `kill`, §4.7 `killmon`); nothing releases it before the slots are cleared at round start (research 02 §4.2), and research 01 §8 hashes the extra values whenever the object exists. `StateView` therefore exposes "spawned this round" per slot (with the last sprite position, facing, alive and nobbin values) next to "slot occupied"; `add-recording-and-conformance` uses the former as the condition. As everywhere, a recording that disagrees wins.
- Should `LEVEL_DONE_FRAMES` scale with the configured speed so the jingle always fits at very fast speeds? It has no effect on results; the desktop-app change can decide by extending waiting frames on its side.
