# Replay conformance

How the port proves that its simulation behaves like the original: by
replaying upstream's recordings and comparing state-hash checkpoints
(change `add-recording-and-conformance`). This document grows with that
change; later sections cover the corpus, the triage procedure and the
tracking table of all 50 recordings.

## Simulation seam

Recording, playback, verification and tracing need no entry point of their
own into the simulation. They are all built on
`Game::step_observed(&FrameInput, &mut dyn Observer)` and the seven hooks of
`digger_core::Observer` (design D4). One implementation,
`digger_core::replay::ReplayObserver`, holds an optional playback source,
recorder, verifier and trace sink and handles every hook in a fixed order:
the playback source, when there is one, replaces the value; the recorder,
the verifier and the trace sink, in that order, then see the value actually
used. With no component it changes nothing
(`empty_replay_observer_changes_nothing`).

### Where the hooks are called

As merged in `digger-core` (`src/sim/mod.rs`, `src/sim/digger.rs`):

| Hook | Called from | When |
|---|---|---|
| `round_seed(round, derived)` | `Ctx::begin_round` | once at every round start (after a full draw, after an announcement, and on the same field), before anything of the round draws a random number; the returned value seeds the generator |
| `level_plan(player, level, layout)` | `Ctx::start_game`, `Ctx::set_up_level_observed` | at game start for every participating player in player order, and when a player advances to a new level (as the round's outcome is decided, before the next draw); never for the placeholder second player of a non-alternating game (Q06); a returned layout is used verbatim, without the Q07 substitution |
| `controls(tick, slot, raw)` | `Ctx::update_diggers` | once per slot on every main-phase and wind-down tick: each slot in play in digger order before that digger's update, then each slot no digger plays from, whose returned value is dropped; never on waiting or paused frames |
| `movement(slot, sample)` | `Ctx::move_digger` | exactly once per control read: on a tick on which the digger is alive, visible and not in a push pause (Q15) |
| `tick_end(tick, state)` | `Ctx::end_tick` | at the end of every main-phase and wind-down tick, after everything of the tick, with the tick count including it |
| `round_end(state)` | `Ctx::end_round` | once per round, after the clean-up (sound stopped, fireballs and bonus removed, bags tidied and saved, field saved, monster sprites erased) and before the round counter rises; also when a quit ends a round in progress |
| `game_end(state)` | `Ctx::finish` | once when the game finishes for any reason, after the round end when a round was in progress |

The order within one tick of a one-digger game is therefore
`controls(t, 0)`, `movement(0)` if the digger is sampled, `controls(t, 1)`,
`tick_end(t)`; with two diggers `controls(t, 0)`, `movement(0)`,
`controls(t, 1)`, `movement(1)`, `tick_end(t)`
(`observer_event_order_one_round`,
`observer_sees_one_movement_per_movable_tick`).

### Upstream's hook points mapped onto the seam

Every row of design D4's table, and the hook and call point that serves it:

| Upstream hook | Port hook | Call point and test |
|---|---|---|
| `playgetrand` / `edrf_getrand` / `roundseed` | `round_seed` returns the recorded seed | `Ctx::begin_round`, once per round start (`observer_event_order_one_round`) |
| `edrf_input` on playback | `controls` returns the recorded raw controls | `Ctx::update_diggers`, before the slot's direction is resolved, so last-pressed-wins resolution and the fire latch run on the recorded values (`observer_event_order_one_round`) |
| `edrf_tick` consuming unread slots | `controls`, called for every slot on every tick, read or not | `Ctx::update_diggers`; slots no digger plays from are offered after those in play and their value is dropped; dying and hidden diggers' slots are offered too (`controls_hook_called_for_unread_slot`, `movement_hook_only_when_alive_visible_unpaused`) |
| `playgetdir` in `getdirect` | `movement` returns the DRF record's direction and fire | `Ctx::move_digger`, the only sampling point (`movement_hook_only_when_alive_visible_unpaused`, `movement_hook_not_called_in_push_pause`) |
| `edrf_level` on playback | `level_plan` returns the recorded layout, or the source records a level-number mismatch | `Ctx::start_game` and `Ctx::set_up_level_observed` (`level_plan_hook_only_for_participating_players`) |
| `edrf_quitting`, `playend → escape` | the source sets an end flag; the replay driver puts `quit` into the next frame's `FrameInput` | `Game::step_observed` handles `quit` before running a tick: the tick in progress has completed and been counted, a round in progress ends through `round_end`, then `game_end` follows |
| `recputrand`, `recputdir`, `recputeol`, `recputeog` | `round_seed`, `movement`, `round_end`, `game_end`, observed by the DRF recorder | the call points above; the recorder sees the value actually used |
| `edrf_input` / `edrf_movement` / `edrf_tick` / `edrf_roundend` / `edrf_gameend` when recording | `controls`, `movement`, `tick_end`, `round_end`, `game_end` (and `level_plan` for `L` records), observed by the eDRF recorder; a slot counts as read on every tick on which a digger plays from it (slot 0, and slot 1 with two diggers), as upstream's `readdirect` runs for every digger in play on every tick; during DRF playback a read slot's value is the record of its `movement` call, 0 on a tick without one | the call points above (`observer_event_order_one_round`) |
| checkpoint comparison | `tick_end`, `round_end`, `game_end` → verifier | `Ctx::end_tick`, `Ctx::end_round`, `Ctx::finish` (`observer_event_order_one_round`) |

### Gap found and fixed

Task 1.4 found one gap between the seam as merged and design D8 of
`add-simulation-core`, which promised `controls` once per slot on every tick
whether or not the slot is read: the simulation offered only the slots in
play, so slot 1 of a one-digger or alternating game was never offered and an
eDRF playback could not use up its values (game-recording spec, "One-digger
game ignores slot 1"). `Ctx::update_diggers` now offers every slot no digger
plays from after the slots in play and drops the returned value; the test
`controls_hook_called_for_unread_slot` checks the count in all five modes
and that the value returned for an unread slot changes nothing, and
`observer_sees_one_movement_per_movable_tick` was corrected to expect both
slots.

## DRF corpus

The 23 upstream `.drf` files read with the DRF reader alone, without a game
(task 3.5, test `corpus_drf_bodies_tokenise` in
`crates/digger-core/tests/drf_corpus.rs`, which fails when this table is
stale). Every header parses and every body tokenises to its end: seed,
runs up to the end marker, `EOL`, repeated while another seed follows.
"Rounds" is the number of seeds read and equals the number of `R` records
of the file's `.edrf` twin; "Records" is the number of movement records the
runs expand to (one per control read during play); "Trailer" is what
follows the last `EOL`, which playback ignores.

No file holds `EOG`. Three
trailers do not match the `["*" c c c]` of research 05 §4.1: `2test01.drf`
holds its initials twice and `p1092525.drf` and `s1058800.drf` hold three
letters without the `*`. The game-recording spec's body grammar was
corrected to treat everything after the last round as an ignored trailer.

| File | Version line | Mode | Rounds | Records | Trailer |
|---|---|---|---:|---:|---|
| `2test01.drf` | `POK SDL 20110912` | `M2` | 6 | 4722 | `*max*max` |
| `a1104675.drf` | `TD WIN 20020310` | `1` | 168 | 106526 | `*AJU` |
| `a1195000.drf` | `TD WIN 20020310` | `1` | 183 | 113769 | `*AJU` |
| `a1262500.drf` | `AJ DOS 19990506` | `1` | 153 | 256961 | `*alb` |
| `b1072175.drf` | `AJ DOS 19990506` | `1` | 162 | 124369 | (none) |
| `dror0.drf` | `AJ DOS 19990130` | `1` | 4 | 2624 | (none) |
| `f1172525.drf` | `AJ DOS 19990506` | `1` | 182 | 195910 | `*fip` |
| `g1322150.drf` | `TD WIN 20020310` | `1` | 200 | 149422 | `*GRY` |
| `h1266775.drf` | `AJ DOS 19990506` | `1` | 180 | 177207 | (none) |
| `k1859075.drf` | `AJ DOS 19990506` | `1` | 232 | 185635 | `*kkk` |
| `k2223700.drf` | `AJ DOS 19990506` | `1` | 233 | 347385 | `*gkl` |
| `m1143925.drf` | `AJ DOS 19990506` | `1` | 165 | 146176 | `*mjw` |
| `muu27700.drf` | `MS MPL 20190212` | `1` | 8 | 7289 | `*muu` |
| `p1092525.drf` | `AJ DOS 19990506` | `1` | 150 | 116006 | `PLA` |
| `r2222400.drf` | `AJ DOS 19990506` | `1` | 256 | 400557 | `*rtp` |
| `r3333300.drf` | `AJ DOS 19990506` | `1` | 398 | 557981 | `*rtp` |
| `ramon.drf` | `TD WIN 20020310` | `1` | 4 | 6732 | (none) |
| `s1058800.drf` | `AJ DOS 19990506` | `1` | 132 | 108045 | `STO` |
| `s1177650.drf` | `AJ DOS 19990506` | `1` | 180 | 221138 | (none) |
| `s1302025.drf` | `AJ DOS 19990506` | `1` | 192 | 147482 | (none) |
| `sobodig.drf` | `POK SDL 20110912` | `1` | 4 | 1050 | `*mmm` |
| `thevoid.drf` | `AJ DOS 19990506` | `1` | 4 | 3545 | `*gkl` |
| `thevoids.drf` | `AJ DOS 19990506` | `1` | 15 | 40841 | (none) |

## Conversion to eDRF

`cargo xtask convert <in> <out>` plays a recording and records it again,
as eDRF when `<out>` ends in `.edrf` and as DRF otherwise; its exit status
is that of `cargo xtask verify <in>`. Converting each of the 23 DRFs above
to eDRF gives exactly the record content of its upstream `.edrf` twin:
the same level records and maps, seeds, `C`, `E`, `Q` and `Z` records and
the same control value for every slot on every tick
(`crates/digger-core/tests/conversion_twins.rs`, one test per twin, quick
twins in the ordinary run and the rest with `-- --ignored`). The
converted files are also byte-identical to the twins once the version
line is replaced: 23 of 23 (`twin_byte_identity_full`, informational).

## Triage procedure

What to do when a recording fails (design D12). The only evidence used is
the recording's own checkpoints, the port's own per-tick dumps and the
research reports in `docs/research/`. No upstream binary is built or run
at any point, and no upstream C is read to be translated (porting plan,
"No C"); upstream's source may be read to confirm a behaviour, and the fix
is then written independently.

### Order of attack

eDRF before DRF, short before long, one digger before two diggers before
gauntlet: `dror0`, `sobodig`, `thevoid`, `thevoids`, `ramon`, `muu27700`,
then `2test01`, `2test02`, `2test03`, `2riptest01`, `gauntlet01`, then the
16 long games in order of their tick totals. A DRF is attempted only after
its eDRF twin passes; from then on a DRF failure can only come from the
placement of control reads, the two-digger start-cell patch or the DRF's
end handling. A fix found on a short recording often clears long ones, so
rerun the cluster after every fix:

```sh
cargo xtask conformance --tier quick
cargo xtask conformance --filter 2test03
```

All 50, with the long games, run fastest from an optimised build:

```sh
cargo run --release -p xtask -- conformance --tier full
```

### Reading a failure report

`cargo xtask verify <file>` and the conformance report name the first
failing checkpoint and the last one that matched:

```text
eDRF: the game diverges from the recording before tick 2391 (end of round state 119A35EA, recorded 66BA26CE)
last matching checkpoint: tick 2369
eDRF: playback of 2test03.edrf FAILED
```

- *tick state* is a `C` checkpoint (every 256 ticks): the state after that
  tick differs, and the fault happened after the last match.
- *end of round state* is an `E` checkpoint: the round-end hash, taken after
  the round's clean-up and with the tick count appended. If the round ends
  on a different tick from the recording's, the tick count alone makes it
  differ; if the `C` of the same tick matched, the fault is in the clean-up.
- *the game ends differently* is the `Z` record: tick total or final state.
- `level N starts, recorded level M` means a round ended (or a level was
  completed) at a different point.

### Window, block, field

1. **Window.** `bisect` turns the report into the window to examine: from
   the tick after the last matching checkpoint to the failing one, with
   the recording's round ends inside it and the commands to run next:

   ```sh
   cargo xtask bisect tests/recordings/data/2test03.edrf
   ```

   Round-end checkpoints usually cut a window well below 256 ticks. When
   the window is a single tick whose own tick checkpoint matched, `bisect`
   says so: look at the round-end clean-up (monsters removed, bags tidied,
   playfield saved; research 01 §8) and at when the round ended.
2. **Block.** Dump the window, one line per tick with the full hash, the
   hash and byte length of the playfield, digger, monster, bag and game
   blocks, the RNG state and both scores; `--fields` adds every hashed
   value under its tick line:

   ```sh
   cargo xtask dump-state tests/recordings/data/2test03.edrf --from 2370 --to 2391 --fields --out target/triage/2test03-2370-2391.trace
   ```

   The state at the window's start is known to be right, so the fault is
   an event inside the window. Read the dump against the research reports:
   which events happen there (a death, a bag falling or breaking, bonus
   mode, a monster spawning, the level ending) and which of them the
   recording's next checkpoint contradicts. A monster block whose length
   alone changes points at the "monster body exists" rule of design D5.
   The RNG state shows at once when the port draws a different number of
   random numbers.
3. **Field.** The decoded values of the suspect block, tick by tick, show
   which variable moves differently from what the reports describe. The
   per-tick trace has no line for the round-end hash; for an `E` failure
   the last tick's values are the state before the clean-up.

### Comparing candidate fixes

`cut` shortens a recording to the window so that each candidate fix is
re-checked in well under a second; the cut file keeps the recording's
header, maps, seeds, checkpoints and controls up to the tick and ends with
`Q 0`, so it verifies as truncated (status 4) once every checkpoint in it
matches, and as failed (status 2) while one does not:

```sh
cargo xtask cut tests/recordings/data/2test03.edrf --at 2391 --out target/triage/2test03-2391.edrf
cargo xtask verify target/triage/2test03-2391.edrf
```

Dump the window before and after a change and compare the two dumps;
`trace-diff` names the first tick and the first block that differ (and,
with `--fields` dumps, the first value), with exit status 0 when the dumps
agree, 1 when they differ and 2 when a file is not a dump:

```sh
cargo xtask dump-state target/triage/2test03-2391.edrf --from 2370 --to 2391 --fields --out target/triage/before.trace
cargo xtask dump-state target/triage/2test03-2391.edrf --from 2370 --to 2391 --fields --out target/triage/after.trace
cargo xtask trace-diff target/triage/before.trace target/triage/after.trace
```

(Run the second `dump-state` after applying the candidate fix; run on the
unchanged code, as here, the two dumps agree.) The first diverging block
between two candidates narrows the question to digger, monster, bag or
playfield logic. `dump-state` plays on past a failed checkpoint with the
recorded controls, so a window may extend beyond the first failure.

### Recording a finding

1. Fix `digger-core` with a scenario or unit test that fails without the
   fix.
2. If the research report was wrong or silent, add an entry to
   `docs/compat-quirks.md` (class, description, the recording and tick that
   exposed it, the test name) and list it under "Findings from
   recordings". If a simulation spec requirement was wrong, add a MODIFIED
   delta spec to `add-recording-and-conformance`.
3. Update the recording's row in the tracking table below (status, first
   failing checkpoint, cause, fixing commit) and flip its entry in
   `crates/digger-core/tests/conformance.rs` from `pending` to `passing`.
4. Commit per fixed defect (`fix(core): …` naming the quirk).

## Tracking table

One row per corpus recording, generated from the first full-tier report
(`cargo xtask conformance --tier full`, 48 of 50 passing) and kept current
by task group 12. *Status* is `passing` or `failing` and matches the
recording's `passing`/`pending` entry in
`crates/digger-core/tests/conformance.rs`
(`tracking_table_lists_every_recording`).

| Recording | Status | Outcome | First failing checkpoint | Last matching tick | Cause | Fixing commit |
|---|---|---|---|---|---|---|
| `2riptest01.edrf` | failing | failed | E at tick 1024 | 1024 | not yet localised (task group 12) | — |
| `2test01.drf` | passing | completed | — | — | — | — |
| `2test01.edrf` | passing | verified | — | 4489 | — | — |
| `2test02.edrf` | passing | verified | — | 10461 | — | — |
| `2test03.edrf` | failing | failed | E at tick 2391 | 2369 | not yet localised (task group 12) | — |
| `a1104675.drf` | passing | completed | — | — | — | — |
| `a1104675.edrf` | passing | verified | — | 111324 | — | — |
| `a1195000.drf` | passing | completed | — | — | — | — |
| `a1195000.edrf` | passing | verified | — | 119146 | — | — |
| `a1262500.drf` | passing | completed | — | — | — | — |
| `a1262500.edrf` | passing | verified | — | 260987 | — | — |
| `b1072175.drf` | passing | completed | — | — | — | — |
| `b1072175.edrf` | passing | verified | — | 130190 | — | — |
| `dror0.drf` | passing | completed | — | — | — | — |
| `dror0.edrf` | passing | verified | — | 2947 | — | — |
| `f1172525.drf` | passing | completed | — | — | — | — |
| `f1172525.edrf` | passing | verified | — | 201088 | — | — |
| `g1322150.drf` | passing | completed | — | — | — | — |
| `g1322150.edrf` | passing | verified | — | 155195 | — | — |
| `gauntlet01.edrf` | passing | verified | — | 1790 | — | — |
| `h1266775.drf` | passing | completed | — | — | — | — |
| `h1266775.edrf` | passing | verified | — | 181703 | — | — |
| `k1859075.drf` | passing | completed | — | — | — | — |
| `k1859075.edrf` | passing | verified | — | 189762 | — | — |
| `k2223700.drf` | passing | completed | — | — | — | — |
| `k2223700.edrf` | passing | verified | — | 353132 | — | — |
| `m1143925.drf` | passing | completed | — | — | — | — |
| `m1143925.edrf` | passing | verified | — | 151534 | — | — |
| `muu27700.drf` | passing | completed | — | — | — | — |
| `muu27700.edrf` | passing | verified | — | 7757 | — | — |
| `p1092525.drf` | passing | completed | — | — | — | — |
| `p1092525.edrf` | passing | verified | — | 119931 | — | — |
| `r2222400.drf` | passing | completed | — | — | — | — |
| `r2222400.edrf` | passing | verified | — | 405894 | — | — |
| `r3333300.drf` | passing | completed | — | — | — | — |
| `r3333300.edrf` | passing | verified | — | 565330 | — | — |
| `ramon.drf` | passing | completed | — | — | — | — |
| `ramon.edrf` | passing | verified | — | 7051 | — | — |
| `s1058800.drf` | passing | completed | — | — | — | — |
| `s1058800.edrf` | passing | verified | — | 110574 | — | — |
| `s1177650.drf` | passing | completed | — | — | — | — |
| `s1177650.edrf` | passing | verified | — | 226318 | — | — |
| `s1302025.drf` | passing | completed | — | — | — | — |
| `s1302025.edrf` | passing | verified | — | 152165 | — | — |
| `sobodig.drf` | passing | completed | — | — | — | — |
| `sobodig.edrf` | passing | verified | — | 1378 | — | — |
| `thevoid.drf` | passing | completed | — | — | — | — |
| `thevoid.edrf` | passing | verified | — | 3866 | — | — |
| `thevoids.drf` | passing | completed | — | — | — | — |
| `thevoids.edrf` | passing | verified | — | 41673 | — | — |
