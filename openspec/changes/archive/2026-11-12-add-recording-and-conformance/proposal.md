# Proposal

## Why

The simulation delivered by `add-simulation-core` is only useful if it behaves
exactly like the original, and the porting plan makes replaying upstream's
recordings the single proof of that. Until the port can read `.drf` and
`.edrf` files, drive a game from them and compare state-hash checkpoints, no
simulation requirement is verified from the outside, and every later phase
would be built on an unproven core.

## What Changes

- Add a reader and a writer for the **DRF** text recording format (header,
  mode line, bonus score, eight level maps, per-round seed, run-length
  direction/fire records, `EOL`/`EOG`, initials).
- Add a reader and a writer for the **eDRF** format (`eDRF 2`: per-slot
  per-tick control bits, inline level maps, seeds, `C`/`E`/`Z` checkpoints,
  `Q` quit record).
- Add playback: a recording supplies the game's per-round seeds, level maps,
  mode and control inputs. DRF records are consumed once per control read;
  eDRF values once per slot per tick.
- Add recording of a game in progress in either format, including
  re-recording during playback (which makes DRF → eDRF conversion possible),
  and the upstream default file naming for saved DRF files.
- Add the **state hash** as a compatibility format: an exact byte stream over
  the simulation state, MD5, first four digest bytes.
- Add checkpoint generation and verification (every 256 ticks, each round
  end, game end), with divergence reports that name the first failing
  checkpoint, the tick and the expected and actual hash.
- Add a headless, unpaced verify run with the upstream result line
  (`score=… [score2=…] level=… ticks=…`) and exit statuses 0/1/2/3/4, exposed
  as a library entry point and as `cargo xtask verify`.
- Copy upstream's 23 `.drf` and 27 `.edrf` recordings and their 27 expected
  result files into `tests/recordings/` with a documented
  `cargo xtask import-recordings` command, and run all 50 as a conformance
  suite in cargo tests and CI.
- Add triage tooling (per-tick state dump, checkpoint-window report, trace
  comparison; no upstream binary is ever built or run) and a
  tracked per-recording checklist; simulation defects found by the suite are
  fixed in `digger-core` within this change and logged in
  `docs/compat-quirks.md`.
- Recordings made by pre-1998 versions (version date ≤ 19981125, upstream's
  "kludge" mode) are **rejected with a clear message**; the monster behaviour
  they need is not ported.
- Taking over from a playback at a chosen tick **is supported** at the
  library level (playback stops supplying input after that tick, live input
  continues, verification is abandoned, the game is not eligible for a high
  score).

### Out of scope

| Item | Picked up by |
|---|---|
| Command-line options of the game binary for play, record, verify, take-over (`--play`, `--record`, `/E:file` aliases, exit codes of the binary) | `add-desktop-app` |
| The "save recording" key, the take-over option (`--take-over-at`, `/T:n`), writing files in the user's directories, entering initials | `add-desktop-app` |
| Running the sound engine during verification (upstream's `long` test tier) | `add-sound-engine` adds its own golden tests; `add-desktop-app` wires sound into replays |
| Real-time replays (upstream's `xlong` tier) and frame pacing | `add-desktop-app` |
| The eDRF NetSim feed, control cross-checks between peers, anything networked | not planned (see porting plan, non-goals) |
| Pre-1998 "kludge" monster behaviour | not planned; such recordings are rejected |
| Upstream level files (`.DLF`), configuration and score files | not planned |
| Golden framebuffer CRCs recorded once conformance passes | `add-desktop-app` (title, level, status line pictures) |

## Capabilities

### New Capabilities

- `game-recording`: the DRF and eDRF file formats (reading and writing), how a
  recording drives a game, how a game is recorded and named, conversion
  between the formats, error handling for malformed input, rejection of
  pre-1998 recordings, take-over from playback.
- `replay-verification`: the state hash byte stream, checkpoint cadence,
  divergence detection and reporting, the headless verify run with its result
  line and exit statuses, configuration isolation, unpaced replay, and the
  conformance suite over upstream's 50 recordings.

### Modified Capabilities

None. The simulation capabilities from `add-simulation-core` (`playfield`,
`digger`, `monsters`, `gold-bags`, `scoring`, `game-flow`) keep their
requirements. Where the conformance suite shows the implementation — or a
requirement — to be wrong, the fix lands in this change; a requirement that
turns out to be wrong is corrected with a delta spec added to this change at
that point and the finding is recorded in `docs/compat-quirks.md`.

## Impact

- `crates/digger-core`: new modules for the DRF codec, the eDRF codec, the
  state hash, the recorder, the replay driver and the state dump. The hash
  reads the simulation through the `StateView` of `add-simulation-core`.
  Defect fixes in
  the simulation modules as the suite demands.
- `crates/digger-core/Cargo.toml`: adds `md-5` (already named in the porting
  plan). No other new runtime dependency.
- `xtask`: new subcommands `import-recordings`, `verify`, `conformance`,
  `convert`, `dump-state`, `cut`, `bisect`, `trace-diff`.
- `tests/recordings/`: 50 recordings, 27 expected-result files, a manifest
  with SHA-256 sums, and a README stating origin (sobomax/digger at commit
  `638125e`) and licence (GPL-2.0-or-later, the same as this repository).
- `crates/digger-core/tests/`: conformance, codec round-trip and
  malformed-input integration tests.
- `.github/workflows`: the quick tier runs in the normal test job; a
  release-mode job runs all 50 recordings.
- `docs/`: `docs/recording-formats.md` (format and hash reference),
  `docs/conformance.md` (how to run, triage procedure, tracking checklist),
  additions to `docs/compat-quirks.md`.
- Later phases: `add-desktop-app` builds its play/record/verify options on
  the library entry points defined here; the simulation interface is used
  as `add-simulation-core` defines it (`step_observed` and its `Observer`).
