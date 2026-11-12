# tests/recordings/

Upstream's recording corpus, used by the conformance suite. **Do not edit
these files by hand.** Every file except this README and `port/` is written
by `cargo xtask import-recordings` and guarded by `MANIFEST.sha256` and the
test `crates/digger-core/tests/corpus_integrity.rs`; a change made by hand
makes `cargo test --workspace` fail. `port/` holds the port's own regression
fixtures for the modes listed under "Known gaps" (see `port/README.md`); they
are not upstream recordings.

## Origin

Copied byte-for-byte from [sobomax/digger](https://github.com/sobomax/digger)
at commit `638125eecc2a36629b886f58d7b75253894cf3d3`:

| Here | Upstream path | Content |
|---|---|---|
| `data/*.drf` | `tests/data/*.drf` | 23 DRF recordings |
| `data/*.edrf` | `tests/data/*.edrf` | 27 eDRF recordings |
| `results/*.out` | `tests/results/*.out` | 27 expected result lines, one file per base name, shared by a DRF and its eDRF twin |
| `MANIFEST.sha256` | (generated) | SHA-256 of every file above, in `sha256sum -c` format |

No upstream code is compiled, linked or run; the recordings are read as data.
`data/muu27700.drf` has CR LF line ends and is kept that way: `.gitattributes`
marks `tests/recordings/**` as `-text` so that no checkout converts line ends.

## Re-importing

Given a checkout of sobomax/digger at the pinned commit:

```sh
cargo xtask import-recordings --from <upstream checkout>
```

The command reads the checkout's `HEAD` from its `.git` metadata and refuses,
changing nothing, if it is not `638125e…`; `--allow-other-commit` imports
anyway (the expected results then describe a different upstream). Building
and testing never need the checkout. To check the files by hand:

```sh
cd tests/recordings && sha256sum -c MANIFEST.sha256
```

## Modes covered

- `1` — one player: 22 DRF files and their 22 eDRF twins.
- `M2` — two simultaneous diggers: `2test01` (DRF and eDRF) and the eDRF-only
  `2riptest01` (ends with a `Q 0` quit record at tick 1024), `2test02`,
  `2test03`.
- `G120` — gauntlet, 120 seconds: `gauntlet01` (eDRF only).

DRF recorder versions present: `AJ DOS 19990130`, `AJ DOS 19990506`,
`TD WIN 20020310`, `POK SDL 20110912` and `MS MPL 20190212`. Every eDRF file
was written by `MS MPL 20190212`.

## Known gaps

- No alternating two-player game (mode `2`); `port/alternating.*` is a
  port-made regression fixture, not upstream-verified.
- No recording with a start level (`I`); `port/start-level-5.*` is a
  port-made regression fixture, not upstream-verified.
- No pre-1998 ("kludge"-era) file: no recording has a version date of
  19981125 or earlier, so the old monster behaviour such files select is
  never exercised; the port refuses them.

## Licence

These recordings and results are part of sobomax/digger and are distributed
under the GNU General Public License, version 2 or (at your option) any later
version, the same licence as this repository. See [`LICENSE`](../../LICENSE)
and [`NOTICE`](../../NOTICE).
