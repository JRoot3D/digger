# tests/recordings/port/

**Regression fixtures made by this port, not upstream recordings.** They are
not upstream-verified: nothing proves that the original game plays them the
same way. They pin the port's own behaviour in the two modes upstream's
corpus never recorded, so that a later change cannot alter those modes
unnoticed.

| Files | Game | Script |
|---|---|---|
| `alternating.drf`, `alternating.edrf`, `alternating.out` | two alternating players (mode line `2`); player 1 clears level 1 | a bot that sweeps the field row by row and fires every third frame, game seed 11 |
| `start-level-5.drf`, `start-level-5.edrf`, `start-level-5.out` | one player from level 5 (mode line `1I5`) | the generated controls of `crates/digger-core/tests/common/inputs.rs`, seed 272 |

Each `.out` holds the result line both files of the game replay to. The
fixtures are checked by `crates/digger-core/tests/port_fixtures.rs`:

- `port_fixture_alternating` and `port_fixture_start_level`: the eDRF
  verifies against its own checkpoints, the DRF (which has none) plays to
  its end, and both give the `.out` result line;
- `port_fixtures_regenerate_identically`: playing the scripts again records
  the same bytes.

The files are written by the port's recorders, never by hand. After a
deliberate change to the simulation or to the writers, regenerate them with

```sh
DIGGER_WRITE_PORT_FIXTURES=1 cargo test -p digger-core --test port_fixtures
```

and review the change. They are outside `MANIFEST.sha256`, which covers the
upstream corpus only. `.gitattributes` keeps them byte-for-byte, as the rest
of `tests/recordings/`.
