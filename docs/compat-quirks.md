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
| U01 | Firing with no facing aborts the program | research 02 §3.4, §9 item 18 | The fire request is ignored and the fireball stays ready (digger spec, "Digger left without a facing") | Undefined behaviour in C, given a defined result | `Ctx::update_fire` (`crates/digger-core/src/sim/fireball.rs`) | `sim::fireball::tests::fire_with_no_facing_is_ignored` |
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

