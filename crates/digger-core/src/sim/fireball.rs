//! Fireballs: launch, flight, the earth probe and the explosion (design D1).
//!
//! Each digger record owns at most one fireball, kept in the record beside
//! the record's `not_firing` flag and recharge countdown, as the original
//! keeps it in `digdat[].bob` (research 02 §3.1, §3.6). The fireball is
//! drawn through the sprite of the *unit* playing the record, so in an
//! alternating game the second player's fireball uses the first fireball
//! sprite and the first fireball's sound commands.
//!
//! Per tick and digger, before the digger moves, either the explosion
//! advances (when the fireball is exploding or spent) or [`Ctx::update_fire`]
//! runs: the recharge countdown and the launch while no fireball is in
//! play, otherwise one flight step (digger spec, "Launching a fireball",
//! "Fireball flight"; research 02 §3.3, §3.6).

use crate::assets::ImageId;
use crate::geom::Px;
use crate::sound_cmd::SoundCommand;

use super::Ctx;
use super::geom::Direction;
use super::ids::DiggerId;
use super::level::Difficulty;

/// Where a fireball is in its life, with the original's numbering of
/// `bob.expsn` (research 02 §2, `bullet_obj`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FireballStage {
    /// Flying (0). Also the value while no fireball is in play, which the
    /// record's `not_firing` flag tells apart.
    Flying,
    /// Exploding: the explosion frame to draw next, 1..=3.
    Exploding {
        /// The next explosion frame, 1..=3.
        next_frame: u8,
    },
    /// The last explosion frame has been drawn; the fireball is removed on
    /// its next update (4).
    Spent,
}

impl FireballStage {
    /// The original's number of the stage, 0 to 4.
    pub const fn number(self) -> u8 {
        match self {
            Self::Flying => 0,
            Self::Exploding { next_frame } => next_frame,
            Self::Spent => 4,
        }
    }
}

/// A digger record's fireball (`bob`): stage, direction and position.
///
/// Every field is kept while no fireball is in play, stale values included
/// (design D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Fireball {
    /// The stage (`expsn`).
    pub(crate) stage: FireballStage,
    /// The direction of travel (`dir`).
    pub(crate) direction: Direction,
    /// The position of the 8×8 sprite (`x`, `y`).
    pub(crate) position: Px,
}

impl Fireball {
    /// The fireball as zeroed at every round start: flying stage, direction
    /// right (0) and position (0, 0) (research 02 §3.2, `initdigger`).
    pub(crate) const ZEROED: Self = Self {
        stage: FireballStage::Flying,
        direction: Direction::Right,
        position: Px::new(0, 0),
    };

    /// The fireball launched by a digger at `digger` facing `facing`:
    /// right at (x+8, y+4), up at (x+4, y), left at (x, y+4), down at
    /// (x+4, y+8) (digger spec, "Launching a fireball").
    pub(crate) const fn launched(digger: Px, facing: Direction) -> Self {
        let (dx, dy) = match facing {
            Direction::Right => (8, 4),
            Direction::Up => (4, 0),
            Direction::Left => (0, 4),
            Direction::Down => (4, 8),
        };
        Self {
            stage: FireballStage::Flying,
            direction: facing,
            position: Px::new(digger.x + dx, digger.y + dy),
        }
    }

    /// The stage (hashed as `expsn`).
    pub const fn stage(&self) -> FireballStage {
        self.stage
    }

    /// The direction of travel (hashed as `bob.dir`).
    pub const fn direction(&self) -> Direction {
        self.direction
    }

    /// The position (hashed as `bob.x`, `bob.y`).
    pub const fn position(&self) -> Px {
        self.position
    }
}

/// Ticks a digger waits after its fireball has been removed before it can
/// fire again: 3 × difficulty + 60, 63..=90 (digger spec, "Recharge time by
/// level"; research 02 §3.6, §7).
pub(crate) const fn recharge_ticks(difficulty: Difficulty) -> u16 {
    // The difficulty is at most 10, so the result fits easily.
    difficulty.get() as u16 * 3 + 60
}

/// The animation counter of a flying fireball sprite, cycling 1, 2, 0, 1, …, advanced on every flying-frame draw and reset with the sprites
/// (research 02 §2, `drawfire`; not hashed).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct FlameFrame(u8);

impl FlameFrame {
    /// The counter as reset with the sprites.
    pub(crate) const START: Self = Self(0);

    /// Advances the counter and returns the flying image of the first
    /// fireball for the new frame.
    #[allow(dead_code, reason = "drawn by the flight of task 9.2")]
    pub(crate) const fn advance(&mut self) -> ImageId {
        self.0 = if self.0 >= 2 { 0 } else { self.0 + 1 };
        match self.0 {
            0 => ImageId::FIREBALL_1,
            1 => ImageId::FIREBALL_2,
            _ => ImageId::FIREBALL_3,
        }
    }
}

/// The image `image` of the first fireball as drawn by `unit`.
const fn for_unit(image: ImageId, unit: DiggerId) -> ImageId {
    if unit.number() == 0 {
        image
    } else {
        match image.for_second_digger() {
            Some(image) => image,
            None => image,
        }
    }
}

impl Ctx<'_> {
    /// Resets both fireball sprites to the first flying image, disabled,
    /// and their animation counters, as when the sprites are set up for a
    /// round (research 02 §2, `initdbfspr`).
    pub(crate) fn reset_fireball_sprites(&mut self) {
        for unit in DiggerId::ALL {
            let fireball = unit.fireball();
            self.gfx
                .reset(fireball.sprite(), for_unit(ImageId::FIREBALL_1, unit));
            *unit.of_mut(&mut self.sim.fireball_frames) = FlameFrame::START;
        }
    }

    /// The fireball part of a digger's update: the explosion while one is
    /// under way, otherwise [`update_fire`](Self::update_fire) (research 02
    /// §3.3, `dodigger`).
    pub(crate) fn update_fireball(&mut self, unit: DiggerId, record: DiggerId) {
        match record.of(&self.sim.diggers).fireball.stage {
            FireballStage::Flying => self.update_fire(unit, record),
            FireballStage::Exploding { .. } | FireballStage::Spent => {}
        }
    }

    /// The recharge countdown and the launch while no fireball is in play
    /// (research 02 §3.6, `updatefire`).
    ///
    /// The countdown runs one per tick and makes the digger ready on the
    /// tick it reaches 0. Otherwise, with fire active for the unit's control
    /// slot and the digger alive, a fireball is launched in the digger's
    /// facing: the digger loses its fire-ready look, the recharge countdown
    /// is set for after the fireball, the sprite is placed without a
    /// collision test, and the fire cue sounds. With no facing the request
    /// is ignored and the fireball stays ready (compat-quirks U01).
    fn update_fire(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = record.of(&self.sim.diggers);
        if !digger.not_firing {
            // Flight is added by task 9.2.
            return;
        }
        if digger.recharge_time != 0 {
            let digger = record.of_mut(&mut self.sim.diggers);
            digger.recharge_time -= 1;
            if digger.recharge_time == 0 {
                digger.fire_ready = true;
            }
            return;
        }
        if !unit.slot().of(&self.sim.inputs).fire_active() || !digger.alive {
            return;
        }
        // Compat-quirks U01: the original aborts here; the request is
        // ignored without using up the fireball.
        let Some(facing) = digger.facing else {
            return;
        };
        let recharge = recharge_ticks(self.sim.current_level().difficulty());
        let digger = record.of_mut(&mut self.sim.diggers);
        digger.fire_ready = false;
        digger.recharge_time = recharge;
        digger.not_firing = false;
        digger.fireball = Fireball::launched(digger.position, facing);
        let at = digger.fireball.position;
        let sprite = unit.fireball().sprite();
        // The sprite shows whatever image it was last given: the first
        // flying image after a round start, otherwise the last explosion
        // frame of the previous fireball (purely visual; research 02 §2).
        let image = self.gfx.sprites().pending_image(sprite);
        self.gfx.place(sprite, image, at);
        self.sound.push(SoundCommand::Fire(unit.fireball()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Controls, FrameInput, Game, GameConfig, Phase};

    #[test]
    fn launch_offsets_per_facing() {
        let at = Px::new(100, 60);
        let offsets = [
            Direction::Right,
            Direction::Up,
            Direction::Left,
            Direction::Down,
        ]
        .map(|facing| Fireball::launched(at, facing).position);
        assert_eq!(
            offsets,
            [
                Px::new(108, 64),
                Px::new(104, 60),
                Px::new(100, 64),
                Px::new(104, 68)
            ]
        );
    }

    #[test]
    fn flame_frames_cycle_from_the_second_image() {
        let mut frame = FlameFrame::START;
        let images: Vec<u8> = (0..4).map(|_| frame.advance().number()).collect();
        assert_eq!(images, [83, 84, 82, 83]);
    }

    fn fire() -> FrameInput {
        FrameInput {
            slots: [
                Controls {
                    fire: true,
                    ..Controls::NONE
                },
                Controls::NONE,
            ],
            ..FrameInput::NONE
        }
    }

    #[test]
    fn fire_with_no_facing_is_ignored() {
        let mut game = Game::new(GameConfig::default(), 0);
        while game.phase != Phase::Playing {
            game.step(&FrameInput::NONE);
        }
        let first = DiggerId::ALL[0];
        first.of_mut(&mut game.sim.diggers).facing = None;
        let _ = game.drain_sound().count();
        for _ in 0..5 {
            game.step(&fire());
        }
        let digger = first.of(&game.sim.diggers);
        assert!(digger.not_firing(), "no fireball in play");
        assert!(digger.fire_ready(), "the fireball stays ready");
        assert_eq!(digger.recharge_time(), 0);
        assert!(!game.gfx.sprites().is_enabled(first.fireball().sprite()));
        assert_eq!(game.drain_sound().count(), 0, "no fire cue");
        assert!(game.summary().is_none(), "the game goes on");

        // A facing recovered lets the same held fire launch at once.
        first.of_mut(&mut game.sim.diggers).facing = Some(Direction::Right);
        game.step(&fire());
        assert!(!first.of(&game.sim.diggers).not_firing());
    }
}
