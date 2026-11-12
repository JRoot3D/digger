# Compatibility quirks

This file lists every place where the port deliberately does what the
original Digger Remastered (sobomax/digger at commit 638125e) does instead of
what a clean implementation would do, or gives defined behaviour where the
original had none. Each entry names the code that isolates it (a comment
carrying the quirk id sits at that spot) and the test that pins it.

## Policy

Summary of the quirk policy in [`docs/porting-plan.md`](porting-plan.md)
§3. Every deviation belongs to one of four classes:

| Class | Policy |
|---|---|
| Affects the simulation | Reproduce exactly, named in code, covered by a test |
| Undefined behaviour in C | Give it defined, safe behaviour; match what recordings observe where they exercise it; reject invalid mode combinations at configuration time |
| Presentation or usability bug | Fix |
| Dropped feature (CGA, NetSim, X11 embed, pre-1998 recording kludge) | Not ported; rejected with a clear message where a user could ask for it |

The picture is game state (the fireball reads pixels back to find walls),
so anything that changes which pixels are drawn counts as affecting the
simulation even when it looks like presentation.

When a recording disagrees with a research report or with an entry here,
the recording wins and the entry is corrected. Recordings win over reports:
the research reports in `docs/research/` were written by reading the
original's code, so every entry derived from them stands until an upstream
recording says otherwise, and each correction is listed under "Findings from
recordings".

Id prefixes, so that entries from different changes never collide: `R`
rendering, `S` sprite layer, `A` asset data (all from
`add-assets-and-renderer`); `Q` simulation and `U` undefined or accidental
behaviour of the original (`add-simulation-core`); `P` recording and
playback; `SQ` and `SF` sound.

Test names below are given with their module path inside the `digger-core`
crate; run one with `cargo test -p digger-core <name>`.

## Rendering (`add-assets-and-renderer`)

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| R1 | The pixel probe reads an unaligned 8x2 block of real pixels at real (2x, 2y), ORs the two rows, and masks the result with 0xEE; beyond logical x 319 or y 199 it returns an unmasked 0xFF (research 03 §4.6) | Affects the simulation | `Framebuffer::probe` (`crates/digger-core/src/video/framebuffer.rs`) | `video::tests::probe_vectors` |
| R2 | A probe near the right edge runs into the next real row, because the picture is addressed linearly; reads before the start or past the end of the picture are 0, where upstream reads out of bounds. Research 03 §4.6 argues the game never probes there; this result is a choice, to be corrected if a recording shows otherwise | Undefined behaviour in C, given a defined result | `Framebuffer::pixel_at_offset` (`video/framebuffer.rs`) | `video::tests::probe_row_wrap`, `video::tests::probe_outside_buffer_reads_zero` |
| R3 | Selecting a palette also resets the intensity to normal (research 03 §3) | Affects the picture | `Framebuffer::set_palette` (`video/framebuffer.rs`) | `video::tests::palette_select_resets_intensity` |
| R4 | The masked blit treats an image byte of 255 as transparent in addition to the mask: such a pixel is left unchanged, any other sets `(pixel AND mask) OR image` (research 03 §4.5, §13 item 9) | Affects the simulation (probe input) | `Framebuffer::blit_parts` (`video/framebuffer.rs`) | `video::tests::blit_rule_truth_table` |
| R5 | Text is opaque, a full 12x12 logical cell including its black background; inside a string a character without a glyph erases its cell like a space; a lone character without a glyph draws nothing (research 03 §6) | Affects the simulation (probe input) | `glyph_or_space` (used by `Framebuffer::draw_text`) and `Framebuffer::draw_char` (`video/text.rs`) | `video::text::tests::text_cell_is_opaque`, `video::text::tests::text_invalid_char_becomes_space` |
| R6 | Number drawing leaves the leftmost cell untouched, not erased, when its digit is zero (research 03 §6) | Affects the simulation (probe input) | `Framebuffer::draw_number` (`video/text.rs`) | `video::text::tests::number_leftmost_cell_not_erased` |
| R7 | Background tiling starts at logical y 14 and its last row of tiles is clipped at y 199; rows 12–13 are never drawn by it (research 03 §8.2) | Affects the simulation (probe input) | `Framebuffer::draw_background` (`video/backdrop.rs`) | `video::backdrop::tests::background_coverage` |

## Sprite layer (`add-assets-and-renderer`)

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| S1 | x is rounded down to a multiple of 4 (towards minus infinity) by place, move and static draw, but not for the protected area of a static draw, for text or for the probe (research 03 §2, §7.5, §13 item 8) | Affects the simulation | `geom::align_x` (`crates/digger-core/src/geom.rs`) and its call sites in `SpriteLayer::place`, `move_to`, `draw_static` | `sprite::tests::alignment_sites` |
| S2 | Collision lists are produced only by a move and persist until the next move (or until game logic empties them) (research 03 §7.4, §13 item 3) | Affects the simulation | `SpriteLayer::move_to` (`sprite/mod.rs`) | `sprite::tests::collisions_only_on_move` |
| S3 | A move restores the sprite's saved background at its recorded position even when the sprite is disabled; saved backgrounds survive erase and reset, so a move after an erase pastes a stale background (the blinking digger) (research 03 §7.4, §7.6) | Affects the simulation | `SpriteLayer::move_to` (`sprite/mod.rs`) | `sprite::tests::move_after_erase_restores_stale_background` |
| S4 | Placing an already enabled sprite does not erase its old image and, when it overlaps another sprite at the new position, writes its old saved background there before the new capture (research 03 §7.4) | Affects the simulation | `SpriteLayer::place` (`sprite/mod.rs`) | `sprite::tests::place_enabled_sprite_quirk` |
| S5 | Other sprites collide with a sprite's current (last drawn) image size, for example 16x14 for image 80, not with its pending image (research 03 §7.2, §7.4) | Affects the simulation | `Sprite::rect` (`sprite/mod.rs`) | `sprite::tests::collision_uses_current_size` |
| S6 | Resetting an enabled sprite forgets it without erasing it: its image stays on the picture (research 03 §7.3, §13 item 15) | Affects the simulation | `SpriteLayer::reset` (`sprite/mod.rs`) | `sprite::tests::reset_does_not_erase` |
| S7 | Stacking order is slot order (higher slot on top, whichever sprite moved last); sprites that game logic regards as dead but are still enabled collide and are redrawn (research 03 §7.3; research 02 §1) | Affects the simulation | ascending iteration of the slot bit sets (`sprite/id.rs`, `sprite/collisions.rs`) | `sprite::tests::stacking_order`, `sprite::tests::enabled_not_alive` |

## Asset data (`add-assets-and-renderer`)

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| A1 | Image numbers 88–93 duplicate 82–87 and 57–61 duplicate 26–30; the duplicates are kept so that the +31 (second digger) and +32 (second fireball) offsets hold for every frame (research 03 §5.2) | Data layout | the generated index `assets/index.rs` and `ImageId::for_second_digger`, `ImageId::for_second_fireball` (`crates/digger-core/src/assets/image.rs`) | `assets::image::tests::duplicate_image_numbers` |

## Simulation quirks (`add-simulation-core`)

Class: affects the simulation. Each quirk is reproduced exactly, isolated
in one named place carrying a comment with its id, and pinned by one test
named `quirk_<id>` (design D12 of `add-simulation-core`). The last two
columns are filled in as the quirks are implemented.

| Id | Behaviour | Research reference | Implemented in | Test |
|---|---|---|---|---|
| Q01 | Penalty counter: every draw and scoring site of the scoring spec's penalty table raises it, a scoring call three times even for 0 points | research 01 §4, research 02 §6 | `PenaltySite` and `Penalty::charge` (`crates/digger-core/src/sim/penalty.rs`); the three charges of every scoring call in `Scores::add` (`crates/digger-core/src/sim/score.rs`); the other sites are charged by the systems of task groups 8 to 13 | `sim::penalty::tests::penalty_resets_and_counts_per_site`, `sim::score::tests::score_call_charges_three_even_for_zero`, `sim::score::tests::bag_kill_makes_two_calls_with_zero_points` |
| Q02 | Penalty overflow delays monster slots 1..n−1 only; slot 0 is exempt, an excess of 1 does nothing, unoccupied slots are included | research 01 §4, research 02 §6, §9 item 1 | `Penalty::overflow_slots` (`crates/digger-core/src/sim/penalty.rs`), applied by `Monsters::apply_penalty_overflow` (`crates/digger-core/src/sim/monster.rs`) at the end of the main-phase tick only (`Ctx::tick_main`, `sim/mod.rs`) | `sim::penalty::tests::quirk_q02_overflow_table`, `quirk_q02_slot_0_never_delayed_by_overflow` (`crates/digger-core/tests/monsters.rs`) |
| Q03 | Random draws are taken even when their result is unused: the noise draw at difficulty 6 and above, the double-move draw for hobbins. A draw of 0 gives a second move to a nobbin that is still alive after its first, whether or not a digger is alive (the monsters spec said "a digger alive"; confirmed against upstream `domonsters`, which tests the monster's own life, and the spec corrected) | research 02 §4.3, §4.4, §8 | `Ctx::decide` (the noise draw before `apply_noise`) and `Ctx::update_monsters` (the double-move draw) (`crates/digger-core/src/sim/monster.rs`) | `sim::monster::tests::quirk_q03_noise_draw_taken_at_level_8_without_effect`, `quirk_q03_hobbin_draws_but_never_double_moves`, `nobbin_double_moves_when_draw_is_zero`, `eaten_nobbin_makes_no_second_move` (`crates/digger-core/tests/monsters.rs`) |
| Q04 | Wind-down updates bags, then diggers, then monsters; monsters and the generator keep running; it lasts 1 to 20 ticks by the moving-bag and penalty rule | research 01 §3.2, research 02 §6 | `Ctx::wind_down_frame`, entered from `Ctx::end_main_phase` (`crates/digger-core/src/sim/mod.rs`): no penalty overflow, no level-completion test, no pause; the hidden diggers take no movement sample (`Ctx::update_diggers`, `sim/digger.rs`) | `quirk_q04_monsters_draw_random_numbers_in_wind_down`, `quiet_field_winds_down_in_1_tick`, `busy_field_winds_down_in_20_ticks`, `wobbling_bag_extends_wind_down` (`crates/digger-core/tests/game_flow.rs`) |
| Q05 | A full redraw opens every partly dug cell completely | research 01 §5.3, §10 item 8, research 03 §8.2 | `Field::normalise_for_redraw` (`crates/digger-core/src/sim/field.rs`), to be called by the full level draw (task group 4) | `sim::field::tests::quirk_q05_redraw_opens_partly_dug_cells`, `sim::field::tests::untouched_cells_survive_redraw` |
| Q06 | The second player's level state is initialised as level 0 in modes without alternating players | research 01 §3.1, §10 item 18 | `Sim::new`, called by `Game::new` (`crates/digger-core/src/sim/mod.rs`): the second player's level is `None` (level 0) and `Sim::set_up_level` gives it an all-earth field and no emeralds; the observer's `level_plan` hook is never called for it | `sim::tests::quirk_q06_second_player_position_is_level_0` |
| Q07 | With two diggers on the built-in levels, layouts 3 and 4 are read with `H` at cells (6,9) and (8,9) | research 01 §5.5, §10 item 7 | `LevelSet::cell_char` (`crates/digger-core/src/sim/level.rs`); `LevelSet::layout_as_played` reads every cell through it | `sim::level::tests::quirk_q07_two_digger_start_cells_open_on_layouts_3_and_4`, `sim::level::tests::q07_not_applied_to_custom_or_verbatim_levels` |
| Q08 | Level sequence 1–8, then 6–8, then 5–8 repeating; level capped at 1000; difficulty frozen at 10 | research 01 §5.5, §10 item 4, research 02 §7 | `LevelNumber::plan`, `LevelNumber::next` and `LevelNumber::difficulty` (`crates/digger-core/src/sim/level.rs`) | `sim::level::tests::layout_sequence_1_to_13`, `sim::level::tests::level_1000_stays_and_uses_layout_5`, `sim::level::tests::difficulty_is_min_level_10` |
| Q09 | The second score's bonus life arrives one point late | research 01 §6.2, §10 item 1 | `Scores::add` (`crates/digger-core/src/sim/score.rs`) | `sim::score::tests::quirk_q09_second_score_needs_20001` |
| Q10 | Lives are capped at 5 while the bonus threshold still advances; one threshold step per scoring call | research 01 §6.2 | `Scores::add` (`crates/digger-core/src/sim/score.rs`), which returns the bonus as a `BonusEffect` for the caller to apply | `sim::score::tests::quirk_q10_cap_at_5_threshold_advances`, `sim::score::tests::one_threshold_step_per_call`, `sim::score::tests::unlimited_lives_passes_cap` |
| Q11 | At one million the visible score rolls over into a carried total, and no bonus lives are awarded afterwards. The redraw after the rollover leaves the leftmost digit position as it was (quirk R6), so a stale digit stays on screen beside `00000` until the score again reaches 100,000 (confirmed against upstream `writenum`) | research 01 §6.2, §10 item 3 | `Scores::add` and `Scores::total` (`crates/digger-core/src/sim/score.rs`) | `sim::score::tests::quirk_q11_rollover_moves_score_to_carried_total`, `sim::score::tests::six_digit_score_from_x0` |
| Q12 | Score awards are truncated to signed 16 bits; the monster-eating multiplier doubles with 16-bit wrap | research 01 §6.2, §9, research 02 §3.8 | `award_i16`, applied by `Scores::add` to every award (`crates/digger-core/src/sim/score.rs`); the multiplier's doubling is `Digger::bump_eat_multiplier` (`sim/digger.rs`), called by `Ctx::eat_monster` (`sim/monster.rs`) | `sim::score::tests::quirk_q12_award_51200_adds_minus_14336` |
| Q13 | The gauntlet clock counts in 1,193,181ths of a second, −80,000 per tick with wrap; bonus time adds 17,897,715 with wrap | research 01 §7.4 | `GauntletClock::count_down` and `GauntletClock::add_bonus` (`crates/digger-core/src/sim/gauntlet.rs`); the countdown runs at the start of `Ctx::update_diggers` (`sim/digger.rs`) in the main phase and the wind-down alike, after the clock is drawn | `quirk_q13_120s_gauntlet_times_out_on_tick_1790_clock_4294949016`, `gauntlet_bonus_adds_17897715`, `gauntlet_clock_runs_while_dying`, `time_up_shown_for_50_frames_then_finished` (`crates/digger-core/tests/game_flow.rs`), `sim::gauntlet::tests::countdown_marks_time_out_and_wraps`, `sim::gauntlet::tests::bonus_adds_17897715_with_wrap` |
| Q14 | A level advances when no emeralds remain even if the digger died on that tick; the dead digger still loses a life. Only a level marked complete sounds the level-done cue and pauses for 40 frames; the outcome (lives, next level) is decided when that pause ends, as the original decides it after its wait for the jingle (confirmed against upstream `game`) | research 01 §3.2, §10 item 11 | `Ctx::decide_outcome`, reached through `Ctx::round_end` (`crates/digger-core/src/sim/mod.rs`) | `quirk_q14_last_emerald_on_death_tick_advances_and_costs_a_life`, `completing_level_2_draws_level_3_without_life_loss`, `level_done_pause_is_40_wait_frames` (`crates/digger-core/tests/game_flow.rs`) |
| Q15 | Movement samples are taken only while a digger is alive, visible and not in a push pause; fire is latched one tick late. The fire control is read on every tick, so fire held on a tick without a sample (a push pause, a dead digger) still takes effect on the next tick and belongs to no sample: a replay fed only seeds and samples does not see it, as the original's DRF playback reads it from the live controls (found by the observer round-trip test of task 15.2; confirmed against upstream `readdirect`, which sets `firepflag` every tick, and `getdirect`, the only place `playgetdir` replaces it; the game-flow spec's "Externally supplied seeds and samples" was qualified accordingly) | research 02 §3.4, research 05 §1.6, §1.7, §4.3 | `Ctx::update_diggers` and `Ctx::move_digger`, the only place a sample is taken and handed to the observer's `movement` hook (`crates/digger-core/src/sim/digger.rs`); the fire latch in `SlotInput::advance_fire` and `SlotInput::apply_sample` (`crates/digger-core/src/sim/input.rs`), advanced at the top of every tick by `Ctx::tick_main` (`sim/mod.rs`) | `sim::digger::tests::quirk_q15_no_movement_sample_during_push_pause`, `sim::input::tests::fire_read_on_tick_n_is_active_on_tick_n_plus_1`, `observer_sees_one_movement_per_movable_tick` (`crates/digger-core/tests/digger.rs`), `quirk_q15_fire_pressed_during_push_pause_fires_without_a_sample` (`crates/digger-core/tests/gold_bags.rs`) |
| Q16 | Emerald thresholds per direction: redraw at remainder 8/12/16/6, eat at 12/9/12/9 (right/up/left/down) | research 02 §3.5 | `EMERALD_STEPS`, applied by `hit_emerald` (`crates/digger-core/src/sim/digger.rs`) | `quirk_q16_emerald_eaten_at_x_64_moving_right`, `emerald_eaten_moving_up_at_remainder_9` (`crates/digger-core/tests/digger.rs`), `sim::digger::tests::emerald_steps_table` |
| Q17 | A digger pauses after every horizontal push attempt, successful or not | research 02 §3.4 | `Ctx::meet_bags` (`crates/digger-core/src/sim/digger.rs`): the push pause is set after every horizontal push attempt, whatever its result | `quirk_q17_push_pause_after_failed_push`, `free_push_moves_bag_4_px_and_slides` (`crates/digger-core/tests/gold_bags.rs`) |
| Q18 | A blocked digger is redrawn (an extra invincibility decrement) and its facing reversed | research 02 §3.4, §3.7 | `Ctx::meet_bags` (`crates/digger-core/src/sim/digger.rs`): the blocked digger is put back, drawn once more through `Ctx::draw_digger` (which counts its invincibility down) facing the way it moved, then faces the other way | `quirk_q18_blocked_digger_faces_back`, `stationary_overlap_leaves_digger_without_facing` (the second decrement), `chain_against_wall_fails_and_reverses_first_bag` (`crates/digger-core/tests/gold_bags.rs`) |
| Q19 | Fireballs find earth by the pixel probe with per-direction sample patterns; the probe is ignored while touching any sprite, the own digger included | research 02 §3.6, research 03 §4.6 | `probe_earth`, read by `Ctx::fly` after the move and before the redraw; `Ctx::fly` steps back and explodes only when the draw touched no sprite (`crates/digger-core/src/sim/fireball.rs`) | `quirk_q19_shot_into_earth_explodes_at_previous_position`, `quirk_q19_probe_ignored_while_touching_a_sprite` (`crates/digger-core/tests/digger.rs`), `sim::fireball::tests::probe_ignored_while_touching_a_tombstone`, `sim::fireball::tests::probe_masks_per_direction` |
| Q20 | Kill rule: a digger can be killed again while carried by a bag or in the death arc, not during the grave stages | research 02 §3.4, §3.7, §9 items 13, 15 | `Sim::kill_digger` (`crates/digger-core/src/sim/digger.rs`), called by the fireball's hits and by the test kit's `Run::kill_digger` | `sim::digger::tests::quirk_q20_kill_ignored_in_grave_stages`, `sim::digger::tests::quirk_q20_carried_digger_can_be_rekilled` |
| Q21 | A crushed digger is held at bag top + 6; a squashed monster snaps to the bag top without the 6 | research 02 §3.7, §4.7, §5.7, §9 items 6, 7 | Digger side: `CARRIED_BELOW_BAG`, applied by `Ctx::carried_by_bag` (`crates/digger-core/src/sim/digger.rs`) to diggers killed by `Ctx::crush` (`sim/bags.rs`); monster side: `SNAP_RANGE`, applied by `Ctx::monster_die` (`sim/monster.rs`) to monsters squashed by `Ctx::squash_monsters` | `quirk_q21_crushed_digger_carried_at_bag_plus_6` (`crates/digger-core/tests/gold_bags.rs`), `sim::digger::tests::carried_digger_goes_to_grave_once_bag_rests`, `sim::monster::tests::quirk_q21_squashed_monster_snaps_to_bag_top`, `squashed_nobbin_scores_250_two_ticks_after_bag_stops` (`crates/digger-core/tests/monsters.rs`) |
| Q22 | Monsters on a fresh grave are removed without score | research 02 §3.7 | `Ctx::grave` (`crates/digger-core/src/sim/digger.rs`): the monsters the first tombstone frame's draw touches are removed through `Ctx::remove_monster`, with no scoring call | `sim::digger::tests::quirk_q22_grave_removal_scores_nothing` |
| Q23 | A one-digger death waits 89 ticks from the first tombstone frame | research 02 §3.7, §9 items 14, 23 | `DIRGE_TICKS`, set as the digger's dirge wait by `Ctx::grave`, counted down by `Ctx::digger_die` and waited for by `Ctx::tombstone` (`crates/digger-core/src/sim/digger.rs`) | `quirk_q23_one_digger_round_ends_at_g_plus_89`, `tombstone_frames_at_g_g3_g6_g9_g12` (`crates/digger-core/tests/digger.rs`) |
| Q24 | "All diggers dead" is tested over absolute digger indices; the eat-multiplier reset and the respawn facing likewise | research 02 §3.7, §3.10 | `Sim::all_records_dead`, read by `Ctx::tombstone`; `start_of`, used for the start and the respawn facing (`crates/digger-core/src/sim/digger.rs`); the eat-multiplier reset at the start of bonus mode, over records 0 up to the number of diggers, is `Ctx::start_bonus_mode` (`crates/digger-core/src/sim/bonus.rs`). The original's `clearfire(n)` at the resolution, also by absolute index, has nothing to clear in the port: it forgets a fire press reported since the last read, and the slot's read earlier in the same update has already taken it | `sim::digger::tests::quirk_q24_all_dead_uses_absolute_indices`, `sim::digger::tests::second_alternating_player_faces_left`, `sim::bonus::tests::quirk_q24_bonus_mode_resets_multipliers_by_absolute_index` |
| Q25 | Respawn does not restore the fire-ready look | research 02 §3.7, §3.10 | `Digger::respawn` (`crates/digger-core/src/sim/digger.rs`), which leaves the look, the death counters and the dirge wait as they were | `sim::digger::tests::quirk_q25_respawn_keeps_fire_look` |
| Q26 | Outside bonus mode only a monster's own move kills a digger; in bonus mode either side's move eats the monster | research 02 §3.4, §3.8, §4.4 | Monster side: `Ctx::meet_diggers_as_monster`, called by `Ctx::monster_move` with the collisions of the move draw at the attempted position (`crates/digger-core/src/sim/monster.rs`); digger side: `Ctx::meet_monsters_as_digger`, called by `Ctx::move_digger` with the collisions of the digger's move draw (`sim/digger.rs`), which does nothing outside bonus mode and in bonus mode removes every listed monster, then sounds the eat cue and scores once per monster | `quirk_q26_digger_walking_into_monster_dies_on_monsters_move`, `monster_running_into_digger_in_bonus_mode_is_eaten`, `eaten_by_two_diggers_scores_both_once_removed`, `invincible_digger_not_killed_by_monster` (`crates/digger-core/tests/monsters.rs`), `two_monsters_eaten_in_one_move_score_200_then_400`, `digger_moving_into_monster_outside_bonus_mode_eats_nothing` (`crates/digger-core/tests/bonus_mode.rs`) |
| Q27 | A monster's side effects (decision, draw, digging, collisions) persist when its move is undone | research 02 §4.4 | `Ctx::monster_move` (`crates/digger-core/src/sim/monster.rs`): the step is undone after the decision, digging, carving and emerald, and before the draw whose collisions are met | `quirk_q27_frozen_monsters_still_draw_random_numbers`, `new_monster_stands_still_for_five_moves` (`crates/digger-core/tests/monsters.rs`) |
| Q28 | With no passable priority a monster takes the reverse direction; a hobbin sticks at the edge; an overlapped monster is reversed between cells | research 02 §4.4, §4.6 | `Ctx::decide` (the reverse fallback; a hobbin takes its first priority) and `Ctx::meet_monsters` (the overlapped monster reversed between cells) (`crates/digger-core/src/sim/monster.rs`); the edge stop in `Ctx::monster_move` | `sim::monster::tests::quirk_q28_dead_end_reverses`, `quirk_q28_hobbin_stuck_at_edge`, `follower_reverses_leader`, `adjacent_but_unconnected_tunnel_does_not_qualify` (`crates/digger-core/tests/monsters.rs`) |
| Q29 | The transformation test also runs for dead monsters still carried by a bag | research 02 §4.3, §4.5, §9 item 24 | `Ctx::update_monsters` (`crates/digger-core/src/sim/monster.rs`): the test runs for every occupied slot before the life check | `sim::monster::tests::quirk_q29_carried_dead_nobbin_still_transforms` |
| Q30 | A dead left-facing hobbin is 14 pixels tall | research 02 §4.7, §9 | `sprite_for` (`crates/digger-core/src/sim/monster.rs`): image 80 is 16×14 (`assets/index.rs`), and other sprites collide with the image last drawn (quirk S5) | `sim::monster::tests::quirk_q30_dead_left_hobbin_is_14_tall` |
| Q31 | Any monster removal in bonus mode raises the round's monster total | research 02 §3.8, §4.7 | `Ctx::remove_monster` (`crates/digger-core/src/sim/monster.rs`), the one path by which a fireball, a grave, an eat and the end of a squash remove a monster | `sim::monster::tests::quirk_q31_bonus_mode_removal_raises_total`, `monster_running_into_digger_in_bonus_mode_is_eaten` (`crates/digger-core/tests/monsters.rs`), `eaten_monster_returns_after_bonus_mode` (`crates/digger-core/tests/bonus_mode.rs`) |
| Q32 | Bags share one push counter, set by every sideways step, a bag's own slide step included, so a lone sliding bag moves on every second tick (research 02 §5.3 reads it as set only by a pusher; confirmed against upstream `pushbag`, which sets it for every sideways step); a push that fails because of an overlap sets the bag's direction to the reverse, while one refused at the playfield edge changes nothing; a failed chain restores twice | research 02 §5.3, §5.5, §9 item 9 | `Ctx::push_bag`, `Ctx::slide_step` and `Ctx::restore_bag`, with the counter read by `Ctx::update_bag` (`crates/digger-core/src/sim/bags.rs`) | `quirk_q32_failed_push_rolls_bag_back_to_corner`, `quirk_q32_shared_counter_lets_second_bag_move_twice`, `quirk_q32_failed_chain_that_touches_a_monster_restores_twice`, `one_push_slides_one_cell_in_8_ticks` (`crates/digger-core/tests/gold_bags.rs`) |
| Q33 | Pushing a falling bag sideways succeeds without moving it and crushes the pusher | research 02 §5.5, §5.7, §9 item 10 | `Ctx::push_falling_bag`, called by `Ctx::push_bag` (`crates/digger-core/src/sim/bags.rs`) | `quirk_q33_walking_into_falling_bag_kills_without_moving_it`, `digger_above_falling_bag_is_safe` (`crates/digger-core/tests/gold_bags.rs`) |
| Q34 | A landing bag removes the bags it overlaps; gold over a dug cell expires early; a bag breaks after falling two cells | research 02 §5.2, §5.4, §9 item 11 | `Ctx::land` (overlapped bags removed, break after two cells) and `Ctx::update_gold` (early end over a dug cell) (`crates/digger-core/src/sim/bags.rs`) | `quirk_q34_landing_bag_removes_overlapped_bag`, `two_cell_drop_breaks`, `one_cell_drop_stays_intact`, `gold_over_dug_cell_gone_within_10_ticks` (`crates/digger-core/tests/gold_bags.rs`) |
| Q35 | A digger takes gold whenever it overlaps it, whoever triggered the collection | research 02 §5.6, §9 item 12 | `Ctx::collect_gold` (`crates/digger-core/src/sim/bags.rs`): the diggers its own draw touches score, whoever pushed; otherwise the monster's gold flag is set | `sim::bags::tests::quirk_q35_digger_wins_gold_tie_with_monster`, `digger_collects_gold_for_500` (`crates/digger-core/tests/gold_bags.rs`) |
| Q36 | A bag's wobble is suppressed by a vertically moving digger in the bag's row or the row below | research 02 §5.3 | `Sim::digger_under_bag` (`crates/digger-core/src/sim/digger.rs`), asked by `Ctx::update_bag` (`sim/bags.rs`) | `quirk_q36_no_wobble_while_digger_climbs_underneath`, `bag_wobbles_when_cell_below_is_dug` (`crates/digger-core/tests/gold_bags.rs`) |
| Q37 | Round-end bag tidy rules, including removal of a bag at cell (7,9) | research 02 §5.1, §5.8 | `Bag::tidied_away`, applied by `Ctx::tidy_bags` in the round end (`crates/digger-core/src/sim/bags.rs`, `sim/mod.rs`) | `sim::bags::tests::quirk_q37_tidy_removes_wobbling_broken_offgrid_and_7_9_bags`, `resting_bag_survives_a_death` (`crates/digger-core/tests/gold_bags.rs`) |
| Q38 | A hobbin's dig step is charged even with no direction; hobbins and falling bags destroy emeralds without score | research 02 §4.4, §5.3 | `Ctx::monster_move` (`crates/digger-core/src/sim/monster.rs`): the dig step is charged inside `Field::dig` for every hobbin move, and `hit_emerald` (`sim/digger.rs`) removes the emerald with no scoring call; the bag side is `Ctx::fall_step` (`sim/bags.rs`) | `quirk_q38_hobbin_destroys_emerald_without_score`, `quirk_q38_hobbin_dig_charged_without_direction` (`crates/digger-core/tests/monsters.rs`), `falling_bag_removes_emerald_without_score` (`crates/digger-core/tests/gold_bags.rs`) |
| Q39 | Zero-point scoring calls at round start, after the `PLAYER n` announcement, after a pause and at game end perform the threshold check. Every one of them calls scores 0..diggers, so in an alternating game it is always the first score, on either player's turn (confirmed against upstream `initscores`, `run_pause` and `endofgame`; the game-flow spec said "each of their diggers" for the game end and was corrected). A bonus life awarded by the game end's calls is kept: `GAME OVER` is still shown and reported, and then the same player plays on, because the original tests the lives again only after the notice (confirmed against upstream `game`) | research 01 §3.2, §3.3, §6.2, §6.3 | `Ctx::refresh_scores`, called by `Ctx::begin_round`, `Ctx::resume` and `Ctx::decide_outcome`; the lives read afresh in `Ctx::continue_after_round` (`crates/digger-core/src/sim/mod.rs`) | `quirk_q39_zero_point_call_awards_pending_bonus` (`crates/digger-core/tests/game_flow.rs`) |
| Q40 | The gold lifetime is one game-wide value, set to 150 − 10 × difficulty each time a player's level is set up, from that player's level. At game start the second player's level is set up last, so in a one-player game (second player at the level 0 placeholder of Q06) the first level's gold lasts 150 ticks, not 140; in an alternating game it follows the second player's level until a level is completed, and after a turn change it keeps the value of the level set up last (found while implementing gold: upstream `initbags` sets the global `goldtime` from `levof10()` of the player being set up, and `initlevel` runs for both players at game start) | research 02 §5.1, §7, research 01 §3.1 | `gold_time` and `Sim::create_bags`, called by `Sim::set_up_level` (`crates/digger-core/src/sim/bags.rs`, `sim/mod.rs`); `Ctx::start_game` keeps the value when the observer's layouts set a level up again | `quirk_q40_one_player_gold_lifetime_is_150_on_the_first_level`, `level_1_gold_disappears_139_ticks_after_landing` (`crates/digger-core/tests/gold_bags.rs`), `sim::bags::tests::gold_lifetime_table_140_to_50` |

## Undefined and accidental behaviour (`add-simulation-core`)

Cases where the original's behaviour is undefined in C or plainly
accidental, and the decision taken for each (design D13 of
`add-simulation-core`).

| Id | Behaviour | Research reference | Decision | Class | Implemented in | Test |
|---|---|---|---|---|---|---|
| U01 | Firing with no facing aborts the program | research 02 §3.4, §9 item 18 | The fire request is ignored and the fireball stays ready (digger spec, "Digger left without a facing"). A digger left without a facing exactly under a bag stays stuck: a sideways move pushes the bag, and the push is refused because the bag's new position overlaps a living digger, the pusher included (found while mapping the spec's scenarios to tests; confirmed against upstream `pushbag`, whose `digalive` loop does not skip the pusher; the digger spec's "Facing recovered" now says the move must take the digger clear of the bag) | Undefined behaviour in C, given a defined result | `Ctx::update_fire` (`crates/digger-core/src/sim/fireball.rs`) | `sim::fireball::tests::fire_with_no_facing_is_ignored`, `stationary_overlap_leaves_digger_without_facing`, `facing_recovered_on_a_cell_corner`, `facing_none_digger_under_a_bag_stays_blocked` (`crates/digger-core/tests/gold_bags.rs`) |
| U02 | The pixel probe beyond the screen reads outside the picture | research 02 §3.6, research 03 §4.6 | The phase-2 probe's defined result is used as is (quirks R1, R2): earth beyond x 319 or y 199, row wrap near the right edge, 0 outside the buffer | Undefined behaviour in C, given a defined result | `probe_earth` (`crates/digger-core/src/sim/fireball.rs`) over `Framebuffer::probe` (`video/framebuffer.rs`) | `sim::fireball::tests::probe_beyond_right_or_bottom_is_earth`, `video::tests::probe_row_wrap`, `video::tests::probe_outside_buffer_reads_zero` |
| U03 | Two alternating players with two diggers index past the digger array | research 01 §1.1, §1.2, §9 | Unrepresentable as a game mode; rejected when settings arrive as separate numbers | Undefined behaviour in C, rejected at configuration time | `GameMode` and `GameMode::from_parts` (`crates/digger-core/src/sim/config.rs`) | `sim::config::tests::two_players_with_two_diggers_rejected`, `sim::config::tests::two_players_with_gauntlet_rejected` |
| U04 | A start level of 0 or below gives blank levels and a division by zero in the generator | research 01 §1.1, §9 | The start level accepts 1..=1000 only; the generator's modulus is non-zero by construction | Undefined behaviour in C, rejected at configuration time | `StartLevel::new` (`crates/digger-core/src/sim/config.rs`), `Modulus` (`crates/digger-core/src/sim/rng.rs`) | `sim::config::tests::start_level_0_and_1001_rejected`, `sim::rng::tests::modulus_rejects_zero` |
| U05 | A dig step indexes outside the field | research 01 §5.3, §9 | No cell changes; the penalty is still charged. No debug assertion fires: the playfield spec's scenario "Dig request beyond the right edge" requires the defined result in debug test builds, so design D13 was corrected | Undefined behaviour in C, given a defined result | `Field::dig` (`crates/digger-core/src/sim/field.rs`) | `sim::field::tests::dig_beyond_grid_changes_nothing` |
| U06 | Signed overflow in the generator and in SplitMix64 | research 01 §7.1, §7.2, §9 | Wrapping arithmetic, as every recording assumes | Undefined behaviour in C, given a defined result | `GameRng::draw` and `round_seed` (`crates/digger-core/src/sim/rng.rs`) | `sim::rng::tests::rng_sequence_from_zero`, `sim::rng::tests::round_seeds_base_0`, `sim::rng::tests::round_seed_index_wraps_in_32_bits` |
| U07 | Reads of unoccupied monster slots, removed bags and out-of-play diggers | research 01 §8, research 02 §9 | Arrays are always fully initialised and keep their last values | Undefined behaviour in C, given a defined result | The fixed arrays of `Sim` (`crates/digger-core/src/sim/mod.rs`): `Monsters` (`sim/monster.rs`), the bags (`sim/bags.rs`) and the digger records (`sim/digger.rs`), created in full by `Sim::new` and never shrunk; `StateView::monster`, `StateView::bag` and `StateView::digger` read any of them (`sim/state.rs`) | `removed_bag_keeps_its_last_values` (`crates/digger-core/tests/gold_bags.rs`), `soak_one_player`, `soak_two_players`, `soak_two_diggers`, `soak_gauntlet`, `soak_gauntlet_two_diggers` (`crates/digger-core/tests/soak.rs`, bounds checks on) |
| U08 | The on-screen monster limit has no value for a level below 1 | research 02 §4.2, §7 | Unreachable: the start level is validated | Undefined behaviour in C, unreachable | `StartLevel::new` (`crates/digger-core/src/sim/config.rs`); levels only rise from there (`LevelNumber::next`, `sim/level.rs`) | `sim::config::tests::start_level_0_and_1001_rejected` |
| U09 | The second player's level is left over from the previous game | research 01 §3.1, §10 item 18 | Always reset to level 0 at game start (Q06 covers the fresh-process case recordings exercise) | Accidental behaviour, fixed | `Sim::new` (`crates/digger-core/src/sim/mod.rs`): every game value starts from its own configuration, so nothing carries over | `sim::tests::quirk_q06_second_player_position_is_level_0` |
| U10 | A custom level file is read into 16-byte rows | research 01 §5.5 | Rows are 15 bytes, as evidently intended | Accidental behaviour, fixed | `LevelSet::from_custom_bytes` (`crates/digger-core/src/sim/level.rs`) | `sim::level::tests::custom_levels_parse_bonus_step_and_rows` |
| U11 | Quitting during the player announcement skips the end-of-game bookkeeping | research 01 §3.4 | Quit always finishes the game through the same path | Accidental behaviour, fixed | `Game::step_observed` and `Ctx::finish` (`crates/digger-core/src/sim/mod.rs`): a quit on any frame passes through the round end when a round is in progress, then the observer's `game_end` | `quit_during_play_reports_300_ticks`, `quit_during_level_draw_reports_0_ticks` (`crates/digger-core/tests/game_flow.rs`) |
| U12 | Timing kludge for recordings made on or before 1998-11-25 | research 01 §7.3, §10 item 17, research 05 §4.1 | Not ported; `add-recording-and-conformance` rejects such files | Dropped feature | Nothing in this change: the simulation has no timing kludge; the recording reader of add-recording-and-conformance rejects such files | None in this change; add-recording-and-conformance tests the rejection |

## Presentation deviations (`add-simulation-core`)

Class: presentation. These change when or how long something is shown, never
the state of the game.

- **Level-complete pause.** A fixed 40 frames instead of "until the jingle
  ends": the simulation never waits on sound (porting plan §4, Sound). 40
  frames at the default 80 ms cover the 3.17 s jingle.
- **Dirge.** No real-time hold beyond its 89 ticks: the in-game wait of the
  grave sequence already covers the 7.05 s tune.
- **Level draw pacing.** The level draw reveals three items per frame instead
  of pacing each item by a third of a frame, so that a draw takes whole
  frames; no tick can observe the difference.
- **Pause requests.** A pause request is honoured only on a main-phase tick,
  because the pause phase is simulation state entered between ticks. As in
  the original, a pause requested on the tick that ends the main phase is
  still honoured, and the wind-down follows the resume.
- **Player announcement.** The diggers' sprites are placed when the
  announcement ends rather than before it, so they are not shown during
  its 45 frames; no tick runs in between, so no state can tell.
- **Bag cue stops.** The original posts the fall-stop and wobble-stop
  commands on every tick on which no bag needs the cue; the port sends a
  stop only while the cue runs. Stopping a silent cue does nothing, so the
  sound heard is the same, and a quiet field fills no sound queue.
- **Game over notice.** `GAME OVER` is always shown for 50 frames; entering
  initials is left to the desktop app, which acts on the player result the
  step reports, so the simulation never blocks on input.

## Recording and playback quirks (`add-recording-and-conformance`)

Each entry reproduces a rule of upstream's recording formats, playback or
state hash, so that the port's recordings and checkpoints agree with
upstream's corpus. The ids follow the design of `add-recording-and-conformance`
(P1 to P16; P17 and later are findings of its divergence triage, task
group 12).

| Id | Behaviour | Class | Implemented in | Test |
|---|---|---|---|---|
| P1 | During DRF playback one record is taken per control read, not per tick: a digger reads its controls only on a tick on which it is alive, the diggers are visible and it is not in the pause after pushing a bag, so dying, invisible and paused ticks take none; two simultaneous diggers draw from the one stream, digger 1 before digger 2 within a tick (research 05 §1.6, §4.3; research 02 §3.3) | Affects the simulation | the `Observer::movement` call site in `Ctx::move_digger` (`crates/digger-core/src/sim/digger.rs`, quirk Q15); the DRF arm of `PlaybackSource::movement` reading the `DrfCursor` (`crates/digger-core/src/replay/playback.rs`) | `drf_record_consumed_only_on_control_read`, `drf_two_diggers_interleave` |
| P2 | A DRF control read that finds `E`/`e` (of `EOL` or `EOG`) or the end of the data ends the game as a quit: that read gives no direction and no fire, the tick completes and is counted, and the game is quit at the start of the next frame (research 05 §4.2, §11 item 12) | Affects the simulation | the end flag set in `PlaybackSource::movement`, turned into `FrameInput::quit` by `Replay::step` (`crates/digger-core/src/replay/`) | `drf_end_marker_counts_the_tick`, `drf_end_of_data_is_like_eol` |
| P3 | During DRF playback the raw controls of every slot are all off, so on a tick without a control read the fire input for the next tick is "not pressed"; a record's fire takes effect on the tick after its control read, as live fire does, and the caller's live controls have no effect (research 05 §1.7, §4.6, §11 items 3–4) | Affects the simulation | the DRF arm of `PlaybackSource::controls` (`crates/digger-core/src/replay/playback.rs`) | `drf_fire_does_not_linger_over_pause` |
| P4 | eDRF playback injects each slot's recorded raw control bits before direction resolution, so last-pressed-wins resolution, the one-tick tap pulse and the one-tick fire delay run as in live play; the resolver's state is reset each round as live (research 05 §1.6, §5.1, §5.3) | Affects the simulation | the eDRF arm of `PlaybackSource::controls` (`crates/digger-core/src/replay/playback.rs`), called before `SlotInput::read` in `Ctx::update_diggers` | `edrf_controls_pass_through_resolver` |
| P5 | A slot whose digger does not read its controls on a tick (dead, paused, not in play, or slot 1 of a one-digger game) still uses up one recorded eDRF value on that tick (research 05 §5.3, §11 item 6) | Affects the simulation | `Observer::controls`, called for every slot on every tick by `Ctx::update_diggers` (`crates/digger-core/src/sim/digger.rs`); the eDRF arm of `PlaybackSource::controls` | `edrf_unread_slot_consumes_value` |
| P6 | The two-digger start-cell patch (`H` at row 9, columns 6 and 8 of plans 3 and 4, quirk Q07) applies in live play and in DRF playback, whose header maps form a non-verbatim level set; eDRF maps are used exactly as recorded, as they already hold the patch (research 05 §4.6 item 6, §11 item 8; research 01 §5.5) | Affects the simulation | `playback_config` and the eDRF arm of `PlaybackSource::level_plan` (`crates/digger-core/src/replay/playback.rs`); a layout returned from `Observer::level_plan` is used verbatim | `two_digger_patch_drf_only` |
| P7 | A DRF map row shorter than 15 characters is padded with earth, and a byte that is not one of the map letters `S V H B C` reads as earth (upstream stores a NUL, which its level code treats as earth) (research 05 §4.1) | Format | the map reader of `DrfRecording::parse` (`crates/digger-core/src/recording/drf.rs`) | `recording::drf::tests::drf_short_map_row_is_earth` |
| P8 | The DRF body ignores every byte below 0x20, line ends included, also inside seeds and repeat counts (research 05 §4.1) | Format | the body filter of `DrfCursor` (`crates/digger-core/src/recording/drf.rs`) | `recording::drf::tests::drf_body_ignores_control_bytes` |
| P9 | The `U` (unlimited lives) option of a mode line is accepted and ignored: a playback never has unlimited lives, and takes its mode, start level, gauntlet time and bonus interval from the recording alone (research 05 §4.1, §4.6; research 01 §2.1) | Affects the simulation | `parse_mode_line` (`crates/digger-core/src/recording/header.rs`); `playback_config` (`crates/digger-core/src/replay/playback.rs`) | `replay::playback::tests::mode_u_ignored`, `replay::playback::tests::config_from_header_modes` |
| P10 | The state hash covers both digger slots, all six monster slots and all seven bag slots whether or not they are in use; it includes the per-digger `firepressed` and `levdone` flags, which the original keeps but never sets (always 0); and a monster slot's body values (sprite position and facing, alive, nobbin) only while the slot holds a monster body, from its spawn until the next round's monster reset, so a killed monster keeps them (research 01 §8, research 02 §2, §4.2, §4.7) | Format | the field table `state_hash::layout` (`crates/digger-core/src/state_hash/layout.rs`); the constant flags and the body condition in `StateHashFields::from_view` (`crates/digger-core/src/state_hash/view.rs`) | `state_hash::layout::tests::layout_golden`, `state_hash::tests::monster_block_is_variable_length`, `state_hash::tests::hash_stream_length_fresh_game` |
| P11 | The round-end hash (`E` records) is taken after the round's clean-up, with fireballs and the bonus removed, the bags tidied, the field saved and the monster sprites erased (the slots keep their "in use" flags, as upstream's `erasemonsters` only erases sprites), and appends the tick count (research 01 §8, research 05 §4.6) | Format | the `Observer::round_end` call site in `Ctx::end_round` (`crates/digger-core/src/sim/mod.rs`); `StateHashFields::hash_at_round_end` (`crates/digger-core/src/state_hash/mod.rs`) | `state_hash::tests::round_end_hash_after_cleanup`, `state_hash::tests::round_end_appends_tick` |
| P12 | A tick checkpoint (`C`) is taken when the tick count, after being incremented, is a multiple of 256; the controls of the chunk ending there are written first; a round that ends on such a tick writes its `C` and then its round-end `E` (research 05 §5.2, §5.3) | Format | `EdrfRecorder::tick_end` and `EdrfRecorder::round_end` (`crates/digger-core/src/replay/recorder.rs`); `Verifier::tick_end` (`crates/digger-core/src/replay/verifier.rs`) | `checkpoint_then_round_end_order` |
| P13 | On the tick the gauntlet runs out the clock wraps below zero, and the hash writes the wrapped unsigned 32-bit value followed by the time-out flag (research 01 §7.4, §8) | Affects the simulation | `GauntletClock::count_down` (`crates/digger-core/src/sim/gauntlet.rs`, quirk Q13), read through `StateView::gauntlet_clock` into `GauntletHashFields` (`crates/digger-core/src/state_hash/view.rs`) | `state_hash::tests::gauntlet_clock_wrap_hashed`, `state_hash::tests::gauntlet_appends_clock_and_timeout` |
| P14 | eDRF `I` records hold at most 32 ticks and are written when the tick count is a multiple of 32 and before any other record. A record has no slot list when every slot agrees on every tick, a slot not read on a tick agreeing with anything (the one-digger form: slot 1 is never read); otherwise it lists each slot that holds a control on some tick and gives one run list per listed slot, in which a run ends wherever the sample changes, so an unread tick and a read 0 make two runs although both print `@`. A slot counts as read on every tick on which a digger plays from it, dead or paused too, as upstream's `readdirect` runs for every digger in play; during DRF playback a read slot's value is the direction and fire of its control read's record, 0 on a tick without one (research 05 §5.2, §5.3; `edrf.c` `flushin`, `allsame`, `edrf_input`, `edrf_movement`) | Format | `EdrfRecorder::flush_controls`, `EdrfRecorder::tick_end` and `EdrfRecorder::for_drf_playback` (`crates/digger-core/src/replay/recorder.rs`) | `edrf_chunk_forms`, `conversion_bits_from_drf_records` |
| P15 | The DRF writer caps runs at 999 (a longer run continues as a new run of the same letter), counts 1 plus the digits of the count per run into a column counter and writes a line break whenever the counter reaches 60, writes `EOL` on a line of its own, and writes each round seed as eight upper-case hex digits (research 05 §4.4; `record.c` `putrun`, `recputdir`, `recputeol`) | Format | `DrfRecorder::put_record`, `DrfRecorder::put_run` and `DrfRecorder`'s `round_seed` and `round_end` hooks (`crates/digger-core/src/replay/recorder.rs`) | `drf_writer_run_cap_and_wrap` |
| P16 | Level records (`L`) are written only for participating players: a one-player or two-digger game writes one at game start, an alternating game two (the second player's reusing the first's map), and the placeholder level of player two outside alternating games writes none; maps are written as played, with the two-digger start cells opened unless a playback supplied the map, and a map already written is referred to by its number (research 05 §5.3; `main.c` `initlevel`, `edrf.c` `edrf_level`) | Format | the `Observer::level_plan` call sites (`Ctx::start_game`, `Ctx::set_up_level_observed`, quirk Q06); `EdrfRecorder::note_level` (`crates/digger-core/src/replay/recorder.rs`) | `one_player_game_has_single_level_record`, `edrf_map_reuse_written` |
| P17 | A quit during the main phase or its pause skips the wind-down ticks but not the wind-down's start: the diggers' sprites are erased, the diggers hidden and the music stopped before the round-end clean-up, so the round-end `E` checkpoint written for the quit hashes the diggers as invisible. Research 01 §3.2 shows `erasediggers()` after the main loop but research and the game-flow spec were silent on the quit path; found on `2riptest01.edrf` (E at tick 1024) and `2test03.edrf` (E at tick 2391), confirmed in upstream `main.c` `game()` | Affects the simulation | `Game::step_observed` and `Ctx::hide_diggers` (`crates/digger-core/src/sim/mod.rs`) | `quirk_p17_quit_hides_diggers_before_round_end`, `quit_during_play_reports_300_ticks` |

## Playback deviations (`add-recording-and-conformance`)

Class: presentation or usability bug. These change how a playback is
judged or reported, never the game played.

- **Verification stops at the first failure.** Upstream plays on with the
  recorded values after the first mismatch and only skips further checks;
  the port quits the game on the frame after it unless
  `ReplayOptions::stop_at_first_failure` is off, so that a long recording
  that fails early is reported at once (replay-verification spec, "Headless
  verify run"). Test: `outcome_failure_takes_precedence`.
- **Recordings have no size limit and use LF line ends.** Upstream's DRF
  writer kept a 256 KB buffer that silently started over when full, and
  wrote files in text mode (CR LF on Windows). The port's recorders grow
  without limit and write LF on every platform; the readers accept both
  line ends. Tests: `drf_writer_exceeds_256k`, `drf_writer_mode_lines`.
- **The quit records follow every quit game.** Upstream writes `EOG`
  (DRF) and `Q` (eDRF) only when a quit ends a round in progress; a game
  quit between rounds (during the `PLAYER n` announcement) ends without
  them, or, in the announcement, without its `Z` record either. The port
  writes `EOG`, and `Q` before `Z`, whenever the game was quit, as the
  game-recording spec states. Playback is unaffected: the end of a DRF's
  data or a run-out eDRF quits the game either way. Test:
  `drf_writer_quit_appends_eog`.
- **A DRF game that ends with rounds left is stopped.** Upstream exits with
  status 3 only for a playback ended by the user (`escape`) before the
  recording ran out; a DRF game that ends by itself while another round's
  seed follows exits 0 there. The port applies the spec's rule to both
  formats: a game that ends while the recording still holds unused
  controls or rounds is stopped (status 3), so a wrong game over cannot
  pass as completed. The trailer after the last `EOL` (`EOG`, initials)
  holds no round. Test: `outcome_stopped_when_game_ends_early`.

## Sound (`add-sound-engine`)

None of these affects the simulation: the sound engine never feeds back
into the game. They are listed because this file is the project's single
register (design D14 of `add-sound-engine`). Test names are those of the
`digger-sound` crate, with the module path for library tests; run one with
`cargo test -p digger-sound <name>`.

Kept (the audible shape of the original):

| Id | Quirk | Class | Isolated in | Test |
|---|---|---|---|---|
| SQ1 | A divisor of 16384 (0x4000) or more is silent, so the death effect is heard only on ticks 4 to 22 of its 51 while it displaces music on all 51 (research 04 §1.2, §2) | Presentation, kept | `Voice::tone` (`crates/digger-sound/src/voice.rs`) | `death_is_audible_on_ticks_4_to_22` (`tests/effects_trace.rs`) |
| SQ2 | A tone's frequency is `1193181 / divisor` with integer division, so every frequency is a whole number of Hz (research 04 §1.2); the square-wave generator plays exactly this whole-Hz frequency, so a tone's wave repeats exactly once per second | Presentation, kept | `Divisor::hertz` (`crates/digger-sound/src/units.rs`), used by `Voice::tone` and `SquareOsc` (`crates/digger-sound/src/osc.rs`) | `units::tests::divisor_division_truncates`, `osc::tests::one_second_of_523_hz_has_1046_transitions` |
| SQ3 | Effect priority is the original's updater order (the last effect to offer a divisor on a tick is heard), and an effect that offers nothing on a tick lets a lower one through; with both fireballs sounding the one heard alternates by a round-robin turn that is kept across ticks and stop-all (research 04 §1.3, §2) | Presentation, kept | `Effects::tick` and `Turn` (`crates/digger-sound/src/effects/mod.rs`) | `effect_priority_pairs`, `two_fireballs_alternate`, `two_explosions_alternate` (`tests/effects_trace.rs`) |
| SQ4 | A fire-start for a flying fireball resets its base divisor to 500 but not its two-tick cadence; fire-stop and explode reset both (research 04 §2) | Presentation, kept | `FireSlot` (`crates/digger-sound/src/effects/fire.rs`) | `fire_restart_keeps_cadence` (`tests/effects_trace.rs`) |
| SQ5 | Gold and death still sound on the tick on which they end, so they last 31 and 51 ticks (research 04 §2) | Presentation, kept | `Step { offer, done }` (`crates/digger-sound/src/effects/mod.rs`), `Gold` and `Death` | `gold_lasts_31_ticks`, `death_lasts_51_ticks` (`tests/effects_trace.rs`) |
| SQ6 | The music level is carried from note to note and from tune to tune: it is 8 when the engine is created, neither a new tune, stop-music nor stop-all resets it, and the level-done jingle leaves it at 50 (research 04 §1.3, §3.1, §3.3) | Presentation, kept | `MusicPlayer::level` (`crates/digger-sound/src/music/player.rs`); the jingle branch of `Synth::advance` | `level_carries_across_tunes`, `jingle_freezes_effects_and_music` (`tests/music_trace.rs`) |
| SQ7 | The normal tune releases every note after a fixed 2 units (a staccato), the other tunes one unit before the note's end; a note occupies its duration times the ticks per unit plus one loading tick (research 04 §3.3) | Presentation, kept | `GateRule` and `TuneParams::note_timing` (`crates/digger-sound/src/music/envelope.rs`) | `music::envelope::tests::normal_tune_two_unit_note_levels`, `note_lengths` (`tests/music_trace.rs`) |
| SQ8 | The quick tempo is 0.7 of the standard ticks per unit, truncated: 4, 2, 7 and 1 for the normal, bonus, dirge and battle tunes instead of 6, 3, 10 and 2; it is kept as an integer table, not computed in floating point (research 04 §3.2) | Presentation, kept | `TuneParams::ticks_per_unit` (`crates/digger-sound/src/music/envelope.rs`) | `music::envelope::tests::tempo_table`, `quick_dirge_finishes_on_tick_363` (`tests/music_trace.rs`) |
| SQ9 | The fireball noise comes from a sound-only 32-bit LCG (`state * 0x15A4E35 + 1`, wrapping), seeded 0 when the engine is created, drawn only on ticks on which a flight sounds and never reseeded by stop-all; it never touches the game's random numbers (research 04 §2) | Presentation, kept | `NoiseRng` (`crates/digger-sound/src/noise.rs`) | `noise::tests::noise_first_values`, `effects::tests::stop_all_empties_every_slot_but_keeps_the_noise` |
| SQ10 | Switching sound on also switches music on, whatever the music switch said before; switching sound off leaves the music switch alone (research 04 §1.3, §3.4) | Presentation, kept | the `ToggleSound` arm of `Synth::apply_control` (`crates/digger-sound/src/synth.rs`) | `sound_on_forces_music_on` (`tests/switches.rs`) |
| SQ11 | Music switched off freezes the tune: the player is not advanced at all, so its note, countdown and envelope level stay where they were and switching music on resumes within the same note; a play-music or stop-music command sent meanwhile is still honoured (research 04 §3.3, §3.4) | Presentation, kept | the `music_on \|\| dirge` rule in `Synth::advance` (`crates/digger-sound/src/synth.rs`) | `music_off_freezes_position`, `tune_requested_while_music_off_plays_when_enabled` (`tests/switches.rs`) |
| SQ12 | The dirge's audible part is the 12 notes before its first rest after the opening one (50 units; the first rest loads on tick 513, or 363 at the quick tempo); it then plays its 12 silent 16-unit rests (1932 ticks) and, unless stopped, starts again from the top, as the original's table loops; the finish is reported once per dirge started (research 04 §3.3, §5) | Presentation, kept | `Playing::load` (`crates/digger-sound/src/music/player.rs`) | `dirge_finishes_on_tick_513`, `dirge_event_reported_once` (`tests/music_trace.rs`) |

Also kept, without an id because the spec states it directly: a start
command for a running wobble or bonus siren keeps its position (the
original's start only raises a flag), and only the matching stop resets it.
The simulation repeats these starts while the cue lasts, so a reset on
start would never let the wobble reach its first blip. Tests:
`wobble_start_while_wobbling_keeps_position`,
`bonus_start_while_sounding_keeps_position`.

Fixed (presentation bugs, with the reason):

| Id | Upstream behaviour | Port | Isolated in | Test |
|---|---|---|---|---|
| SF1 | The level-done jingle is silent until some music has played since start-up (the mode its first oscillator plays in is only set by the music player), for example with music off from the start | The jingle always sounds: it is a voice of its own, independent of the music switch and of whether any tune has played | `jingle::note_voice` (`crates/digger-sound/src/jingle.rs`), the jingle branch of `Synth::advance` | `jingle_audible_before_any_music` (`tests/music_trace.rs`) |
| SF2 | The `SoundVolume` setting is read and never used; the only gain is a constant 1, so there is no master volume (research 04 §4.3) | A master volume of 0 to 100 % (values above 100 are treated as 100), applied last in the output stage as `sample * volume / 100` truncated toward zero | `Volume::new` (`crates/digger-sound/src/units.rs`), `synth::apply_volume` (`crates/digger-sound/src/synth.rs`) | `synth::tests::half_volume_peak`, `synth::tests::zero_volume_is_silent`, `synth::tests::volume_above_100_clamps` |
| SF3 | Starting quiet (`/Q`) opens no device and installs no sound functions, so sound cannot be switched on later, contrary to the game's own manual (research 04 §6) | The engine always exists; sound off is a mute, and the sound switch works whatever the initial state | `Synth::sound_on` and the output selection in `Synth::tick` (`crates/digger-sound/src/synth.rs`) | `started_quiet_can_be_enabled` (`tests/switches.rs`) |
| SF4 | With sound off the whole tick body is skipped: effects freeze and resume stale when sound comes back, and music requests made meanwhile are lost (research 04 §1.3) | Sound off only silences the output: effects, the jingle and the tunes advance and commands are honoured exactly as with sound on | the output selection in `Synth::tick` (`crates/digger-sound/src/synth.rs`) | `mute_does_not_freeze_state` (`tests/switches.rs`) |
| SF5 | A dirge requested with music off switches the music flag on and restores it later (at the dirge's first rest or at stop-music), a hidden state that a toggle in between could leave wrong | The dirge plays and advances whatever the music switch says, and never changes the switch: after it, a normal-tune request stays silent while music is off | the `music_on \|\| dirge` rule in `Synth::advance` (`crates/digger-sound/src/synth.rs`) | `dirge_plays_with_music_off` (`tests/music_trace.rs`) |
| SF6 | The fall effect's on/off phase survives a stop and a restart, so a restarted fall may sound up to 2 ticks (27 ms) earlier or later | A fall-start always begins the same cycle, tone on its third tick at divisor 1000 | `Fall::new` (`crates/digger-sound/src/effects/fall.rs`) | `fall_first_three_tones`, `fall_restart_begins_same_cycle` (`tests/effects_trace.rs`) |
| SF7 | The game thread blocks in two real-time loops until the audio thread acknowledges the end of the level-done jingle and of the dirge, so the game's timing depends on the sound backend (and on whether there is one) (research 04 §1.5, §5) | The simulation waits fixed frame counts of its own (`add-simulation-core`); the engine's jingle-finished and dirge-finished events are informational, go to the frontend only, and are dropped when nobody reads them; `digger-core` has no input through which sound state could enter the game | `SoundEvent` (`crates/digger-sound/src/command.rs`), the event ring of `AudioSource` (`crates/digger-sound/src/link.rs`) | `link::tests::unread_events_do_not_affect_output`; `undrained_sound_queue_changes_nothing` (`crates/digger-core/tests/determinism.rs`); `simulation_unaffected_by_sound` (`tests/`, added with the golden renders) |
| SF8 | Several sound variables (`spkrmode`, `pulsewidth`, `volume`, backend statics) are written from both the game and the audio thread without synchronisation, and the generator takes a spinlock for every sample (research 04 §1.4, §4.1) | All engine state is owned by one `Synth` on the audio thread; the game side reaches it only through a wait-free input ring of 4096 entries (a full ring drops and counts the input, the sender never blocks), an event ring of 16 entries and two atomic switch flags; nothing on the audio path allocates, locks or blocks | `sound_link`, `SoundHandle` and `AudioSource` (`crates/digger-sound/src/link.rs`) | `two_thread_smoke` (`tests/link_threads.rs`), `link::tests::overflow_drops_and_counts` |
| SF9 | The emerald step indexes the divisor table without a bounds check; the jingle's tone gate divides by the beat frequency, which is a division by zero if the two frequencies were equal | `EmeraldStep` holds only 0 to 7, so the lookup cannot go out of range; the beat is computed from two valid divisors and 0 Hz means "never gated off" | `emerald_divisor` (`crates/digger-sound/src/effects/emerald.rs`), `jingle::beat` (`crates/digger-sound/src/jingle.rs`) | `effects::tests::emerald_step_range`, `jingle::tests::zero_beat_is_ungated` |
| SF10 | The output filter (high-pass 1000 Hz, then low-pass 4000 Hz) is a compile-time choice that differs between the build systems: the GNU makefile builds raw square waves, the CMake and FreeBSD builds filter (research 04 §4.2) | A runtime switch, on by default as in the CMake build; switching it on clears the filter's state, so a toggle is deterministic | `OutputFilter` (`crates/digger-sound/src/filter.rs`), the `SetFilter` arm of `Synth::apply_control` (`crates/digger-sound/src/synth.rs`) | `synth::tests::filter_toggle`, `filter::tests::coefficients_at_44100` |

Generator tolerance (design D6). The port does not reproduce the
original's floating-point generator sample for sample: it plays each tone
with an exact integer phase accumulator. Against the original, each level
change of a square wave may fall up to one sample earlier or later, and the
phase of a tone that starts after a silent tick restarts at 0. Against the
port itself there is no tolerance: tick traces and unfiltered samples are
exact and the same on every platform. Filtered output uses `f64` and
`f64::tan`, so it may differ in the last bit between platforms and is
checked by tolerance tests only. Tests:
`osc::tests::phase_kept_across_frequency_change`, `generator_props`
(`tests/generator_props.rs`), `synth::tests::render_is_chunk_size_independent`.

The user pause and the jingle's freeze are kept apart. The original uses
one flag for both, so a resume during the jingle would unfreeze effects and
music under it. In the port a pause freezes everything, the jingle
included, and a resume continues exactly what was frozen; the jingle's
freeze of effects and music ends only with the jingle (research 04 §2).
Tests: `pause_is_separate_from_the_jingle_freeze`,
`pause_freezes_gold_at_tick_10_and_resumes_at_11` (`tests/switches.rs`).

## Command line (`add-desktop-app`)

The upstream one-letter options are accepted as aliases of the long
options (`configuration` spec, upstream option aliases); where upstream's
parsing is accidental, the port gives a defined result instead. Class:
usability bug, fixed. All are in `crates/digger/src/cli/`.

| Id | Upstream behaviour | Port | Isolated in | Test |
|---|---|---|---|---|
| CL1 | Option values are parsed without validation: `/S:n` and `/I:n` wrap in 16 bits, `/I` accepts `0x` and octal forms, `/G` clamps above 3599 (research 01 §1.1) | A value that is not a decimal number in range ends the program with status 64 and a message quoting the option as typed (`/I`, `-s`) and the value | `Cli` value parsers and `usage_error` (`cli/mod.rs`), the spelling table of `legacy::translate` (`cli/legacy.rs`) | `cli_range_errors_quote_alias` |
| CL2 | Once a speed has been given, every later bare word is parsed as a (garbage) speed, so `digger /S:40 levels.dlf` loses the level file (research 01 §1.1, `[BUG?]`) | Option order does not matter: a bare word of digits is the speed, any other bare word the level file; a bare speed together with `--speed`, or a bare level file together with `--levels`, is refused with status 64 naming both | `convert` (`cli/mod.rs`) | `cli_bare_number_is_speed`, `cli_bare_word_is_level_file` |
| CL3 | A Unix absolute path such as `/Users/ann/levels.dlf` is read as the option `/U` with junk after it | A file-valued alias (`/L /R /P /E`) needs its colon and a flag alias must stand alone, so such a path passes through as a level file name; a flag alias with a value (`/Q:x`) is refused with status 64 | `legacy::translate` (`cli/legacy.rs`) | `legacy_absolute_paths_untouched`, `legacy_flag_with_value_refused` |
| CL4 | An unreadable or short level file is dropped silently and the built-in levels are played (research 01 §1.1) | The program stops with status 66 naming the file; `<name>.dlf` is tried first when the name has no extension | `levels::load` (`cli/levels.rs`) | `level_file_missing`, `level_file_dlf_retry`, `level_file_too_short` |
| CL5 | Options are executed in order while parsing (`/P` plays at once, `/O` restarts the parse), and `/?`, `/H` and unknown options exit with status 1 (research 01 §1.1) | Options are collected first and their combinations checked (`--verify` with `--play`, `--loop` or `--take-over-at`; `--loop` or `--take-over-at` without `--play`; `--headless` without `--verify`; `--players 2` with `--two-diggers` or `--gauntlet`), each refused with status 64; help and version exit with 0, unknown options with 64 | `convert` (`cli/mod.rs`), `main` | `cli_conflicts`, `cli_help_and_version`, `unknown_option_is_a_usage_error` (`crates/digger/tests/cli_basics.rs`) |

## Input (`add-desktop-app`)

The frontend hands the core, once per frame, each control's "held now or
pressed since the previous frame" state; the core's resolver and fire
latch (quirk Q15, P4) do the rest, as upstream's `readdirect` and
`getdirect` do over its sticky key flags. The tests feed scripted press and
release timelines through the input layer into a real game and read the
direction and fire of every movement sample back through the observer.
Ids `IN`. The input layer is `crates/digger/src/input/`; tests are run with
`cargo test -p digger <name>`.

| Id | Behaviour | Class | Isolated in | Test |
|---|---|---|---|---|
| IN1 | Direction resolution: the most recently pressed held direction wins; presses within one frame rank right, then left, then down, then up; releasing the active key falls back to a held key in that order, releasing another changes nothing (research 05 §1.6) | Affects the simulation | the core's `SlotInput::read` (`crates/digger-core/src/sim/input.rs`), fed raw states by `InputState::take_frame` (`input/state.rs`) | `input_resolution_last_pressed`, `input_resolution_same_frame`, `input_resolution_release_fallback` (`input/resolution_tests.rs`) |
| IN2 | A tap shorter than a frame gives its direction for exactly one tick, also while another direction stays held (research 05 §1.6) | Affects the simulation | the pressed-since-frame flags of `InputState` and the core's one-tick pulse | `input_tap_one_tick`, `input_tap_is_seen_once` |
| IN3 | Fire is read as held or pressed since the previous frame, so a short tap fires once, and fire sampled on one tick acts on the next (research 05 §1.7) | Affects the simulation | `InputState::take_frame`; the core's fire latch (Q15) | `fire_takes_effect_next_tick` |
| IN4 | Alternating two-player games drive the digger from the player-1 controls on both players' turns; the player-2 keys do nothing there (research 05 §1.5) | Affects the simulation | the core's slot mapping; the input layer maps `p1_*` to slot 1 and `p2_*` to slot 2 in every mode | `alternating_uses_p1_controls` |
| IN5 | Upstream repeats a function action for every further key drained in the same poll, and keyboard auto-repeat repeats it while held (research 05 §1.4, §1.5); the port triggers each function action once per press, ignores auto-repeat, and lets several function keys pressed in one frame each act once | Usability bug, fixed | `InputState::key_down` and `InputState::pad_signal` (`input/state.rs`) | `action_once_per_press`, `two_actions_same_frame` |

## Not ported

Class: dropped feature. Each is rejected with a clear message where a user
could ask for it.

| Feature | Rejected by |
|---|---|
| CGA graphics mode | `add-desktop-app` (command line and configuration) |
| Networked play (NetSim) | `add-desktop-app` (command line and configuration) |
| X11 window embedding | `add-desktop-app` (command line) |
| Timing kludge for recordings made on or before 1998-11-25 (U12) | `add-recording-and-conformance` (recording reader) |

## Related tests

Beyond the per-quirk tests, `crates/digger-core/tests/sprite_model.rs`
checks long disciplined operation sequences against a composite-picture
oracle and a brute-force collision scan, and
`crates/digger-core/tests/golden_frames.rs` pins the CRC-32 of the title,
backgrounds, text, numbers and a scripted sprite scene; `cargo xtask
render-sheet` writes those scenes as PPM files for review.

The simulation's integration tests run whole games:
`crates/digger-core/tests/determinism.rs` plays one scripted game per mode
twice, once with the sound queue never drained and once headless, comparing
the picture's CRC-32 and the tick trace after every frame;
`crates/digger-core/tests/observer_roundtrip.rs` replays the round seeds and
movement samples of a live game into a second one with empty controls; and
`crates/digger-core/tests/soak.rs` runs every mode for 50,000 frames in the
debug build. `crates/digger-core/tests/quirks_doc.rs` checks that the tables
above name only tests that exist, that no quirk row lacks its code or test,
and that every `quirk_q*` test is listed in its quirk's row.

## Findings from recordings

Behaviour found by replaying the upstream corpus where the research reports
were wrong or silent (task group 12 of `add-recording-and-conformance`). The
first full run (task 7.5) had 48 of the 50 recordings passing; the two
failures had one cause.

| Recording | Tick | Finding | Quirk | Test |
|---|---|---|---|---|
| `2riptest01.edrf` | 1024 (`E` after the recorded quit) | A quit hides the diggers before the round end | P17 | `quirk_p17_quit_hides_diggers_before_round_end` |
| `2test03.edrf` | 2391 (`E` after the recorded quit) | The same | P17 | `quirk_p17_quit_hides_diggers_before_round_end` |

The monster-body lifetime rule of design D5 (the monster block holds a
body from the monster's spawn until the round's monster reset) was
confirmed, not corrected: every `E` checkpoint of the corpus matches, and
the tick-256 hash of `dror0.edrf` (`17B2FBDB`, monster block 187 bytes)
equals its first `C` record. No codec check had to be relaxed for any
recording, so the "Deliberate deviations" of the change's design stand as
written.
