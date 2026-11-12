//! The wire codes of the state hash: how each of the simulation's enums
//! and identifiers is written into the stream (replay-verification spec,
//! "State stream value encoding"; research 01 §8, §9; design D5).
//!
//! Every mapping is an exhaustive `match`, so adding a variant to one of
//! the simulation's enums is a compile error here rather than a silent
//! change of the hash. The original holds all of these values as signed
//! 16-bit integers, so every code is an `i16`.

use crate::{BagId, DeathStage, DiggerId, Direction, FireballStage, MonsterDeath, PlayerId};

/// The code of "no direction" (the original's `DIR_NONE`), written as the
/// bytes FF FF.
pub const NO_DIRECTION: i16 = -1;

/// The original's number of `direction`: right 0, up 2, left 4, down 6.
pub const fn direction(direction: Direction) -> i16 {
    match direction {
        Direction::Right => 0,
        Direction::Up => 2,
        Direction::Left => 4,
        Direction::Down => 6,
    }
}

/// The code of an optional direction: [`direction`], or [`NO_DIRECTION`]
/// for none.
pub const fn optional_direction(dir: Option<Direction>) -> i16 {
    match dir {
        Some(dir) => direction(dir),
        None => NO_DIRECTION,
    }
}

/// A digger's death stage (`deathstage`): the stage's number 1 to 6, or 0
/// for a record that was never placed.
pub const fn death_stage(stage: Option<DeathStage>) -> i16 {
    match stage {
        None => 0,
        Some(DeathStage::Bag) => 1,
        Some(DeathStage::Grave) => 2,
        Some(DeathStage::Monster) => 3,
        Some(DeathStage::Tombstone) => 4,
        Some(DeathStage::Arc) => 5,
        Some(DeathStage::Done) => 6,
    }
}

/// A monster's death (`death`): 0, 1 or 4.
pub const fn monster_death(death: MonsterDeath) -> i16 {
    match death {
        MonsterDeath::None => 0,
        MonsterDeath::Carried => 1,
        MonsterDeath::Ending => 4,
    }
}

/// A fireball's stage (`expsn`): 0 flying, 1 to 3 exploding, 4 spent.
pub const fn fireball_stage(stage: FireballStage) -> i16 {
    match stage {
        FireballStage::Flying => 0,
        FireballStage::Exploding { next_frame } => next_frame as i16,
        FireballStage::Spent => 4,
    }
}

/// A bag index (`deathbag`, the monster's `bag`): 0 to 6.
pub const fn bag(id: BagId) -> i16 {
    id.number() as i16
}

/// A digger record index (the monster's `chase`): 0 or 1. In an
/// alternating game record `n` is player `n`'s, as in the original's
/// `digdat[curplayer + k]`.
pub const fn digger(id: DiggerId) -> i16 {
    id.number() as i16
}

/// The current player's emerald mask (`emmask`): 1 for the first player,
/// 2 for the second.
pub const fn emerald_mask(player: PlayerId) -> i16 {
    1 << player.number()
}

/// A count the original keeps in a signed 16-bit variable while the port
/// keeps it wider (lives): its low 16 bits, as the C variable would hold
/// them. Every value the game produces fits.
pub const fn low_16(value: u32) -> i16 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the original stores the value in an int16_t; the low 16 bits are its value"
    )]
    let low = value as u16;
    low.cast_signed()
}

/// A pixel coordinate (`x`, `y`) as the original's signed 16-bit value.
/// Every position in play lies within the 320×200 screen, so it fits.
pub const fn coordinate(value: i32) -> i16 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the original stores coordinates in int16_t; the low 16 bits are its value"
    )]
    let low = value as i16;
    low
}

/// An unsigned 16-bit counter as the original's signed 16-bit variable:
/// the same two bytes.
pub const fn counter(value: u16) -> i16 {
    value.cast_signed()
}

/// An 8-bit counter widened to the original's signed 16-bit variable.
pub const fn small(value: u8) -> i16 {
    value as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_codes() {
        assert_eq!(Direction::ALL.map(direction), [0, 2, 4, 6]);
        assert_eq!(optional_direction(None), -1);
        assert_eq!(NO_DIRECTION.to_le_bytes(), [0xFF, 0xFF]);
        assert_eq!(optional_direction(Some(Direction::Down)), 6);
    }

    #[test]
    fn stage_and_identifier_codes() {
        assert_eq!(death_stage(None), 0);
        assert_eq!(death_stage(Some(DeathStage::Bag)), 1);
        assert_eq!(death_stage(Some(DeathStage::Done)), 6);
        assert_eq!(
            [
                MonsterDeath::None,
                MonsterDeath::Carried,
                MonsterDeath::Ending
            ]
            .map(monster_death),
            [0, 1, 4]
        );
        assert_eq!(fireball_stage(FireballStage::Flying), 0);
        assert_eq!(
            fireball_stage(FireballStage::Exploding { next_frame: 3 }),
            3
        );
        assert_eq!(fireball_stage(FireballStage::Spent), 4);
        assert_eq!(BagId::ALL.map(bag), [0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(DiggerId::ALL.map(digger), [0, 1]);
        assert_eq!(PlayerId::ALL.map(emerald_mask), [1, 2]);
    }

    #[test]
    fn width_conversions_keep_the_low_bytes() {
        assert_eq!(low_16(3), 3);
        assert_eq!(low_16(0x1_0005), 5);
        assert_eq!(coordinate(-4), -4);
        assert_eq!(counter(0xFFFE), -2);
        assert_eq!(small(200), 200);
    }
}
