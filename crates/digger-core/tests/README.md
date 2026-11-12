# Tests of `digger-core`

This directory holds the scenario and integration tests of the simulation
(design D11 of `add-simulation-core`). Unit tests sit next to the code in
`src/` under `#[cfg(test)]`.

| File | What it checks |
|---|---|
| `digger.rs` | digger spec: start, controls, movement, emeralds, fireballs, deaths, lives |
| `monsters.rs` | monsters spec: spawning, decisions, moves, transformations, deaths, draw pattern |
| `gold_bags.rs` | gold-bags spec and the digger's side of meeting bags |
| `bonus_mode.rs` | bonus appearance, bonus mode and eating monsters |
| `game_flow.rs` | game-flow spec: rounds, phases, outcomes, pause, quit, gauntlet, music |
| `determinism.rs` | one scripted game per mode run twice, undrained and headless (task 15.1) |
| `observer_roundtrip.rs` | seeds and movement samples recorded and replayed (task 15.2) |
| `observer_seam.rs` | the observer hook call points recording and playback rely on, and `ReplayObserver` with no components (`add-recording-and-conformance`, task group 1) |
| `soak.rs` | every mode for 50,000 frames in a debug build (task 15.3) |
| `corpus_integrity.rs` | the upstream recording corpus in `tests/recordings/` against its manifest: digests, counts 23/27/27, twins and one-line results (`add-recording-and-conformance`, task 2.3) |
| `drf_corpus.rs` | every upstream `.drf` header parses and its body tokenises with the DRF cursor alone; the "DRF corpus" table of `docs/conformance.md` is current (`add-recording-and-conformance`, task 3.5) |
| `edrf_corpus.rs` | every upstream `.edrf` parses with the eDRF reader, its `Z` tick total equals its `.out` result, and `docs/recording-formats.md` carries both spec grammars (`add-recording-and-conformance`, task 4.4) |
| `recording_writers.rs` | the DRF and eDRF recorders: header and mode lines, run cap and wrap, `EOG`, no size limit, `I` chunk forms, map reuse, `C`/`E` order, level records, a playable prefix, DRF-to-eDRF control bits (`add-recording-and-conformance`, tasks 8.1–8.3) |
| `codec_roundtrip.rs` | live games in five modes recorded as DRF and eDRF, replayed and re-recorded byte for byte, and eDRF to DRF to eDRF conversion (`add-recording-and-conformance`, task 8.4) |
| `conversion_twins.rs` | each corpus DRF converted to eDRF matches its upstream `.edrf` twin record for record and tick for tick; an informational count of byte-identical twins (`add-recording-and-conformance`, task 8.6) |
| `malformed_input.rs` | malformed-input hardening: 10000 deterministic mutations of the quick-tier files read and played with a tick limit, every 97th-byte prefix (an eDRF prefix never verifies), huge repeat counts, a 10 MB line and a million blank lines handled in under a second with short error messages (`add-recording-and-conformance`, tasks 11.1–11.2) |
| `takeover.rs` | taking a playback over: recorded controls up to the hand-over tick and live ones after it, a game recorded across the hand-over replays to the same end, no high score (`add-recording-and-conformance`, task 9.1) |
| `isolation.rs` | corpus recordings verified beside a live game under hostile caller settings (unlimited lives, start level 5, other modes, another bonus interval) give identical reports and leave the settings untouched (`add-recording-and-conformance`, task 9.2) |
| `trace.rs` | the per-tick state trace: a 256-tick window, the decoded dump, tracing is passive (`add-recording-and-conformance`, task 10.1) |
| `quirks_doc.rs` | `docs/compat-quirks.md` names only existing tests (task 15.4) |
| `golden_frames.rs`, `sprite_model.rs` | renderer and sprite layer (`add-assets-and-renderer`) |

## Spec coverage

Every `#### Scenario:` of the six capability specs of `add-simulation-core`
(`openspec/changes/add-simulation-core/specs/*/spec.md`) and the tests that
check it (task 15.5). A test named without a path is in the file given in
parentheses; `src/...` files hold unit tests in their `tests` module.


### playfield

| Requirement | Scenario | Tests |
|---|---|---|
| Grid geometry and pixel mapping | Corner cells | `corner_cells_map_to_pixels` (`src/sim/geom.rs`) |
| Grid geometry and pixel mapping | Position between cells | `pixel_60_27_is_cell_2_0_rem_8_9` (`src/sim/geom.rs`) |
| Level layout alphabet | Built-in layout 1 | `builtin_layout_1_rows_match_spec` (`src/sim/level.rs`) |
| Level layout alphabet | Unknown character | `unknown_char_is_solid_earth` (`src/sim/field.rs`) |
| Level number to layout sequence | Levels after the eighth | `layout_sequence_1_to_13` (`src/sim/level.rs`) |
| Level number to layout sequence | Level cap | `level_1000_stays_and_uses_layout_5` (`src/sim/level.rs`) |
| Difficulty index | Difficulty frozen after level 10 | `difficulty_is_min_level_10` (`src/sim/level.rs`) |
| Difficulty index | Low levels | `difficulty_is_min_level_10` (`src/sim/level.rs`) |
| Opened start cells for two simultaneous diggers | Two diggers on level 3 | `quirk_q07_two_digger_start_cells_open_on_layouts_3_and_4` (`src/sim/level.rs`), `q07_start_cells_hold_no_emerald` (`src/sim/field.rs`) |
| Opened start cells for two simultaneous diggers | One digger on level 3 | `quirk_q07_two_digger_start_cells_open_on_layouts_3_and_4` (`src/sim/level.rs`) |
| Initial earth and tunnel state | Horizontal tunnel cell | `initial_cells_from_layout_chars` (`src/sim/field.rs`) |
| Initial earth and tunnel state | Junction cell | `initial_cells_from_layout_chars` (`src/sim/field.rs`) |
| Digging removes one slice per step | First step right from a cell corner | `dig_right_from_corner_removes_slice_0_of_next_cell` (`src/sim/field.rs`) |
| Digging removes one slice per step | First step left from a cell corner | `dig_left_from_corner_removes_slice_4_of_previous_cell` (`src/sim/field.rs`) |
| Digging removes one slice per step | Step up inside a cell | `dig_up_inside_cell` (`src/sim/field.rs`) |
| Untouched mark is cleared when a cell is fully crossed | Crossing a cell horizontally | `five_steps_clear_untouched_mark` (`src/sim/field.rs`) |
| Untouched mark is cleared when a cell is fully crossed | Partly dug cell | `partly_dug_cell_stays_untouched` (`src/sim/field.rs`) |
| Dig steps never leave the grid | Dig request beyond the right edge | `dig_beyond_grid_changes_nothing` (`src/sim/field.rs`) |
| Redrawing a level fully opens partly dug cells | Turn passes back in a two-player game | `returning_player_gets_saved_field_normalised` (`tests/game_flow.rs`), `quirk_q05_redraw_opens_partly_dug_cells` (`src/sim/field.rs`) |
| Redrawing a level fully opens partly dug cells | Death without a redraw | `round_after_a_death_keeps_field_emeralds_and_bags` (`tests/game_flow.rs`) |
| Per-player field memory | Players do not share tunnels | `players_have_separate_fields` (`src/sim/field.rs`), `alternating_players_keep_separate_progress` (`tests/game_flow.rs`) |
| Per-player field memory | Field restored for the returning player | `returning_player_gets_saved_field_normalised` (`tests/game_flow.rs`) |
| Emerald placement and counting | Emerald count of level 1 | `level_1_has_30_emeralds` (`src/sim/field.rs`) |
| Emerald placement and counting | Emerald under a falling bag | `falling_bag_removes_emerald_without_score` (`tests/gold_bags.rs`) |
| Background drawing | Background follows the layout number | `background_uses_layout_tile` (`src/sim/draw.rs`) |
| Background drawing | Tile coverage | `background_covers_rows_14_to_199` (`src/sim/draw.rs`) |
| Tunnel drawing on a full draw | Horizontal corridor | `horizontal_corridor_is_continuous_black` (`src/sim/draw.rs`) |
| Tunnel drawing on a full draw | Solid neighbours | `no_opening_into_solid_neighbour` (`src/sim/draw.rs`) |
| Tunnel carving shapes during play | Digger moving right | `right_cap_at_x_plus_16_y_minus_1` (`src/sim/draw.rs`) |
| Tunnel carving shapes during play | Carving at the bottom row | `shapes_clip_at_screen_bottom` (`src/sim/draw.rs`) |
| Emerald drawing | Emerald position | `emerald_3_1_occupies_x72_y39` (`src/sim/draw.rs`) |
| Progressive level draw | Draw takes frames but no ticks | `level_1_draw_takes_21_frames` (`src/sim/draw.rs`) |
| Lives display | Three lives | `three_lives_show_two_icons` (`src/sim/status.rs`) |
| Lives display | Many spare lives | `seven_lives_show_x6` (`src/sim/status.rs`) |
| Gauntlet clock display | Two-minute gauntlet start | `gauntlet_120s_shows_2_00` (`src/sim/status.rs`) |
| Custom level data | Valid custom levels | `custom_levels_parse_bonus_step_and_rows` (`src/sim/level.rs`) |
| Custom level data | Truncated data | `custom_levels_reject_short_data` (`src/sim/level.rs`) |
| Custom level data | Rows are fifteen characters wide | `custom_levels_parse_bonus_step_and_rows` (`src/sim/level.rs`), `parse_rows_rejects_wrong_row_length` (`src/sim/level.rs`) |

### digger

| Requirement | Scenario | Tests |
|---|---|---|
| Start position and facing | One-player start | `one_player_starts_at_152_180_facing_right` (`tests/digger.rs`) |
| Start position and facing | Two simultaneous diggers | `two_diggers_start_at_172_and_132` (`tests/digger.rs`) |
| Start position and facing | Second alternating player | `second_alternating_player_faces_left` (`src/sim/digger.rs`) |
| Direction resolution from held controls | Last pressed wins | `last_pressed_wins` (`src/sim/input.rs`) |
| Direction resolution from held controls | Fallback after release | `fallback_after_release_prefers_down_over_up` (`src/sim/input.rs`) |
| Direction resolution from held controls | Short tap | `short_tap_lasts_one_tick` (`src/sim/input.rs`) |
| Direction resolution from held controls | Releasing a non-winning control | `releasing_non_winner_changes_nothing` (`src/sim/input.rs`) |
| Controls are reset at round start | Held key across a death | `held_key_counts_as_new_press_after_reset` (`src/sim/input.rs`) |
| Movement samples are taken only when a digger can move | Push pause consumes no sample | `quirk_q15_no_movement_sample_during_push_pause` (`src/sim/digger.rs`) |
| Movement samples are taken only when a digger can move | Dying digger | `dying_digger_takes_no_movement_sample` (`tests/digger.rs`) |
| Turning rules | Turn requested between cells | `turn_requested_between_cells_waits_for_alignment` (`tests/digger.rs`) |
| Turning rules | No input | `no_input_stops_between_cells` (`tests/digger.rs`) |
| Turning rules | Immediate reversal | `reversal_is_immediate` (`tests/digger.rs`) |
| Step size and playfield limits | Crossing one cell downward | `crossing_a_cell_takes_5_or_6_ticks` (`tests/digger.rs`) |
| Step size and playfield limits | At the left limit | `stops_at_left_limit_keeping_facing` (`tests/digger.rs`) |
| Digging while moving | Tunnelling through solid earth | `tunnelling_clears_horizontal_slices` (`tests/digger.rs`) |
| Eating emeralds | Eating while moving right | `quirk_q16_emerald_eaten_at_x_64_moving_right` (`tests/digger.rs`) |
| Eating emeralds | Eating while moving up | `emerald_eaten_moving_up_at_remainder_9` (`tests/digger.rs`) |
| Eight emeralds in a row | Unbroken run | `eight_emerald_run_scores_450_with_notes_0_to_7` (`tests/digger.rs`) |
| Eight emeralds in a row | Run broken by a pause | `run_restarts_after_9_idle_ticks` (`tests/digger.rs`) |
| Meeting a bag while moving | Successful push | `free_push_moves_bag_4_px_and_slides` (`tests/gold_bags.rs`) |
| Meeting a bag while moving | Bag above blocks upward movement | `quirk_q18_blocked_digger_faces_back` (`tests/gold_bags.rs`) |
| Meeting a bag while moving | Blocked horizontal push | `quirk_q17_push_pause_after_failed_push` (`tests/gold_bags.rs`), `chain_against_wall_fails_and_reverses_first_bag` (`tests/gold_bags.rs`) |
| Digger left without a facing | Fire with no facing | `fire_with_no_facing_is_ignored` (`src/sim/fireball.rs`), `stationary_overlap_leaves_digger_without_facing` (`tests/gold_bags.rs`) |
| Digger left without a facing | Facing recovered | `facing_recovered_on_a_cell_corner` (`tests/gold_bags.rs`) |
| Digger left without a facing | Still overlapping after the move | `facing_none_digger_under_a_bag_stays_blocked` (`tests/gold_bags.rs`) |
| Fire control is latched one tick late | Fire pressed on one tick | `fire_on_tick_21_after_press_on_tick_20` (`tests/digger.rs`), `fire_read_on_tick_n_is_active_on_tick_n_plus_1` (`src/sim/input.rs`) |
| Launching a fireball | Firing to the right | `fire_right_spawns_at_160_184` (`tests/digger.rs`) |
| Launching a fireball | Fire held while not ready | `no_second_fireball_while_one_exists` (`tests/digger.rs`) |
| Recharge time by level | Recharge table | `recharge_table_63_to_90` (`src/sim/fireball.rs`) |
| Recharge time by level | Earliest second shot on level 1 | `level_1_next_shot_64_ticks_after_removal` (`tests/digger.rs`) |
| Fireball flight | Flight along an open tunnel | `fireball_advances_8_px_per_tick` (`tests/digger.rs`) |
| Earth probe for fireballs | Background counts as earth | `quirk_q19_shot_into_earth_explodes_at_previous_position` (`tests/digger.rs`), `probe_masks_per_direction` (`src/sim/fireball.rs`) |
| Earth probe for fireballs | Probe outside the screen | `probe_beyond_right_or_bottom_is_earth` (`src/sim/fireball.rs`) |
| Earth stops a fireball only when it touches no sprite | Shot reaches earth | `quirk_q19_shot_into_earth_explodes_at_previous_position` (`tests/digger.rs`) |
| Earth stops a fireball only when it touches no sprite | Probe ignored while touching a sprite | `quirk_q19_probe_ignored_while_touching_a_sprite` (`tests/digger.rs`), `probe_ignored_while_touching_a_tombstone` (`src/sim/fireball.rs`) |
| What a fireball hits | Shooting a monster | `fireball_kills_nobbin_for_250` (`tests/digger.rs`) |
| What a fireball hits | Shooting a bag | `fireball_explodes_on_bag_without_harming_it` (`tests/digger.rs`) |
| What a fireball hits | Leaving the playfield | `fireball_up_explodes_at_y_11` (`tests/digger.rs`) |
| Fireball explosion | Explosion length | `explosion_lasts_three_frames_then_removed` (`tests/digger.rs`) |
| Exploding fireball as an obstacle | Exploding fireball hit again | `exploding_fireball_hit_again_is_not_restarted` (`src/sim/fireball.rs`), `two_fireballs_explode_each_other` (`tests/digger.rs`) |
| Who can be killed | Digger walks into a monster | `quirk_q26_digger_walking_into_monster_dies_on_monsters_move` (`tests/monsters.rs`) |
| Who can be killed | Crushed digger hit by a monster | `quirk_q20_carried_digger_can_be_rekilled` (`src/sim/digger.rs`) |
| Death by monster or fireball | Death arc | `death_arc_heights_177_175_174_174_175_177_180` (`tests/digger.rs`) |
| Death by falling bag | Carried to the floor | `quirk_q21_crushed_digger_carried_at_bag_plus_6` (`tests/gold_bags.rs`), `carried_digger_goes_to_grave_once_bag_rests` (`src/sim/digger.rs`) |
| Grave sequence | One-digger timing | `quirk_q23_one_digger_round_ends_at_g_plus_89` (`tests/digger.rs`) |
| Grave sequence | Monster on the grave | `quirk_q22_grave_removal_scores_nothing` (`src/sim/digger.rs`) |
| Lives | One-player death | `one_player_death_leaves_2_lives` (`tests/digger.rs`) |
| Lives | Gauntlet death | `gauntlet_death_keeps_lives` (`tests/digger.rs`) |
| Respawn with invincibility in two-digger games | Respawn | `respawn_at_132_180_with_2_lives_and_blinking` (`tests/digger.rs`) |
| Respawn with invincibility in two-digger games | Last life lost | `last_life_lost_removes_tombstone_next_tick` (`tests/digger.rs`) |
| Invincibility | Monster touches an invincible digger | `invincible_digger_not_killed_by_monster` (`tests/monsters.rs`) |
| Invincibility | Invincibility runs out faster after a blocked push | `stationary_overlap_leaves_digger_without_facing` (`tests/gold_bags.rs`) |
| Invincibility | Normal duration | `invincible_for_50_ticks_when_unblocked` (`tests/digger.rs`) |
| Bonus appearance | Cherry after the last monster | `cherry_appears_44_ticks_after_last_level_1_spawn` (`tests/bonus_mode.rs`) |
| Entering bonus mode | Duration table | `bonus_duration_table_230_to_50` (`src/sim/bonus.rs`) |
| Entering bonus mode | Bonus mode length on level 1 | `bonus_mode_lasts_ticks_p_to_p_plus_229_on_level_1` (`tests/bonus_mode.rs`) |
| Bonus mode signalling | Start of bonus mode | `pulse_stops_and_bonus_music_starts_on_tick_p_plus_19` (`tests/bonus_mode.rs`) |
| Eating monsters by moving into them | Two monsters eaten in one move | `two_monsters_eaten_in_one_move_score_200_then_400` (`tests/bonus_mode.rs`) |
| Two-digger interaction rules | Friendly fire | `fireball_kills_other_digger` (`tests/digger.rs`) |
| Two-digger interaction rules | One digger down | `respawn_at_132_180_with_2_lives_and_blinking` (`tests/digger.rs`), `quirk_q24_all_dead_uses_absolute_indices` (`src/sim/digger.rs`) |

### monsters

| Requirement | Scenario | Tests |
|---|---|---|
| Monster slots and spawn state | New monster stands still | `new_monster_stands_still_for_five_moves` (`tests/monsters.rs`) |
| Level parameters | Parameter table | `monster_parameter_table` (`src/sim/monster.rs`) |
| Level parameters | Schedule restarts after a death | `schedule_restarts_after_a_death` (`tests/monsters.rs`) |
| Spawn schedule | First two monsters on level 1 | `level_1_monsters_spawn_on_ticks_11_and_55` (`tests/monsters.rs`) |
| Spawn schedule | On-screen limit reached | `no_spawn_at_on_screen_limit` (`tests/monsters.rs`) |
| Spawn schedule | No spawning in bonus mode | `no_spawning_in_bonus_mode` (`tests/monsters.rs`) |
| Monsters remaining | Eaten monster comes back | `eaten_monster_returns_after_bonus_mode` (`tests/bonus_mode.rs`), `quirk_q31_bonus_mode_removal_raises_total` (`src/sim/monster.rs`) |
| Movement step and move scheduling | Undelayed nobbin in a corridor | `nobbin_advances_4_px_per_tick_in_corridor` (`tests/monsters.rs`) |
| Movement step and move scheduling | Delayed monster | `delayed_monster_waits_two_ticks` (`tests/monsters.rs`) |
| Random double move for nobbins | Double move | `nobbin_double_moves_when_draw_is_zero` (`tests/monsters.rs`) |
| Random double move for nobbins | Eaten nobbin | `eaten_nobbin_makes_no_second_move` (`tests/monsters.rs`) |
| Random double move for nobbins | Hobbin still draws | `quirk_q03_hobbin_draws_but_never_double_moves` (`tests/monsters.rs`) |
| Sources of delay | Turning costs a tick | `turning_costs_a_tick` (`tests/monsters.rs`) |
| Sources of delay | Pushing a bag | `nobbin_pushes_bag_and_gains_2_delay` (`tests/monsters.rs`) |
| Chase target | Alternating targets | `alternating_targets_with_two_diggers` (`tests/monsters.rs`) |
| Chase target | Target dead | `target_dead_chases_the_other_digger` (`tests/monsters.rs`) |
| Direction priorities at cell corners | Target mostly below | `priorities_target_mostly_below` (`src/sim/monster.rs`) |
| Direction priorities at cell corners | No reversing by preference | `reverse_direction_goes_last` (`src/sim/monster.rs`) |
| Direction priorities at cell corners | Fleeing | `fleeing_reverses_priority_list` (`src/sim/monster.rs`) |
| Random element in the choice | Level 3 noise | `noise_swaps_first_and_third_below_level_6` (`src/sim/monster.rs`) |
| Random element in the choice | Level 8 draw without effect | `quirk_q03_noise_draw_taken_at_level_8_without_effect` (`src/sim/monster.rs`) |
| Nobbin passability and choice | Open corridor ahead | `nobbin_advances_4_px_per_tick_in_corridor` (`tests/monsters.rs`), `passable_through_joined_cells` (`src/sim/field.rs`) |
| Nobbin passability and choice | Dead end | `quirk_q28_dead_end_reverses` (`src/sim/monster.rs`) |
| Nobbin passability and choice | Adjacent but unconnected tunnel | `adjacent_but_unconnected_tunnel_does_not_qualify` (`tests/monsters.rs`), `adjacent_unconnected_tunnel_not_passable` (`src/sim/field.rs`) |
| Hobbin movement | Digging through earth | `hobbin_digs_down_through_earth` (`tests/monsters.rs`) |
| Hobbin movement | Hobbin eats an emerald | `quirk_q38_hobbin_destroys_emerald_without_score` (`tests/monsters.rs`) |
| Side effects persist while a monster is held in place | Monsters freeze when the digger dies | `quirk_q27_frozen_monsters_still_draw_random_numbers` (`tests/monsters.rs`) |
| Nobbin to hobbin | Thresholds | `hobbin_thresholds_by_level` (`src/sim/monster.rs`) |
| Nobbin to hobbin | Two nobbins crossing | `two_overlaps_on_level_9_make_a_hobbin` (`tests/monsters.rs`) |
| Hobbin to nobbin | Thresholds | `hobbin_reverts_at_corner_above_30_plus_2l` (`src/sim/monster.rs`) |
| Monsters meeting monsters | Catching up from behind | `follower_reverses_leader` (`tests/monsters.rs`) |
| Monsters meeting monsters | Head-on | `head_on_monsters_keep_directions` (`tests/monsters.rs`) |
| Monsters meeting bags | Nobbin pushes a bag | `nobbin_pushes_bag_and_gains_2_delay` (`tests/monsters.rs`) |
| Monsters meeting bags | Hobbin eats a bag | `hobbin_eats_bag_on_second_move` (`tests/monsters.rs`) |
| Monsters meeting bags | Nobbin under a bag | `nobbin_under_bag_turns_back` (`tests/monsters.rs`) |
| Monsters killing diggers | Catching the digger | `quirk_q26_digger_walking_into_monster_dies_on_monsters_move` (`tests/monsters.rs`) |
| Being eaten in bonus mode | Monster runs into a digger | `monster_running_into_digger_in_bonus_mode_is_eaten` (`tests/monsters.rs`) |
| Death under a falling bag | Squashed nobbin | `squashed_nobbin_scores_250_two_ticks_after_bag_stops` (`tests/monsters.rs`) |
| Death under a falling bag | Dead monster still counts | `dead_monster_still_blocks_spawning` (`tests/monsters.rs`) |
| Frightened by falling bags | Nobbin climbing under a falling bag | `nobbin_under_falling_bag_turns_down` (`tests/monsters.rs`) |
| Random-number draw pattern | One undelayed monster on a cell corner | `draw_pattern_two_draws_on_corner_one_between_cells` (`tests/monsters.rs`) |
| Random-number draw pattern | Monster between cells | `draw_pattern_two_draws_on_corner_one_between_cells` (`tests/monsters.rs`) |
| Random-number draw pattern | Nothing else draws | `no_draws_without_undelayed_living_monster` (`tests/monsters.rs`) |

### gold-bags

| Requirement | Scenario | Tests |
|---|---|---|
| Bag creation | Bags of level 1 | `level_1_has_seven_bags_first_at_1_2_second_at_4_0` (`tests/gold_bags.rs`) |
| Bag creation | Too many bags | `ninth_bag_cell_is_ignored` (`tests/gold_bags.rs`) |
| Bags persist per player across rounds | Wobbling bag at round end | `quirk_q37_tidy_removes_wobbling_broken_offgrid_and_7_9_bags` (`src/sim/bags.rs`) |
| Bags persist per player across rounds | Resting bag survives a death | `resting_bag_survives_a_death` (`tests/gold_bags.rs`) |
| When a bag starts to wobble | Tunnel dug under a bag | `bag_wobbles_when_cell_below_is_dug` (`tests/gold_bags.rs`) |
| When a bag starts to wobble | Digger climbing under a bag | `quirk_q36_no_wobble_while_digger_climbs_underneath` (`tests/gold_bags.rs`) |
| Wobble timing | Wobble then fall | `first_fall_step_16_ticks_after_wobble_starts` (`tests/gold_bags.rs`) |
| Falling | Fall speed | `bag_falls_one_cell_per_3_ticks` (`tests/gold_bags.rs`) |
| Landing | Shaft with a floor | `two_cell_drop_breaks` (`tests/gold_bags.rs`), `quirk_q34_landing_bag_removes_overlapped_bag` (`tests/gold_bags.rs`) |
| Landing | Landing on another bag | `quirk_q34_landing_bag_removes_overlapped_bag` (`tests/gold_bags.rs`) |
| Breaking after two or more cells | One-cell drop | `one_cell_drop_stays_intact` (`tests/gold_bags.rs`) |
| Breaking after two or more cells | Two-cell drop | `two_cell_drop_breaks` (`tests/gold_bags.rs`) |
| Gold opening and expiry | Opening frames | `penalty_table_for_monsters_and_bags_bag_sites` (`tests/gold_bags.rs`), `two_cell_drop_breaks` (`tests/gold_bags.rs`) |
| Gold lifetime | Lifetime table | `gold_lifetime_table_140_to_50` (`src/sim/bags.rs`) |
| Gold lifetime | Level 1 gold | `level_1_gold_disappears_139_ticks_after_landing` (`tests/gold_bags.rs`) |
| Gold lifetime | First level of a one-player game | `quirk_q40_one_player_gold_lifetime_is_150_on_the_first_level` (`tests/gold_bags.rs`) |
| Gold over a dug cell vanishes early | Digging under gold | `gold_over_dug_cell_gone_within_10_ticks` (`tests/gold_bags.rs`) |
| Collecting gold | Digger walks into gold | `digger_collects_gold_for_500` (`tests/gold_bags.rs`) |
| Collecting gold | Digger and monster together | `quirk_q35_digger_wins_gold_tie_with_monster` (`src/sim/bags.rs`) |
| Collecting gold | Gold from below | `gold_collected_from_below_does_not_turn_digger` (`tests/gold_bags.rs`) |
| Horizontal push | Free push | `free_push_moves_bag_4_px_and_slides` (`tests/gold_bags.rs`) |
| Horizontal push | Monster on the far side | `push_blocked_by_monster_on_far_side` (`tests/gold_bags.rs`) |
| Horizontal push | Wobbling bag pushed | `pushed_wobbling_bag_stops_wobbling` (`tests/gold_bags.rs`) |
| Failed push reverses the bag | Blocked between cells | `quirk_q32_failed_push_rolls_bag_back_to_corner` (`tests/gold_bags.rs`) |
| Push chains | Two bags in a row | `two_bags_pushed_together` (`tests/gold_bags.rs`) |
| Push chains | Chain against the wall | `chain_against_wall_fails_and_reverses_first_bag` (`tests/gold_bags.rs`) |
| Sliding continues to the cell corner | One push, one cell | `one_push_slides_one_cell_in_8_ticks` (`tests/gold_bags.rs`) |
| Sliding continues to the cell corner | Shared counter with two sliding bags | `quirk_q32_shared_counter_lets_second_bag_move_twice` (`tests/gold_bags.rs`) |
| End of a slide | Pushed over a shaft | `bag_pushed_over_shaft_drops_without_wobble` (`tests/gold_bags.rs`) |
| End of a slide | Pushed onto solid ground | `bag_pushed_onto_solid_ground_comes_to_rest` (`tests/gold_bags.rs`) |
| Pushing against a falling bag | Walking into a falling bag | `quirk_q33_walking_into_falling_bag_kills_without_moving_it` (`tests/gold_bags.rs`) |
| Crushing | Bag falls on a digger | `falling_bag_kills_digger_below` (`tests/gold_bags.rs`) |
| Crushing | Digger above the bag | `digger_above_falling_bag_is_safe` (`tests/gold_bags.rs`) |
| Moving-bag count for the wind-down | Wobbling bag delays the round end | `wobbling_bag_extends_wind_down` (`tests/game_flow.rs`) |
| Bag sound cues | Wobble and fall cues | `first_fall_step_16_ticks_after_wobble_starts` (`tests/gold_bags.rs`) |
| Stopping the bag cues | Bag lands | `fall_stop_emitted_when_last_falling_bag_lands` (`src/sim/bags.rs`) |

### scoring

| Requirement | Scenario | Tests |
|---|---|---|
| Score event values | Event table | `event_values_accumulate_25_525_775_1775` (`src/sim/score.rs`) |
| Score event values | Unscored removal | `quirk_q38_hobbin_destroys_emerald_without_score` (`tests/monsters.rs`) |
| Who is credited | Fireball credit with two diggers | `credited_score_is_turn_plus_digger` (`src/sim/score.rs`) |
| Bag kills with two diggers | Both diggers in the game | `bag_kill_split_both_alive` (`src/sim/score.rs`) |
| Bag kills with two diggers | Digger 2 out of lives | `bag_kill_all_to_digger_1_when_2_out` (`src/sim/score.rs`), `bag_kill_makes_two_calls_with_zero_points` (`src/sim/score.rs`) |
| Monster-eating progression | Three monsters in one bonus mode | `event_values_accumulate_25_525_775_1775` (`src/sim/score.rs`), `two_monsters_eaten_in_one_move_score_200_then_400` (`tests/bonus_mode.rs`) |
| Monster-eating progression | New bonus mode | `multiplier_resets_in_second_bonus_mode` (`tests/bonus_mode.rs`) |
| Score awards are 16-bit signed values | Ninth consecutive eat | `quirk_q12_award_51200_adds_minus_14336` (`src/sim/score.rs`) |
| Bonus life thresholds | First bonus life | `first_bonus_life_at_20000` (`src/sim/score.rs`) |
| Bonus life thresholds | One step per call | `one_threshold_step_per_call` (`src/sim/score.rs`) |
| Second score is one point late | Exactly on the threshold | `quirk_q09_second_score_needs_20001` (`src/sim/score.rs`) |
| Life cap | At the cap | `quirk_q10_cap_at_5_threshold_advances` (`src/sim/score.rs`) |
| Life cap | Unlimited lives option | `unlimited_lives_passes_cap` (`src/sim/score.rs`) |
| Bonus time in gauntlet mode | Crossing 20000 in gauntlet | `gauntlet_bonus_adds_17897715` (`tests/game_flow.rs`), `gauntlet_bonus_adds_time_under_the_same_cap` (`src/sim/score.rs`) |
| Rollover at one million | Passing one million | `quirk_q11_rollover_moves_score_to_carried_total` (`src/sim/score.rs`) |
| Score display | Small score | `score_25_draws_00025_from_x12` (`src/sim/score.rs`) |
| Score display | Six digits | `six_digit_score_from_x0` (`src/sim/score.rs`) |
| Zero-point scoring calls | Round start refresh | `round_start_draws_scores_and_lives` (`src/sim/mod.rs`), `quirk_q39_zero_point_call_awards_pending_bonus` (`tests/game_flow.rs`) |
| Penalty counter | Penalty table for diggers and fireballs | `moving_digger_charges_dig_and_draw` (`tests/digger.rs`), `quirk_q15_no_movement_sample_during_push_pause` (`src/sim/digger.rs`), `quirk_q18_blocked_digger_faces_back` (`tests/gold_bags.rs`), `fireball_advances_8_px_per_tick` (`tests/digger.rs`), `explosion_lasts_three_frames_then_removed` (`tests/digger.rs`), `carried_digger_goes_to_grave_once_bag_rests` (`src/sim/digger.rs`), `death_arc_heights_177_175_174_174_175_177_180` (`tests/digger.rs`), `tombstone_frames_at_g_g3_g6_g9_g12` (`tests/digger.rs`), `emerald_tick_penalty_is_6` (`tests/digger.rs`), `quirk_q38_hobbin_destroys_emerald_without_score` (`tests/monsters.rs`) |
| Penalty counter | Penalty table for monsters and bags | `nobbin_advances_4_px_per_tick_in_corridor` (`tests/monsters.rs`), `follower_reverses_leader` (`tests/monsters.rs`), `nobbin_under_bag_turns_back` (`tests/monsters.rs`), `penalty_table_for_monsters_and_bags_carried_monster` (`tests/monsters.rs`), `penalty_table_for_monsters_and_bags_bag_sites` (`tests/gold_bags.rs`), `quirk_q33_walking_into_falling_bag_kills_without_moving_it` (`tests/gold_bags.rs`), `quirk_q32_failed_chain_that_touches_a_monster_restores_twice` (`tests/gold_bags.rs`), `digger_collects_gold_for_500` (`tests/gold_bags.rs`), `monster_takes_gold_without_score` (`tests/monsters.rs`) |
| Penalty counter | Digger eats an emerald | `emerald_tick_penalty_is_6` (`tests/digger.rs`) |
| Penalty overflow delays monsters | Overflow table | `quirk_q02_overflow_table` (`src/sim/penalty.rs`) |
| Penalty overflow delays monsters | Slot 0 exempt | `quirk_q02_slot_0_never_delayed_by_overflow` (`tests/monsters.rs`) |
| Reported results | Summary of a finished one-player game | `one_player_game_over_after_last_life` (`tests/game_flow.rs`), `quit_during_play_reports_300_ticks` (`tests/game_flow.rs`) |

### game-flow

| Requirement | Scenario | Tests |
|---|---|---|
| Game modes | Alternating players keep separate progress | `alternating_players_keep_separate_progress` (`tests/game_flow.rs`) |
| Game modes | Simultaneous diggers share the field | `simultaneous_diggers_share_the_field` (`tests/game_flow.rs`) |
| Invalid configurations are rejected | Two players with two diggers | `two_players_with_two_diggers_rejected` (`src/sim/config.rs`) |
| Invalid configurations are rejected | Start level zero | `start_level_0_and_1001_rejected` (`src/sim/config.rs`) |
| Invalid configurations are rejected | Gauntlet duration limits | `gauntlet_duration_clamped` (`src/sim/config.rs`) |
| Game start | One-player start at level 5 | `one_player_start_at_level_5` (`tests/game_flow.rs`) |
| Second player's level state in other modes | One-player game after a two-player game | `one_player_game_after_a_two_player_game` (`tests/game_flow.rs`), `quirk_q06_second_player_position_is_level_0` (`src/sim/mod.rs`) |
| Rounds | Death then completion | `death_then_completion_numbers_rounds_0_1_2` (`tests/game_flow.rs`) |
| Round start | Round after a death | `round_after_a_death_keeps_field_emeralds_and_bags` (`tests/game_flow.rs`), `resting_bag_survives_a_death` (`tests/gold_bags.rs`) |
| Player announcement | Turn passes | `turn_passes_with_45_frame_announcement` (`tests/game_flow.rs`) |
| Player announcement | One-player game | `no_announcement_in_one_player_game` (`tests/game_flow.rs`) |
| Update order of a main-phase tick | Digger moves before monsters | `digger_moves_before_monsters` (`tests/game_flow.rs`) |
| Update order of a main-phase tick | Bags see this tick's digging | `bag_wobbles_same_tick_as_dig` (`tests/game_flow.rs`) |
| Level completion test | Last emerald | `last_emerald_ends_main_phase` (`tests/game_flow.rs`) |
| Level completion test | All monsters killed | `killing_last_monster_completes_level` (`tests/game_flow.rs`) |
| Wind-down | Quiet field | `quiet_field_winds_down_in_1_tick` (`tests/game_flow.rs`) |
| Wind-down | Busy field without moving bags | `busy_field_winds_down_in_20_ticks` (`tests/game_flow.rs`) |
| Wind-down | Monsters keep drawing random numbers | `quirk_q04_monsters_draw_random_numbers_in_wind_down` (`tests/game_flow.rs`) |
| Round end | Flying fireball at round end | `flying_fireball_gone_at_round_end` (`tests/game_flow.rs`), `flying_fireball_removed_at_round_end` (`src/sim/fireball.rs`) |
| Level completion outcome | Normal completion | `completing_level_2_draws_level_3_without_life_loss` (`tests/game_flow.rs`) |
| Level completion outcome | Dying with the last emerald gone | `quirk_q14_last_emerald_on_death_tick_advances_and_costs_a_life` (`tests/game_flow.rs`) |
| Level-complete jingle and pause | Pause length | `level_done_pause_is_40_wait_frames` (`tests/game_flow.rs`) |
| Death outcome and passing the turn | Other player out of lives | `same_player_continues_when_other_is_out` (`tests/game_flow.rs`) |
| Game over | One-player game over | `one_player_game_over_after_last_life` (`tests/game_flow.rs`) |
| Game over | Alternating game continues | `alternating_game_continues_after_player_1_is_out` (`tests/game_flow.rs`) |
| End of the game | Bonus life from the game-over calls | `quirk_q39_zero_point_call_awards_pending_bonus` (`tests/game_flow.rs`) |
| Gauntlet clock | Two-minute gauntlet | `quirk_q13_120s_gauntlet_times_out_on_tick_1790_clock_4294949016` (`tests/game_flow.rs`) |
| Gauntlet clock | Clock runs while dying | `gauntlet_clock_runs_while_dying` (`tests/game_flow.rs`) |
| Gauntlet time-out | Time-out during the main phase | `time_up_shown_for_50_frames_then_finished` (`tests/game_flow.rs`) |
| Frames and ticks | Tick count across a level change | `tick_count_across_a_level_change` (`tests/game_flow.rs`) |
| Per-frame input | Input during the level draw | `input_during_level_draw_is_ignored` (`src/sim/mod.rs`) |
| Speed setting and tick length | Default speed | `default_speed_is_80000_us` (`src/sim/timing.rs`) |
| Speed setting and tick length | Faster at the floor | `faster_stops_at_10000` (`src/sim/timing.rs`) |
| Speed setting and tick length | Same game at two speeds | `repeated_run_is_identical_in_every_mode` (`tests/determinism.rs`), `speed_zero_means_no_wait` (`src/sim/timing.rs`) |
| Determinism | Repeated run | `repeated_run_is_identical_in_every_mode` (`tests/determinism.rs`) |
| Determinism | Headless run | `headless_run_has_the_same_states` (`tests/determinism.rs`), `undrained_sound_queue_changes_nothing` (`tests/determinism.rs`) |
| Random-number generator | Sequence from state 0 | `rng_sequence_from_zero` (`src/sim/rng.rs`) |
| Per-round seeding | Seeds from base 0 | `round_seeds_base_0` (`src/sim/rng.rs`) |
| Per-round seeding | Seeds from base 1 | `round_seed_base_1` (`src/sim/rng.rs`) |
| Externally supplied seeds and samples | Re-running from observed values | `re_running_from_observed_values_one_player` (`tests/observer_roundtrip.rs`), `re_running_from_observed_values_two_players` (`tests/observer_roundtrip.rs`), `re_running_from_observed_values_two_diggers` (`tests/observer_roundtrip.rs`), `re_running_from_observed_values_gauntlet` (`tests/observer_roundtrip.rs`), `re_running_from_observed_values_gauntlet_two_diggers` (`tests/observer_roundtrip.rs`) |
| Externally supplied seeds and samples | Fire held during a push pause | `quirk_q15_fire_pressed_during_push_pause_fires_without_a_sample` (`tests/gold_bags.rs`) |
| Pause | Paused frames do not simulate | `paused_frames_change_no_state` (`tests/game_flow.rs`) |
| Pause | Pause requested during the wind-down | `pause_ignored_during_wind_down` (`tests/game_flow.rs`) |
| Resuming from pause | Resume | `resume_refreshes_scores_and_lives` (`tests/game_flow.rs`) |
| Quit | Quit during play | `quit_during_play_reports_300_ticks` (`tests/game_flow.rs`) |
| Music requests | Last life | `last_life_round_requests_last_life_tune` (`tests/game_flow.rs`), `last_life_marks_main_music` (`src/sim/mod.rs`) |
| Music requests | Bonus life changes the tune | `bonus_life_on_last_life_rerequests_main_tune` (`tests/game_flow.rs`) |
| Sound commands per frame | Emerald and gold on one tick | `commands_arrive_in_event_order` (`tests/game_flow.rs`) |
| Finished-game summary | Stepping a finished game | `stepping_a_finished_game_changes_nothing` (`tests/game_flow.rs`) |
