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
| `edrf_input` / `edrf_movement` / `edrf_tick` / `edrf_roundend` / `edrf_gameend` when recording | `controls`, `movement`, `tick_end`, `round_end`, `game_end` (and `level_plan` for `L` records), observed by the eDRF recorder; a slot counts as read on a tick when `movement` was called for it between its `controls` call and `tick_end` | the call points above (`observer_event_order_one_round`) |
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
