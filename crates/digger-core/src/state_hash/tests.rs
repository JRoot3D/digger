#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]

use md5::{Digest, Md5};

use super::layout::{Condition, GROUPS, Repeat, Shape};
use super::*;
use crate::testkit::Scenario;
use crate::{
    DiggerId, FrameInput, Game, GameConfig, GameMode, GauntletSeconds, MonsterSlot, Observer,
    PlayerId, StateView,
};

/// Offset of the digger block in the stream.
const DIGGER_AT: usize = 300;
/// Offset of the monster block in a stream without monster bodies.
const MONSTER_AT: usize = DIGGER_AT + 263;
/// Offset of the random-number state in a stream without monster bodies.
const RNG_AT: usize = MONSTER_AT + 163 + 165;

/// The first four MD5 digest bytes of `bytes`, first byte most significant,
/// computed directly.
fn md5_prefix(bytes: &[u8]) -> u32 {
    let digest = Md5::digest(bytes);
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]])
}

#[test]
fn hash_is_md5_prefix_big_endian() {
    // All-zero values without monster bodies: 903 zero bytes, whose MD5
    // (computed independently) is e2f00d570092fdbb8696d63e3d4d0fdb.
    let fields = StateHashFields::default();
    assert_eq!(fields.stream(None), vec![0; 903]);
    assert_eq!(fields.hash(), 0xE2F0_0D57);
    assert_eq!(format_hash(fields.hash()), "E2F00D57");

    let fields = StateHashFields {
        rng_state: 0x1234_5678,
        ..StateHashFields::default()
    };
    assert_eq!(fields.hash(), md5_prefix(&fields.stream(None)));

    // The spec's examples of the text form.
    assert_eq!(format_hash(0x9E10_7D9D), "9E107D9D");
    assert_eq!(format_hash(0x000A_BC01), "000ABC01");
}

#[test]
fn values_are_little_endian() {
    let mut fields = StateHashFields::default();
    fields.playfield[0] = 300;
    fields.playfield[149] = 0xD03F;
    fields.digger.shared.bonus_start_time_left = -2;
    fields.rng_state = 0x1122_3344;
    fields.scores = [3100, -2];
    let stream = fields.stream(None);
    assert_eq!(stream.len(), 903);
    assert_eq!(stream[0..2], [0x2C, 0x01]);
    assert_eq!(stream[298..300], [0x3F, 0xD0]);
    assert_eq!(stream[DIGGER_AT..DIGGER_AT + 2], [0xFE, 0xFF]);
    assert_eq!(stream[RNG_AT..RNG_AT + 4], [0x44, 0x33, 0x22, 0x11]);
    assert_eq!(stream[RNG_AT + 4..RNG_AT + 8], [0x1C, 0x0C, 0x00, 0x00]);
    assert_eq!(stream[RNG_AT + 8..RNG_AT + 12], [0xFE, 0xFF, 0xFF, 0xFF]);
}

#[test]
fn bool_is_one_byte() {
    let mut fields = StateHashFields::default();
    fields.digger.shared.bonus_mode = true;
    fields.digger.slots[0].dead = false;
    fields.digger.slots[1].alive = true;
    let stream = fields.stream(None);
    assert_eq!(stream.len(), 903);
    // Bonus visible, bonus mode, diggers visible follow three 16-bit values.
    assert_eq!(stream[DIGGER_AT + 6..DIGGER_AT + 9], [0x00, 0x01, 0x00]);
    // Slot 2's "alive" flag: after the shared 159 bytes, slot 1's 52 bytes
    // and slot 2's 16 × 16-bit, 5 flags and 3 × 16-bit values.
    let alive = DIGGER_AT + 159 + 52 + 32 + 5 + 6;
    assert_eq!(stream[alive], 0x01);
    // Slot 1's "dead" flag is the third of its five flags.
    assert_eq!(stream[DIGGER_AT + 159 + 32 + 2], 0x00);
}

#[test]
fn no_direction_is_ffff() {
    let mut fields = StateHashFields::default();
    fields.monster.slots[0].direction = codes::optional_direction(None);
    let stream = fields.stream(None);
    // Slot 0 follows the 13 schedule bytes; its direction follows four
    // 16-bit values.
    let at = MONSTER_AT + 13 + 8;
    assert_eq!(stream[at..at + 2], [0xFF, 0xFF]);
}

#[test]
fn round_end_appends_tick() {
    let fields = StateHashFields::default();
    let plain = fields.stream(None);
    let round_end = fields.stream(Some(1378));
    assert_eq!(round_end.len(), 907);
    assert_eq!(round_end[..903], plain[..]);
    assert_eq!(round_end[903..], [0x62, 0x05, 0x00, 0x00]);
    // MD5 of 903 zero bytes and 62 05 00 00, computed independently.
    assert_eq!(fields.hash_at_round_end(1378), 0xCFBF_B8A6);
    assert_ne!(
        fields.hash_at_round_end(1378),
        fields.hash_at_round_end(1379)
    );
    assert_ne!(fields.hash_at_round_end(0), fields.hash());
}

#[test]
fn gauntlet_appends_clock_and_timeout() {
    let fields = StateHashFields {
        gauntlet: Some(GauntletHashFields {
            clock: 143_181_720,
            timed_out: true,
        }),
        ..StateHashFields::default()
    };
    let stream = fields.stream(None);
    assert_eq!(stream.len(), 908);
    assert_eq!(stream[903..907], 143_181_720_u32.to_le_bytes());
    assert_eq!(stream[907], 0x01);

    let round_end = fields.stream(Some(7));
    assert_eq!(round_end.len(), 912);
    assert_eq!(round_end[..908], stream[..]);
    assert_eq!(round_end[908..], [7, 0, 0, 0]);
}

/// The stream length of the state of `game`.
fn stream_len(game: &Game) -> usize {
    StateHashFields::from_view(game.state()).stream(None).len()
}

#[test]
fn hash_stream_length_fresh_game() {
    let mut game = Game::new(GameConfig::default(), 0);
    assert_eq!(stream_len(&game), 903);
    // The first ticks of the round, before the first monster appears on
    // the 11th, are still 903 bytes long.
    while game.state().ticks() < 5 {
        game.step(&FrameInput::NONE);
    }
    assert_eq!(stream_len(&game), 903);

    let config = GameConfig {
        mode: GameMode::Gauntlet {
            time: GauntletSeconds::DEFAULT,
        },
        ..GameConfig::default()
    };
    let game = Game::new(config, 0);
    assert_eq!(stream_len(&game), 908);
    let fields = StateHashFields::from_view(game.state());
    assert_eq!(
        fields.gauntlet,
        Some(GauntletHashFields {
            clock: 143_181_720,
            timed_out: false,
        })
    );
    // Part 6 always holds two scores, the second one zero here.
    assert_eq!(fields.scores, [0, 0]);
}

/// The byte length of the monster block of `state`.
fn monster_len(state: StateView<'_>) -> usize {
    StateHashFields::from_view(state)
        .block_hashes(None)
        .len(Block::Monster)
}

#[test]
fn monster_block_is_variable_length() {
    let mut run = Scenario::new(GameMode::OnePlayer).build();
    run.start();
    assert_eq!(monster_len(run.state()), 163);

    let slot = MonsterSlot::ALL[0];
    assert!(run.run_until(|s| s.monster(slot).spawned(), 1000));
    assert_eq!(monster_len(run.state()), 171);
    assert_eq!(
        StateHashFields::from_view(run.state()).stream(None).len(),
        911
    );

    // A killed monster keeps its body until the round's monster reset.
    run.set_spawn_timer(1000).expect("in a round");
    run.remove_monster(slot).expect("in a round");
    let fields = StateHashFields::from_view(run.state());
    let removed = fields.monster.slots[0];
    assert!(!removed.in_use);
    let body = removed.body.expect("the body stays after the kill");
    assert!(!body.alive);
    assert_eq!(monster_len(run.state()), 171);
    run.run_ticks(3);
    assert_eq!(monster_len(run.state()), 171);

    // The next round starts with every slot reset.
    run.kill_digger(DiggerId::ALL[0], crate::testkit::KillCause::Monster)
        .expect("in a round");
    assert!(run.run_until(|s| s.round() == 1, 10_000));
    run.start();
    assert_eq!(monster_len(run.state()), 163);
}

/// The block of every byte of the stream, from the decoded values.
fn block_streams(fields: &StateHashFields, tick: Option<u32>) -> [Vec<u8>; 5] {
    let mut streams: [Vec<u8>; 5] = Default::default();
    for item in decode(fields, tick) {
        let stream = &mut streams[item.entry.block.index()];
        item.value
            .write_le(&mut |bytes| stream.extend_from_slice(bytes));
    }
    streams
}

/// The state of a game a little way into its first round, with monsters.
fn played_fields() -> StateHashFields {
    let mut run = Scenario::new(GameMode::TwoDiggers).build();
    run.start();
    run.run_ticks(120);
    StateHashFields::from_view(run.state())
}

#[test]
fn block_hashes_recombine() {
    let fields = played_fields();
    assert!(
        fields.monster.slots.iter().any(|slot| slot.body.is_some()),
        "monsters are in play"
    );
    for tick in [None, Some(120)] {
        let streams = block_streams(&fields, tick);
        let blocks = fields.block_hashes(tick);
        let mut whole = Vec::new();
        for block in Block::ALL {
            let stream = &streams[block.index()];
            assert_eq!(blocks.hash(block), md5_prefix(stream), "{block:?}");
            assert_eq!(blocks.len(block), stream.len(), "{block:?}");
            whole.extend_from_slice(stream);
        }
        assert_eq!(whole, fields.stream(tick));
        let full = match tick {
            Some(tick) => fields.hash_at_round_end(tick),
            None => fields.hash(),
        };
        assert_eq!(md5_prefix(&whole), full);
    }
    assert_eq!(fields.block_hashes(None).len(Block::Playfield), 300);
    assert_eq!(fields.block_hashes(None).len(Block::Digger), 263);
    assert_eq!(fields.block_hashes(None).len(Block::Bag), 165);
}

/// The `(entry, index)` sequence the table describes for `fields`: every
/// entry in order, a per-cell entry 150 times, each run of consecutive
/// entries with the same per-slot repeat once per slot, monster body
/// entries only for slots with a body, gauntlet entries in the gauntlet
/// modes and the tick in the round-end form.
fn expected_order(
    fields: &StateHashFields,
    round_end: bool,
) -> Vec<(&'static layout::Entry, Option<usize>)> {
    let present = |entry: &layout::Entry, slot: Option<usize>| match entry.condition {
        Condition::Always => true,
        Condition::MonsterBody => {
            slot.is_some_and(|slot| fields.monster.slots[slot].body.is_some())
        }
        Condition::Gauntlet => fields.gauntlet.is_some(),
        Condition::RoundEnd => round_end,
    };
    let entries: Vec<&'static layout::Entry> = GROUPS.into_iter().flatten().collect();
    let mut order = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        let entry = entries[i];
        match entry.repeat {
            Repeat::Once => {
                if present(entry, None) {
                    order.push((entry, None));
                }
                i += 1;
            }
            Repeat::PerCell => {
                order.extend((0..layout::CELLS).map(|cell| (entry, Some(cell))));
                i += 1;
            }
            repeat @ (Repeat::PerDigger | Repeat::PerMonster | Repeat::PerBag) => {
                let run: Vec<&'static layout::Entry> = entries[i..]
                    .iter()
                    .copied()
                    .take_while(|e| e.repeat == repeat)
                    .collect();
                for slot in 0..repeat.count() {
                    for &e in &run {
                        if present(e, Some(slot)) {
                            order.push((e, Some(slot)));
                        }
                    }
                }
                i += run.len();
            }
        }
    }
    order
}

#[test]
fn decoded_dump_matches_layout_order() {
    let mut fields = played_fields();
    fields.gauntlet = Some(GauntletHashFields {
        clock: 5,
        timed_out: false,
    });
    let bodies = fields
        .monster
        .slots
        .iter()
        .filter(|slot| slot.body.is_some())
        .count();
    for tick in [None, Some(99)] {
        let decoded = decode(&fields, tick);
        let got: Vec<(&'static layout::Entry, Option<usize>)> = decoded
            .iter()
            .map(|item| (item.entry, item.index))
            .collect();
        assert_eq!(got, expected_order(&fields, tick.is_some()));
        for item in &decoded {
            assert_eq!(item.value.width(), item.entry.width, "{}", item.name());
        }
        let bytes: usize = decoded.iter().map(|item| item.value.width().bytes()).sum();
        let shape = Shape {
            bodies,
            gauntlet: true,
            round_end: tick.is_some(),
        };
        assert_eq!(bytes, shape.stream_len());
    }

    let decoded = decode(&fields, None);
    assert_eq!(
        decoded[0].to_string(),
        format!("playfield.cell[0]={}", Value::U16(fields.playfield[0]))
    );
    let lives = decoded
        .iter()
        .find(|item| item.name() == "digger.lives[0]")
        .expect("the first digger's lives");
    assert_eq!(lives.value, Value::I16(3));
    let rng = decoded
        .iter()
        .find(|item| item.name() == "game.rng_state")
        .expect("the generator state");
    assert_eq!(rng.value, Value::I32(fields.rng_state));
}

#[test]
fn gauntlet_clock_wrap_hashed() {
    let config = GameConfig {
        mode: GameMode::Gauntlet {
            time: GauntletSeconds::from(1),
        },
        ..GameConfig::default()
    };
    let mut game = Game::new(config, 0);
    let mut frames = 0;
    while !game.state().timed_out() {
        game.step(&FrameInput::NONE);
        frames += 1;
        assert!(frames < 10_000, "the one-second gauntlet times out");
    }
    let fields = StateHashFields::from_view(game.state());
    let gauntlet = fields.gauntlet.expect("a gauntlet game");
    assert!(gauntlet.timed_out);
    // The tick that ran out wrapped the clock to just below 2^32.
    assert!(gauntlet.clock > u32::MAX - 80_000, "{}", gauntlet.clock);
    let stream = fields.stream(None);
    let len = stream.len();
    assert_eq!(stream[len - 5..len - 1], gauntlet.clock.to_le_bytes());
    assert_eq!(stream[len - 1], 0x01);
}

/// Records what the round-end hook sees.
#[derive(Debug, Default)]
struct RoundEndProbe {
    /// Per round end: the hashed fields, the tick, whether the live field
    /// equals the current player's saved field, whether any monster slot
    /// had been used this round, and whether any monster sprite was still
    /// shown.
    seen: Vec<(StateHashFields, u32, bool, bool, bool)>,
}

impl Observer for RoundEndProbe {
    fn round_end(&mut self, state: StateView<'_>) {
        let player = state.current_player();
        let saved = state.field() == state.saved_field(player);
        let spawned = MonsterSlot::ALL
            .into_iter()
            .any(|slot| state.monster(slot).spawned());
        let shown = MonsterSlot::ALL
            .into_iter()
            .any(|slot| state.sprites().is_enabled(slot.sprite()));
        self.seen.push((
            StateHashFields::from_view(state),
            state.ticks(),
            saved,
            spawned,
            shown,
        ));
    }
}

#[test]
fn round_end_hash_after_cleanup() {
    // Quirk P11: the round-end hash covers the state after the monsters
    // are removed, the bags tidied and the field saved, with the tick.
    let mut run = Scenario::new(GameMode::OnePlayer).build_observed(RoundEndProbe::default());
    run.start();
    assert!(run.run_until(|s| s.monster(MonsterSlot::ALL[0]).spawned(), 1000));
    run.kill_digger(DiggerId::ALL[0], crate::testkit::KillCause::Monster)
        .expect("in a round");
    assert!(run.run_until(|s| s.round() == 1, 10_000));
    let [(fields, tick, saved, spawned, shown)] = run.observer().seen.as_slice() else {
        panic!("one round end");
    };
    assert!(*saved, "the field is saved before the hook");
    assert!(*spawned, "monsters were spawned this round");
    // `erasemonsters` erases the sprites; the slots keep their "in use"
    // flags until the next round's reset.
    assert!(!*shown, "the monsters are erased before the hook");
    assert!(
        fields.monster.slots.iter().any(|slot| slot.body.is_some()),
        "the bodies stay until the next round's reset"
    );
    assert!(fields.digger.slots[0].not_firing, "the fireball is removed");
    assert!(!fields.digger.shared.bonus_visible, "the bonus is removed");
    assert_eq!(*tick, run.ticks());
    assert_eq!(
        fields.hash_at_round_end(*tick),
        md5_prefix(&fields.stream(Some(*tick)))
    );
    assert_eq!(
        fields.digger.shared.emerald_mask,
        codes::emerald_mask(PlayerId::ALL[0])
    );
}
