# Golden render review

The hashes in `hashes.toml` guard the renders of the `*.txt` scripts in
this directory (44100 Hz, filter off, volume 100). Before they were first
committed, every render was written to WAV and checked against
`docs/research/04-sound.md` and the sound spec of `add-sound-engine`
(task 9.3). No upstream build was run.

```sh
cargo xtask render-sound crates/digger-sound/tests/golden/gold.txt target/sound/gold.wav
```

## 2026-11-12: waveform analysis (automated)

- **Reviewer:** Claude (Opus 5.5), implementing agent of task group 9 of
  `add-sound-engine`.
- **Method:** every script rendered with `cargo xtask render-sound`
  (defaults: 44100 Hz, filter off, volume 100); each WAV decoded with
  `ffmpeg` (all decode without error) and cut into 606-sample tick windows
  starting at sample 605 (the first tick). For each window the peak
  amplitude and the mean half period were measured, giving the sounding
  divisor (1193181 / Hz) and, for music, the envelope level
  ((level - 1) x 32767 / 49 / 2).
- **Compared, all in agreement:**
  - Effects alone, divisor per tick and duration: gold (31 ticks, 4000,
    531, 3516, 599 ... 582), break (15000 for 3 ticks), explode (1313 ...
    398, 10 ticks), death (audible on ticks 4 to 22 only, 19000 down to
    10000 and up again), fall (1000, 1050, 1100 ... on ticks 3, 7, 11 ...),
    wobble (2500, 3000, 2500, 2000 on ticks 16, 32, 48, 64, repeating),
    eat-monster (three chirps 1758 ... 593 on even ticks, last on tick 62),
    extra life (3 ticks on, 3 off, 980 falling to 612, last on tick 94),
    bonus siren (1230 on ticks 1-5 and 16-21, 1513 on 8-13), emerald click
    (one tick of 1000), emerald scale (14 tone ticks per step, steps 0 to 7
    at 2280, 2032, 1810, 1708, 1522, 1356, 1208, 1140), fireball flight
    (510, 572, 564, 554, 570, 571 on even ticks, inaudible once the divisor
    passes 16383, ended by itself); peaks 16383 throughout.
  - Tunes: normal opens D4 C4 D4 A3 F3 A3 D3 (Popcorn), 13 ticks per
    2-unit note with levels 50, 35, 20, 8 x 8, 7, 6; bonus opens with the
    repeated-note William Tell figure, 7 ticks per 2-unit note; battle notes
    of 4 units last 9 ticks with levels 36, 50, 42 ...; dirge opens with a
    21-tick rest and then the Funeral March figure (C C C C, E-flat D D C C),
    last audible tick 512 at the standard tempo and 362 at the quick tempo,
    silent from ticks 513 and 363.
  - Level-done jingle: 21-tick notes at 515, 646 ... Hz (carrier D + 35),
    beat-gated, over on tick 232; the normal tune requested with it stays
    frozen and starts on tick 232 at level 50.
  - Mixed script: effects displace the tune for their own ticks only; the
    falling bag beats gold, the siren beats eat-monster, extra life beats
    death, a silent tick of the higher effect lets the lower one through,
    and the tune is silent after the death effect.
- **Disagreements found:** none; spec, tests and `docs/compat-quirks.md`
  unchanged.

## Pending: listening review by ear

Task 9.3 also asks for each render to be listened to once. That needs a
person with speakers and was not possible in the sandbox where the hashes
were generated. A reviewer who listens adds a dated entry here naming
themself and what they heard, and ticks task 9.3 once the golden test has
also passed on the Windows and macOS CI runners with the same hashes.
