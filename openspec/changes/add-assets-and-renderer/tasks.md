# Tasks

Conventions for every group: work from `docs/research/03-graphics.md`,
`docs/research/02-entities.md` §0–§1, the specs and `design.md`; do not open
upstream C source except as input data to `xtask import-assets`. The last
task of each group is the quality gate and the commit.

## 1. Importer: reading upstream tables

- [x] 1.1 Add the `import-assets` subcommand to `xtask` with arguments `--upstream <dir>`, `--out <dir>` (default `assets/`) and `--check`, plus the `ImportError` enum from design D7; verify `cargo xtask import-assets --help` lists the three arguments and that running without `--upstream` exits non-zero with a usage message
- [x] 1.2 Implement the table reader (`xtask/src/import/ctable.rs`) for byte and 32-bit array initialisers with decimal and hexadecimal numbers, comments and qualifiers; verify with unit tests `ctable_reads_byte_array`, `ctable_reads_u32_array` and `ctable_skips_comments` on hand-written fixtures
- [x] 1.3 Extend the reader to pointer tables whose elements are identifiers, `NULL` or `0`, resolving identifiers against the arrays read from the same file; verify with `ctable_reads_pointer_table` and `ctable_unknown_symbol_is_error`
- [x] 1.4 Make every malformed input a `Syntax` error carrying file name and line, never a panic; verify with `ctable_reports_line_of_bad_token`, `ctable_unterminated_initialiser_is_error` and a test feeding 200 pseudo-random byte strings that must all return `Ok` or `Err`
- [x] 1.5 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(xtask): add upstream data table reader for import-assets`

## 2. Importer: producing the asset files

- [x] 2.1 Implement the sprite import: read the 240-entry index table, apply the fixed size-by-number table (research 03 §5.2), validate lengths (exact for image-specific arrays, at-least for shared arrays), de-duplicate arrays by content and plan `sprites.bin` plus the 120 index entries; verify with `sprites_import_fixture` (a 3-image fixture) and `sprites_wrong_length_is_error`
- [x] 2.2 Implement the font import: 40 glyphs of 576 bytes in the order A–Z, 0–9, `.`, `:`, `_`, space, and the 95-entry character map; verify with `font_import_fixture`, `font_lowercase_maps_to_uppercase_glyph` and `font_wrong_glyph_size_is_error`
- [x] 2.3 Implement the title import (copy the stream verbatim, inflate to check the 256000-byte length, print the CRC-32 of the inflated bytes) and the icon import (4096 ARGB words to 16384 R, G, B, A bytes); add `miniz_oxide` and `sha2` to `xtask`; verify with `title_import_checks_inflated_length` and `icon_channel_order`
- [x] 2.4 Generate `index.rs` (`SPRITE_INDEX`, `GLYPH_INDEX`) and `manifest.txt` (upstream commit header; SHA-256, length and name per file) from the planned outputs; verify with `index_rs_is_stable_text` (snapshot of the fixture output) and `manifest_lists_every_output`
- [x] 2.5 Implement validate-then-write: build everything in memory, write each file through a temporary file and rename, and write nothing on any error; verify with `import_is_deterministic` (two runs, identical bytes) and `failed_import_leaves_out_dir_untouched`
- [x] 2.6 Implement `--check`: compare the in-memory result with the files in `--out`, list differing, missing and extra files, modify nothing; verify with `check_passes_on_identical_output` and `check_reports_changed_file`
- [x] 2.7 Confirm that CGA tables are never read: verify with `cga_tables_are_ignored` (fixture directory containing a CGA file whose content is invalid; import still succeeds)
- [x] 2.8 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(xtask): write sprite, font, title and icon assets with manifest`

## 3. Import and commit the assets

- [x] 3.1 Fetch upstream `sobomax/digger` at commit `638125eecc2a36629b886f58d7b75253894cf3d3` into a temporary directory outside the repository and run `cargo xtask import-assets --upstream <dir>`; verify exit status 0 and that `assets/` holds `sprites.bin`, `font.bin` (23040 bytes), `title.zz` (2772 bytes), `icon.rgba` (16384 bytes), `index.rs` and `manifest.txt`; if the reader rejects real upstream formatting, fix it in group 1's module with a new fixture test
- [x] 3.2 Run `cargo xtask import-assets --upstream <dir> --check` and a second full import; verify the check exits 0 and `git status` shows no change after the second import
- [x] 3.3 Add `xtask/tests/asset_manifest.rs` that hashes every file in `assets/` against `manifest.txt` and fails on a mismatch, a missing file or an unlisted file (ignoring `README.md` and `manifest.txt` itself only if they are listed as exempt in the test); verify with `cargo test -p xtask --test asset_manifest`, and by temporarily flipping one byte of `font.bin` and seeing the test name that file
- [x] 3.4 Write `assets/README.md` (origin, pinned commit, GPL-2.0-or-later notice, "do not edit by hand") and `docs/assets.md` (file formats from design D2, how to re-import and check, how later changes add level and music tables); verify the two commands in `docs/assets.md` run exactly as written
- [x] 3.5 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(assets): import upstream sprite, font, title and icon data`

## 4. Asset access in digger-core

- [x] 4.1 Create the `assets` module: embed `sprites.bin`, `font.bin` and `title.zz`, include `index.rs`, add `const` assertions on blob lengths, and define `ImageSize`, `ImageId` (constructor returns `None` for 120 and above) and `ImageId::get()`; verify with `image_id_rejects_120`, `image_count_is_120` and `image_byte_size_matches_dimensions`
- [x] 4.2 Add the named `ImageId` constants and helpers needed by later phases (blank, bag frames, gold, monsters, bonus, fireball, `background(plan)`, tunnel shapes, emerald and eraser, life icons, `for_second_digger()`, `for_second_fireball()`), each with its research citation; verify with `image_sizes_by_number` (the full size table of the spec), `second_digger_offset_is_31`, `second_fireball_offset_is_32` and `duplicate_image_numbers`
- [x] 4.3 Add the content guarantees as tests over the embedded data: `image_values_in_allowed_set`, `mask_values_are_0_or_255`, `coloured_pixels_have_zero_mask`, `eraser_images_are_zero`, `opaque_images_have_zero_mask`, `tile_colour_sets`, `tunnel_masks_are_pixel_doubled`; verify all pass
- [x] 4.4 Implement glyph lookup by character; verify with `glyph_coverage_is_66_of_95`, `lowercase_shares_uppercase_glyph`, `hyphen_has_no_glyph`, `glyph_values_and_ink_columns` and `space_glyph_is_blank`
- [x] 4.5 Implement `TitlePicture::decode()` with `AssetError` (add `miniz_oxide` to `digger-core`); verify with `title_decodes_to_256000_bytes`, `title_value_set`, `title_landmark_pixels` and `title_truncated_stream_is_error` (decoding the first 1000 bytes through the same internal function)
- [x] 4.6 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(core): embed assets with typed image, glyph and title access`

## 5. Framebuffer and palettes

- [x] 5.1 Add `geom` (`Px`, `align_x`, logical rectangle helpers) and `Framebuffer` with creation, `clear`, `pixels()` and the logical-to-real mapping; verify with `new_framebuffer_is_black_palette_zero`, `clear_keeps_palette` and `align_x_rounds_toward_negative_infinity`
- [x] 5.2 Add `Palette`, `Intensity`, `Rgb` and the four colour tables, `set_palette` (resets intensity, quirk R3), `set_intensity` and `colors()`; verify with `palette_tables_match_spec` (all 64 entries), `palette_select_resets_intensity`, `intensity_keeps_palette` and `palette_change_leaves_pixels`
- [x] 5.3 Implement the in-crate CRC-32 and `Framebuffer::crc32()`; verify with `crc32_known_vectors` (empty input, "123456789" = 0xCBF43926) and `crc32_of_cleared_frame` against the independently computed CRC-32 of 256000 zero bytes
- [x] 5.4 Implement `blit_masked` with the original rule (quirk R4) and clipping on all four edges; verify with `blit_rule_truth_table`, `blit_clips_bottom_edge`, `blit_clips_left_edge`, `blit_fully_outside_is_noop` and `blit_maps_logical_to_real`
- [x] 5.5 Implement `SavedRect`, `capture` (zero-filled outside the picture) and `restore` (opaque, own size, clipped); verify with `capture_restore_round_trip`, `capture_zero_fills_outside`, `restore_is_opaque` and `restore_uses_saved_size`
- [x] 5.6 Implement the pixel probe returning `ProbeBits` with the documented in-range rule, the 0xFF result beyond x 319 / y 199, row wrap and zero reads outside the buffer (quirks R1, R2); verify with `probe_vectors`, `probe_beyond_right_or_bottom_is_ff`, `probe_row_wrap`, `probe_outside_buffer_reads_zero` and `probe_negative_coordinates`
- [x] 5.7 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(core): add indexed framebuffer, palettes, masked blit and pixel probe`

## 6. Text, numbers, background and title

- [x] 6.1 Implement `TextColor` and `draw_char` (opaque 12x12 cell, recolouring table, unaligned x, no-op for a character without a glyph); verify with `text_cell_is_opaque`, `text_recolour_table` (all four colours), `text_unaligned_position` and `draw_char_without_glyph_is_noop`
- [x] 6.2 Implement `draw_text` and `erase_text` (12-pixel advance, unsupported characters as spaces, quirk R5); verify with `text_advance_is_12`, `text_invalid_char_becomes_space`, `text_lowercase_equals_uppercase` and `erase_top_line_covers_320_columns`
- [x] 6.3 Implement `draw_number` (right-aligned, leftmost cell only when non-zero, quirk R6); verify with `number_zero_width_6`, `number_six_digits`, `number_leftmost_cell_not_erased`, `number_width_1_zero_draws_nothing` and `number_drops_high_digits`
- [x] 6.4 Implement `LevelPlan` (1–8, constructor returns `None` otherwise) and `draw_background` (752 tiles from y 14, last row clipped, quirk R7); verify with `level_plan_rejects_0_and_9`, `background_coverage`, `background_plan_selects_tile` and `background_preserves_top_line`
- [x] 6.5 Implement `draw_title`; verify with `title_replaces_whole_picture`, `title_keeps_palette` and `title_text_fits_in_black_margin`
- [x] 6.6 Document the public `video` API with rustdoc, including a module-level explanation of logical versus real coordinates and why the picture is game state; verify with `cargo doc -p digger-core --no-deps` producing no warnings
- [x] 6.7 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(core): add text, number, background and title drawing`

## 7. Sprite layer

- [x] 7.1 Add `SpriteId`, `SpriteClass` (class ranges 0, 1–7, 8–13, 14–15, 16–17) and `Collisions` as a per-class ascending iterator over a bit set; verify with `sprite_class_ranges`, `sprite_id_constructors_reject_out_of_range` and `collisions_iterate_ascending_per_class`
- [x] 7.2 Add `SpriteLayer` with initial state, `reset`, `set_pending`, the sprite rectangle from the current image and the overlap predicate; verify with `initial_state_all_disabled`, `set_pending_changes_nothing_visible`, `reset_does_not_erase`, `overlap_16x15_pairs`, `overlap_touching_edges` and `overlap_fireball_vs_16x15`
- [x] 7.3 Implement the redraw set as a bit-set worklist (starting rectangle given explicitly, enabled sprites only); verify with `redraw_set_chain`, `redraw_set_skips_disabled`, `redraw_set_isolated_is_empty` and `redraw_set_includes_enabled_start_slot`
- [x] 7.4 Implement `place` in the specified step order, without collision output (quirk S4 included); verify with `place_first_appearance`, `place_under_higher_sprite_saves_clean_background`, `place_does_not_touch_collisions`, `place_enabled_sprite_quirk` and `place_aligns_x`
- [x] 7.5 Implement `erase`; verify with `erase_lone_sprite_restores_picture`, `erase_under_overlapping_sprite`, `erase_twice_is_noop` and `erase_keeps_collisions`
- [x] 7.6 Implement `move_to` in the specified step order: union of old and new redraw sets, restore, latch, capture, draw (quirks S3, S5); verify with `move_removes_old_image`, `move_repairs_old_neighbour`, `move_onto_neighbour_saves_clean_background`, `move_in_place_animates`, `move_without_place_is_safe`, `move_after_erase_restores_stale_background` and `move_restores_saved_size`
- [x] 7.7 Compute the collision lists as the last step of `move_to`, return and store them, and add `collisions()` and `clear_collisions()` (quirk S2); verify with `collisions_one_of_each`, `collisions_ascending_within_class`, `collisions_exclude_moved_sprite`, `collisions_exclude_disabled`, `collisions_only_on_move`, `collision_uses_current_size`, `enabled_not_alive` and `clear_collisions_empties_lists`
- [x] 7.8 Implement `draw_static` (protected: restore, masked draw at aligned x, re-capture, redraw) and `draw_static_unprotected` (quirk S1); verify with `static_draw_under_digger_becomes_background`, `static_draw_without_sprites_equals_direct_blit`, `static_draw_leaves_distant_sprites`, `static_unprotected_touches_no_sprite` and `alignment_sites`
- [x] 7.9 Add `stacking_order` and `stacking_order_within_class` tests on overlapping sprites of different and equal classes (quirk S7), and rustdoc for the `sprite` module stating the call discipline and the step order of each operation; verify the tests pass and `cargo doc -p digger-core --no-deps` has no warnings
- [x] 7.10 Run `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`; commit as `feat(core): add sprite layer with background save/restore and collision lists`

## 8. Model-based tests, golden frames and quirk record

- [x] 8.1 Add `crates/digger-core/tests/sprite_model.rs`: a fixed-seed generator producing at least 20 sequences of 2000 disciplined operations (place, move, pending image, erase, protected static draw) on a tiled background, checking after every step that the picture equals the composite oracle and the lists equal a brute-force scan, and at the end that erasing everything leaves the static background; verify with `cargo test -p digger-core --test sprite_model`
- [ ] 8.2 Add `cargo xtask render-sheet`, writing PPM files to `target/sheets/`: a contact sheet of all 120 images over a tile, the font in four colours, the title picture, the eight backgrounds and the scripted sprite scene, each through palette 0 normal; verify the command exits 0, writes the files, and a person confirms the pictures look right (digger, monsters, bags, legible font, framed title)
- [ ] 8.3 Add `crates/digger-core/tests/golden_frames.rs` with recorded CRC-32 values for: title picture (must equal the value printed by the importer in 2.3), title plus "D I G G E R" at (100, 0) colour 3, backgrounds for plans 1–8, the text sheet, the number sheet and the scripted sprite scene; a failing assertion prints the actual value; verify with `cargo test -p digger-core --test golden_frames`
- [ ] 8.4 Create `docs/compat-quirks.md` with the quirk policy summary from the porting plan and the entries R1–R7, S1–S7 and A1 of design D10, each naming its test; verify every test name in the file exists with `cargo test --workspace -- --list`
- [ ] 8.5 Add a "Rendering and sprite layer" section to the `digger-core` crate documentation with a short usage example (place, move, read collisions, probe) compiled as a doc test; verify with `cargo test -p digger-core --doc`
- [ ] 8.6 Run `openspec validate add-assets-and-renderer --strict`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace`, and confirm `cargo tree -p digger-core` shows `miniz_oxide` (and its own dependencies) as the only non-workspace dependency; commit as `test(core): add sprite model tests, golden frames and compat quirk record`

## Workflow follow-up

- Archive the change with `/opsx:archive add-assets-and-renderer` after review, so that `game-assets`, `rendering` and `sprite-layer` become main specs before `add-simulation-core` is applied.
- `add-simulation-core` extends `xtask import-assets` with the level tables (`assets/levels.bin`) and appends to `docs/compat-quirks.md`; `add-sound-engine` extends the importer with the music tables (`assets/music.bin`). Both keep this change's flags (`--upstream`, `--out`, `--check`), validate-then-write rule and `manifest.txt` format.
