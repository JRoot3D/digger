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
use crate::sprite::{Collisions, SpriteClass};
use crate::video::{Framebuffer, ProbeBits};

use super::Ctx;
use super::digger::DeathStage;
use super::geom::Direction;
use super::ids::{BagId, DiggerId, MonsterSlot};
use super::level::Difficulty;
use super::penalty::PenaltySite;
use super::score::{ScoreEvent, ScoreId};

/// Pixels a fireball travels per tick horizontally (research 02 §0, §3.6).
const STEP_X: i32 = 8;
/// Pixels a fireball travels per tick vertically (research 02 §0, §3.6).
const STEP_Y: i32 = 7;
/// Rows the vertical earth probe reads, from the fireball's top row down
/// (research 02 §3.6).
const VERTICAL_PROBE_ROWS: i32 = 7;

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

    /// Starts (or restarts) the explosion at frame 1 (research 02 §2,
    /// `explode`).
    const fn explode(&mut self) {
        self.stage = FireballStage::Exploding { next_frame: 1 };
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

/// The position one flight step from `at` in `direction`: 8 pixels
/// horizontally, 7 vertically (digger spec, "Fireball flight").
const fn step(at: Px, direction: Direction) -> Px {
    match direction {
        Direction::Right => Px::new(at.x + STEP_X, at.y),
        Direction::Up => Px::new(at.x, at.y - STEP_Y),
        Direction::Left => Px::new(at.x - STEP_X, at.y),
        Direction::Down => Px::new(at.x, at.y + STEP_Y),
    }
}

/// The position one flight step back from `at`, where a fireball stopped
/// by earth explodes.
const fn step_back(at: Px, direction: Direction) -> Px {
    step(at, direction.reverse())
}

/// **Quirk Q19:** whether the picture shows earth for a fireball at `at`
/// travelling in `direction`, read before the fireball is redrawn there
/// (digger spec, "Earth probe for fireballs"; research 02 §3.6, research 03
/// §4.6).
///
/// Horizontal shots probe row y+4 at x and x+4, covering logical x..x+7;
/// upward shots probe x+4 on rows y..y+6 and keep the bits of its two real
/// columns; downward shots probe x on rows y..y+6 and keep the bits of the
/// left half of logical pixel x+3. Any non-black pixel is earth. The probe
/// results outside the picture are the phase-2 probe's own (quirks R1, R2;
/// compat-quirks U02): past the right or bottom edge everything is earth.
pub(crate) fn probe_earth(fb: &Framebuffer, at: Px, direction: Direction) -> bool {
    let column = |x: i32, mask: ProbeBits| {
        (0..VERTICAL_PROBE_ROWS)
            .map(|row| fb.probe(Px::new(x, at.y + row)))
            .any(|bits| bits.intersects(mask))
    };
    match direction {
        Direction::Right | Direction::Left => {
            let row = at.y + 4;
            !fb.probe(Px::new(at.x, row)).is_empty() || !fb.probe(Px::new(at.x + 4, row)).is_empty()
        }
        Direction::Up => column(at.x + 4, ProbeBits::UP),
        Direction::Down => column(at.x, ProbeBits::DOWN),
    }
}

/// Whether a fireball at `at` travelling in `direction` has left the
/// range it may fly in: x > 296 right, y < 15 up, x < 16 left, y > 183
/// down (digger spec, "What a fireball hits"; research 02 §3.6).
const fn out_of_range(at: Px, direction: Direction) -> bool {
    match direction {
        Direction::Right => at.x > 296,
        Direction::Up => at.y < 15,
        Direction::Left => at.x < 16,
        Direction::Down => at.y > 183,
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
    ///
    /// An exploding fireball draws its next explosion frame and charges the
    /// penalty; once the third frame has been drawn it is removed on the
    /// next update (digger spec, "Fireball explosion"; research 02 §3.6,
    /// `drawexplosion`).
    pub(crate) fn update_fireball(&mut self, unit: DiggerId, record: DiggerId) {
        match record.of(&self.sim.diggers).fireball.stage {
            FireballStage::Flying => self.update_fire(unit, record),
            FireballStage::Exploding { .. } => {
                // The explosion's collisions are not used.
                let _ = self.draw_fireball(unit, record);
                self.sim.penalty.charge(PenaltySite::ExplosionFrame);
            }
            FireballStage::Spent => self.remove_fireball(unit, record),
        }
    }

    /// Removes `record`'s fireball if one is in play: the digger's recharge
    /// countdown may run again, the sprite is erased, the fire cue stops
    /// once the explosion has shown a frame, and the stage returns to 0
    /// (research 02 §2 `remove`, §3.6 `killfire`). Also called for every
    /// digger at round end.
    pub(crate) fn remove_fireball(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = record.of_mut(&mut self.sim.diggers);
        if digger.not_firing {
            return;
        }
        digger.not_firing = true;
        let shown = digger.fireball.stage.number() > 1;
        digger.fireball.stage = FireballStage::Flying;
        self.gfx.erase(unit.fireball().sprite());
        if shown {
            self.sound.push(SoundCommand::FireStop(unit.fireball()));
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
            self.fly(unit, record);
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

    /// One flight step (research 02 §3.6, `updatefire`).
    ///
    /// The fireball moves, the picture is probed for earth at the new
    /// position, the fireball is drawn there and the penalty charged. Then
    /// it acts on what its draw touched: every overlapped monster is
    /// removed for 250 points to the owner, every other living,
    /// non-invincible digger is killed, and the fireball explodes on any of
    /// these and on the bonus, a bag or another fireball, every overlapped
    /// flying fireball exploding too. It also explodes when out of range;
    /// otherwise earth stops it only if the draw touched no sprite at all,
    /// the own digger included, and then it is drawn back at its previous
    /// position and explodes there (quirk Q19; digger spec, "What a
    /// fireball hits").
    fn fly(&mut self, unit: DiggerId, record: DiggerId) {
        let fireball = record.of(&self.sim.diggers).fireball;
        let direction = fireball.direction;
        let to = step(fireball.position, direction);
        // Quirk Q19: the picture is read after the move and before the
        // redraw, so the fireball's own old image can be in the probe.
        let earth = probe_earth(self.gfx.fb(), to, direction);
        record.of_mut(&mut self.sim.diggers).fireball.position = to;
        let hits = self.draw_fireball(unit, record);
        self.sim.penalty.charge(PenaltySite::FireballFlight);
        self.act_on_hits(unit, record, hits);
        let touching = !hits.is_empty();
        if out_of_range(to, direction) {
            record.of_mut(&mut self.sim.diggers).fireball.explode();
        } else if earth && !touching {
            record.of_mut(&mut self.sim.diggers).fireball.position = step_back(to, direction);
            self.draw_fireball(unit, record);
            record.of_mut(&mut self.sim.diggers).fireball.explode();
        }
    }

    /// What a flying fireball does to the sprites its draw touched, in the
    /// original's order: monsters, diggers, then the bonus, bags and
    /// fireballs (research 02 §3.6).
    fn act_on_hits(&mut self, unit: DiggerId, record: DiggerId, hits: Collisions) {
        let owner = ScoreId::credited(self.sim.turn, unit);
        for sprite in hits.of(SpriteClass::Monster) {
            if let Some(slot) = MonsterSlot::from_sprite(sprite) {
                self.remove_monster(slot);
            }
            self.add_score(owner, ScoreEvent::FireballKill);
            record.of_mut(&mut self.sim.diggers).fireball.explode();
        }
        for sprite in hits.of(SpriteClass::Digger) {
            let Some(other) = DiggerId::from_sprite(sprite) else {
                continue;
            };
            let victim = self.sim.record_of_unit(other);
            let target = victim.of(&self.sim.diggers);
            if victim != record && !target.invincible && target.alive {
                // Shot down: the death of a digger killed by a monster.
                self.sim
                    .kill_digger(victim, DeathStage::Monster, BagId::ALL[0]);
                record.of_mut(&mut self.sim.diggers).fireball.explode();
            }
        }
        let obstacle = [SpriteClass::Bonus, SpriteClass::Bag, SpriteClass::Fireball]
            .into_iter()
            .any(|class| hits.first(class).is_some());
        if obstacle {
            record.of_mut(&mut self.sim.diggers).fireball.explode();
            for sprite in hits.of(SpriteClass::Fireball) {
                let Some(other) = DiggerId::ALL
                    .into_iter()
                    .find(|unit| unit.fireball().sprite() == sprite)
                else {
                    continue;
                };
                let other = self.sim.record_of_unit(other);
                let fireball = &mut other.of_mut(&mut self.sim.diggers).fireball;
                if fireball.stage == FireballStage::Flying {
                    fireball.explode();
                }
            }
        }
    }

    /// Draws `record`'s fireball through `unit`'s fireball sprite and
    /// returns the collisions of the draw (research 02 §2, `animate`).
    ///
    /// A flying fireball shows the next flying frame. An exploding one
    /// shows its next explosion frame, sounds the explode cue with frame 1
    /// and moves on to the following frame.
    fn draw_fireball(&mut self, unit: DiggerId, record: DiggerId) -> Collisions {
        let fireball = record.of(&self.sim.diggers).fireball;
        let (image, next) = match fireball.stage {
            FireballStage::Flying => (
                unit.of_mut(&mut self.sim.fireball_frames).advance(),
                FireballStage::Flying,
            ),
            FireballStage::Exploding { next_frame } => {
                let image = match next_frame {
                    1 => ImageId::EXPLOSION_1,
                    2 => ImageId::EXPLOSION_2,
                    _ => ImageId::EXPLOSION_3,
                };
                let next = if next_frame >= 3 {
                    FireballStage::Spent
                } else {
                    FireballStage::Exploding {
                        next_frame: next_frame + 1,
                    }
                };
                (image, next)
            }
            FireballStage::Spent => {
                debug_assert!(false, "a spent fireball is removed, not drawn");
                return Collisions::EMPTY;
            }
        };
        let hits =
            self.gfx
                .move_fireball(unit.fireball(), for_unit(image, unit), fireball.position);
        if fireball.stage == (FireballStage::Exploding { next_frame: 1 }) {
            self.sound.push(SoundCommand::Explode(unit.fireball()));
        }
        record.of_mut(&mut self.sim.diggers).fireball.stage = next;
        hits
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{Controls, FrameInput, Game, GameConfig, LevelNumber, Phase};

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
    fn probe_beyond_right_or_bottom_is_earth() {
        let fb = Framebuffer::new();
        let y = 100;
        // A black picture is earth only where the probe leaves it.
        for direction in [
            Direction::Right,
            Direction::Up,
            Direction::Left,
            Direction::Down,
        ] {
            assert!(!probe_earth(&fb, Px::new(100, y), direction));
        }
        // Right: the second probe at x+4 = 320 lies past the right edge.
        assert!(!probe_earth(&fb, Px::new(312, y), Direction::Right));
        assert!(probe_earth(&fb, Px::new(316, y), Direction::Right));
        assert!(probe_earth(&fb, Px::new(320, y), Direction::Left));
        // Up reads column x+4, down column x.
        assert!(probe_earth(&fb, Px::new(316, y), Direction::Up));
        assert!(!probe_earth(&fb, Px::new(316, y), Direction::Down));
        assert!(probe_earth(&fb, Px::new(320, y), Direction::Down));
        // Bottom: rows y..y+6 for vertical shots, row y+4 for horizontal.
        assert!(!probe_earth(&fb, Px::new(100, 193), Direction::Down));
        assert!(probe_earth(&fb, Px::new(100, 194), Direction::Down));
        assert!(probe_earth(&fb, Px::new(100, 194), Direction::Up));
        assert!(!probe_earth(&fb, Px::new(100, 195), Direction::Right));
        assert!(probe_earth(&fb, Px::new(100, 196), Direction::Right));
    }

    #[test]
    fn probe_masks_per_direction() {
        // A single non-black logical pixel at (103, 50): real columns 206
        // and 207 on real rows 100 and 101.
        let mut fb = Framebuffer::new();
        fb.put_opaque(Px::new(103, 50), 2, &[1; 4]);
        // Horizontal at row y+4 = 50 covering x 100..107: earth.
        assert!(probe_earth(&fb, Px::new(100, 46), Direction::Right));
        // Down reads the left half of x+3: earth for x = 100 on rows
        // 44..50.
        assert!(probe_earth(&fb, Px::new(100, 44), Direction::Down));
        assert!(!probe_earth(&fb, Px::new(100, 51), Direction::Down));
        // Up reads only x+4: logical 103 is not seen from x = 100, but is
        // from x = 99.
        assert!(!probe_earth(&fb, Px::new(100, 44), Direction::Up));
        assert!(probe_earth(&fb, Px::new(99, 44), Direction::Up));
    }

    #[test]
    fn recharge_table_63_to_90() {
        let ticks: Vec<u16> = (1..=11)
            .map(|level| {
                let level = LevelNumber::new(level).unwrap_or_else(|| unreachable!());
                recharge_ticks(level.difficulty())
            })
            .collect();
        assert_eq!(ticks, [63, 66, 69, 72, 75, 78, 81, 84, 87, 90, 90]);
    }

    #[test]
    fn explode_restarts_an_explosion() {
        let mut fireball = Fireball::launched(Px::new(100, 100), Direction::Left);
        fireball.explode();
        assert_eq!(fireball.stage, FireballStage::Exploding { next_frame: 1 });
        fireball.stage = FireballStage::Exploding { next_frame: 3 };
        fireball.explode();
        assert_eq!(fireball.stage, FireballStage::Exploding { next_frame: 1 });
        assert_eq!(
            [
                FireballStage::Flying,
                FireballStage::Exploding { next_frame: 2 },
                FireballStage::Spent
            ]
            .map(FireballStage::number),
            [0, 2, 4]
        );
    }

    #[test]
    fn flying_fireball_removed_at_round_end() {
        let mut game = Game::new(GameConfig::default(), 0);
        while game.phase != Phase::Playing {
            game.step(&FrameInput::NONE);
        }
        game.step(&fire());
        game.step(&fire());
        let first = DiggerId::ALL[0];
        assert!(!first.of(&game.sim.diggers).not_firing);
        let _ = game.drain_sound().count();
        // A quit ends the game through the round end.
        game.step(&FrameInput {
            quit: true,
            ..FrameInput::NONE
        });
        let digger = first.of(&game.sim.diggers);
        assert!(digger.not_firing);
        assert_eq!(digger.fireball.stage, FireballStage::Flying);
        assert!(!game.gfx.sprites().is_enabled(first.fireball().sprite()));
        // A flying fireball has no fire cue left to stop.
        let sounds: Vec<SoundCommand> = game.drain_sound().collect();
        assert_eq!(sounds, [SoundCommand::StopAll]);
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

    #[test]
    fn probe_ignored_while_touching_a_tombstone() {
        use crate::sim::{DeathStage, GameMode, LevelLayout, LevelSet};
        // Two diggers on earth with only the first digger's start cell
        // open. The second digger lies dead, its tombstone at (168,180) in
        // the way of the first digger's shot to the right.
        let mut rows = ["               "; 10];
        rows[9] = "        S      ";
        let layout = LevelLayout::parse_rows(rows).unwrap_or_else(|_| unreachable!());
        let config = GameConfig {
            mode: GameMode::TwoDiggers,
            levels: LevelSet::from_plans([layout; 8], 20_000, true),
            ..GameConfig::default()
        };
        let mut game = Game::new(config, 0);
        while game.phase != Phase::Playing {
            game.step(&FrameInput::NONE);
        }
        let [first, second] = DiggerId::ALL;
        {
            let dead = second.of_mut(&mut game.sim.diggers);
            dead.alive = false;
            dead.death = Some(DeathStage::Tombstone);
        }
        let tombstone = ImageId::new(30).unwrap_or_else(|| unreachable!());
        game.gfx
            .place(second.sprite(), tombstone, Px::new(188, 180));
        // Digger 1 at (172,180) fires right: launched at (180,184) on the
        // second tick. At x = 188 and 196 the probe reads the earth of
        // column 9 but the shot overlaps the tombstone; at 204 it touches
        // nothing and is drawn back at 196.
        let mut path = Vec::new();
        for _ in 0..5 {
            game.step(&fire());
            let fireball = first.of(&game.sim.diggers).fireball;
            path.push((fireball.position.x, fireball.stage));
        }
        let flying = FireballStage::Flying;
        let exploding = FireballStage::Exploding { next_frame: 1 };
        assert_eq!(
            path.get(1..).unwrap_or_default(),
            [
                (180, flying),
                (188, flying),
                (196, flying),
                (196, exploding)
            ]
        );
        assert!(!second.of(&game.sim.diggers).alive, "the dead stay dead");

        // Without the tombstone the same shot stops at once.
        game.gfx.erase(second.sprite());
        {
            let digger = first.of_mut(&mut game.sim.diggers);
            digger.not_firing = true;
            digger.recharge_time = 0;
            digger.fireball = Fireball::ZEROED;
        }
        game.gfx.erase(first.fireball().sprite());
        let mut path = Vec::new();
        for _ in 0..2 {
            game.step(&fire());
            let fireball = first.of(&game.sim.diggers).fireball;
            path.push((fireball.position.x, fireball.stage));
        }
        assert_eq!(path, [(180, flying), (180, exploding)]);
    }

    #[test]
    fn exploding_fireball_hit_again_is_not_restarted() {
        use crate::sim::GameMode;
        let config = GameConfig {
            mode: GameMode::TwoDiggers,
            ..GameConfig::default()
        };
        let mut game = Game::new(config, 0);
        while game.phase != Phase::Playing {
            game.step(&FrameInput::NONE);
        }
        let [first, second] = DiggerId::ALL;
        // Digger 1's fireball flies right at (188,184); digger 2's is
        // exploding at (196,184), its second frame next.
        let set_up = |game: &mut Game, unit: DiggerId, stage, x, image| {
            let digger = unit.of_mut(&mut game.sim.diggers);
            digger.not_firing = false;
            digger.fireball = Fireball {
                stage,
                direction: Direction::Right,
                position: Px::new(x, 184),
            };
            game.gfx
                .place(unit.fireball().sprite(), image, Px::new(x, 184));
        };
        set_up(
            &mut game,
            first,
            FireballStage::Flying,
            188,
            ImageId::FIREBALL_1,
        );
        let second_image = for_unit(ImageId::EXPLOSION_1, second);
        let stage = FireballStage::Exploding { next_frame: 2 };
        set_up(&mut game, second, stage, 196, second_image);
        game.step(&FrameInput::NONE);
        let flying = first.of(&game.sim.diggers).fireball;
        assert_eq!(flying.position, Px::new(196, 184));
        assert_eq!(flying.stage, FireballStage::Exploding { next_frame: 1 });
        // The hit fireball drew its second frame, as without the hit.
        let hit = second.of(&game.sim.diggers).fireball;
        assert_eq!(hit.stage, FireballStage::Exploding { next_frame: 3 });
        assert_eq!(
            game.gfx.sprites().image(second.fireball().sprite()),
            for_unit(ImageId::EXPLOSION_2, second)
        );
    }
}
