//! The digger: movement, digging, emeralds, death, respawn and lives
//! (design D1).
//!
//! The game keeps two digger records, indexed by [`DiggerId`]: in a
//! two-digger game one per digger, in an alternating game one per player
//! (the record of the player whose turn it is plays), otherwise only the
//! first is in play (research 01 §2.1, §3.1).
//!
//! A record in play is driven by a *unit*: the sprite, the control slot and
//! the image set it uses. Unit `i` plays record `turn + i`, so in an
//! alternating game the second player's record uses the first digger's
//! sprite, images and control slot (research 02 §1, §3.2).
//!
//! The record keeps every variable of the upstream state hash at the
//! original's granularity, stale values included (design D5).

use crate::assets::ImageId;
use crate::geom::Px;
use crate::sound_cmd::{EmeraldStep, SoundCommand};
use crate::sprite::Collisions;

use super::draw::{self, Carve};
use super::field::EmeraldMap;
use super::geom::{CellPos, Direction, Located, MAX_X, MAX_Y, MIN_X, MIN_Y, locate};
use super::gfx::Gfx;
use super::ids::{BagId, DiggerId, PlayerId};
use super::input::FrameInput;
use super::penalty::{Penalty, PenaltySite};
use super::score::{ScoreEvent, ScoreId};
use super::{Ctx, Sim};

/// Lives every digger in play starts a game with (game-flow spec, "Game
/// start").
pub(crate) const START_LIVES: u32 = 3;

/// The row every digger starts a round on (research 02 §3.2).
const START_ROW: u8 = 9;
/// The start column in a game with one digger (research 02 §3.2).
const SOLO_START_COLUMN: u8 = 7;
/// The start column of digger record 0 with two diggers; record `n` starts
/// two columns further left per index (research 02 §3.2).
const PAIR_START_COLUMN: u8 = 8;

/// Pixels a moving digger advances per tick horizontally (research 02 §0).
const STEP_X: i32 = 4;
/// Pixels a moving digger advances per tick vertically (research 02 §0).
const STEP_Y: i32 = 3;

/// The length of the emerald-run window: restarted at 9 by every emerald
/// eaten and counted down at the end of every tick, so the next emerald
/// continues the run when eaten within 8 ticks (research 02 §3.5).
const EMERALD_WINDOW: u8 = 9;
/// Emeralds in a full run; the last of them scores the run bonus
/// (research 02 §3.5).
const EMERALD_RUN: u8 = 8;

/// The remainders along the direction of travel at which an emerald in the
/// cell being entered is redrawn and eaten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmeraldSteps {
    /// Remainder at which the emerald is redrawn.
    redraw: u8,
    /// Remainder at which the emerald is eaten.
    eat: u8,
}

/// **Quirk Q16:** the emerald thresholds per direction of travel, in the
/// order right, up, left, down: redrawn at remainder 8, 12, 16 or 6 and
/// eaten at 12, 9, 12 or 9 (digger spec, "Eating emeralds"; research 02
/// §3.5, `embox`).
const EMERALD_STEPS: [EmeraldSteps; 4] = [
    EmeraldSteps { redraw: 8, eat: 12 },
    EmeraldSteps { redraw: 12, eat: 9 },
    EmeraldSteps {
        redraw: 16,
        eat: 12,
    },
    EmeraldSteps { redraw: 6, eat: 9 },
];

/// The thresholds for travel in `direction`.
const fn emerald_steps(direction: Direction) -> EmeraldSteps {
    match direction {
        Direction::Right => EMERALD_STEPS[0],
        Direction::Up => EMERALD_STEPS[1],
        Direction::Left => EMERALD_STEPS[2],
        Direction::Down => EMERALD_STEPS[3],
    }
}

/// The stage of a digger's death, with the original's numbering (research
/// 02 §3.7, design D5).
///
/// [`DeathStage::Bag`] is also the value while the digger is alive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeathStage {
    /// Killed by a falling bag and carried with it; also the value of a
    /// living digger (1).
    Bag,
    /// The grave sequence: waiting, then the growing tombstone (2).
    Grave,
    /// Killed by a monster or a fireball: the undrawn tick (3).
    Monster,
    /// The resting tombstone (4).
    Tombstone,
    /// The death arc (5).
    Arc,
    /// Out of play: no lives left (6).
    Done,
}

impl DeathStage {
    /// The original's number of the stage, 1 to 6.
    pub const fn number(self) -> u8 {
        match self {
            Self::Bag => 1,
            Self::Grave => 2,
            Self::Monster => 3,
            Self::Tombstone => 4,
            Self::Arc => 5,
            Self::Done => 6,
        }
    }
}

/// The ping-pong animation counter of a digger sprite: 0, 1, 2, 1, 0, …,
/// advanced on every draw call (research 02 §2, `drawdigger`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AnimFrame {
    /// The frame, 0..=2.
    frame: u8,
    /// Whether the frame is counting up.
    rising: bool,
}

impl AnimFrame {
    /// The counter as reset with the sprites: frame 0, counting up.
    pub(crate) const START: Self = Self {
        frame: 0,
        rising: true,
    };

    /// Advances the counter and returns the new frame.
    pub(crate) const fn advance(&mut self) -> u8 {
        self.frame = if self.rising {
            self.frame.saturating_add(1)
        } else {
            self.frame.saturating_sub(1)
        };
        if self.frame >= 2 {
            self.frame = 2;
            self.rising = false;
        } else if self.frame == 0 {
            self.rising = true;
        }
        self.frame
    }
}

/// One digger record.
///
/// Every field the upstream state hash covers is kept as its own value,
/// including those that go stale (design D5). The original's
/// `firepressed` and `levdone` digger fields are never set and always
/// hash as false, so they are not stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags of the state hash, kept at the original's granularity (design D5)"
)]
pub struct Digger {
    /// Lives left, the one being played included.
    pub(crate) lives: u32,
    /// The position (`dob.x`, `dob.y`).
    pub(crate) position: Px,
    /// The facing (`dob.dir`); `None` when a stationary overlap with a bag
    /// left the digger without one (digger spec, "Digger left without a
    /// facing").
    pub(crate) facing: Option<Direction>,
    /// The direction moved on the last update (`mdir`), or none.
    pub(crate) moving: Option<Direction>,
    /// The cell and remainders as stored at the end of the last update
    /// (`h`, `v`, `rx`, `ry`); the turning rules read these.
    pub(crate) located: Located,
    /// Ticks still to pause after a push (`bagtime`).
    pub(crate) bag_time: u16,
    /// The fireball recharge countdown (`rechargetime`).
    pub(crate) recharge_time: u16,
    /// Whether no fireball of this digger is flying or exploding
    /// (`notfiring`).
    pub(crate) not_firing: bool,
    /// Whether the digger shows its fire-ready look (`dob.can_fire`; not
    /// hashed).
    pub(crate) fire_ready: bool,
    /// The death stage (`deathstage`); `None` for a record that was never
    /// placed, which the original holds as 0.
    pub(crate) death: Option<DeathStage>,
    /// The bag that killed the digger (`deathbag`).
    pub(crate) death_bag: BagId,
    /// The frame counter of the death sequence (`deathani`).
    pub(crate) death_frame: u8,
    /// The timer of the death sequence (`deathtime`).
    pub(crate) death_timer: u16,
    /// The emerald-run window (`emocttime`).
    pub(crate) emerald_window: u8,
    /// Emeralds eaten in the current run, 0..=7 (`emn`).
    pub(crate) emerald_run: u8,
    /// The monster-eating multiplier (`msc`).
    pub(crate) eat_multiplier: i16,
    /// Whether the digger is invincible (`invin`).
    pub(crate) invincible: bool,
    /// Draws of invincibility left (`ivt`).
    pub(crate) invincibility: u16,
    /// Whether the death sequence has finished (`dead`).
    pub(crate) dead: bool,
    /// Whether the digger is alive and controllable (`dob.alive`).
    pub(crate) alive: bool,
}

impl Default for Digger {
    fn default() -> Self {
        Self::NEVER_PLACED
    }
}

impl Digger {
    /// A record before its first round: the original's zero-initialised
    /// values, in which direction 0 is right (research 02 §0, §3.1).
    pub(crate) const NEVER_PLACED: Self = Self {
        lives: 0,
        position: Px::new(0, 0),
        facing: Some(Direction::Right),
        moving: Some(Direction::Right),
        located: Located {
            cell: match CellPos::new(0, 0) {
                Some(cell) => cell,
                None => unreachable!(),
            },
            rem_x: 0,
            rem_y: 0,
        },
        bag_time: 0,
        recharge_time: 0,
        not_firing: false,
        fire_ready: false,
        death: None,
        death_bag: BagId::ALL[0],
        death_frame: 0,
        death_timer: 0,
        emerald_window: 0,
        emerald_run: 0,
        eat_multiplier: 0,
        invincible: false,
        invincibility: 0,
        dead: false,
        alive: false,
    };

    /// Lives left, the one being played included (hashed as `lives`).
    pub const fn lives(&self) -> u32 {
        self.lives
    }

    /// The position (hashed as `dob.x`, `dob.y`).
    pub const fn position(&self) -> Px {
        self.position
    }

    /// The facing (hashed as `dob.dir`), or `None` when the digger has
    /// none.
    pub const fn facing(&self) -> Option<Direction> {
        self.facing
    }

    /// The direction moved on the last update (hashed as `mdir`), or none.
    pub const fn moving(&self) -> Option<Direction> {
        self.moving
    }

    /// The cell and remainders as stored at the end of the last update
    /// (hashed as `h`, `v`, `rx`, `ry`).
    pub const fn located(&self) -> Located {
        self.located
    }

    /// Ticks still to pause after a push (hashed as `bagtime`).
    pub const fn bag_time(&self) -> u16 {
        self.bag_time
    }

    /// The fireball recharge countdown (hashed as `rechargetime`).
    pub const fn recharge_time(&self) -> u16 {
        self.recharge_time
    }

    /// Whether no fireball of this digger is in play (hashed as
    /// `notfiring`).
    pub const fn not_firing(&self) -> bool {
        self.not_firing
    }

    /// Whether the digger shows its fire-ready look.
    pub const fn fire_ready(&self) -> bool {
        self.fire_ready
    }

    /// The death stage (hashed as `deathstage`); `None` for a record never
    /// placed (hashed as 0).
    pub const fn death_stage(&self) -> Option<DeathStage> {
        self.death
    }

    /// The bag that killed the digger (hashed as `deathbag`).
    pub const fn death_bag(&self) -> BagId {
        self.death_bag
    }

    /// The death-sequence frame counter (hashed as `deathani`).
    pub const fn death_frame(&self) -> u8 {
        self.death_frame
    }

    /// The death-sequence timer (hashed as `deathtime`).
    pub const fn death_timer(&self) -> u16 {
        self.death_timer
    }

    /// The emerald-run window (hashed as `emocttime`).
    pub const fn emerald_window(&self) -> u8 {
        self.emerald_window
    }

    /// Emeralds eaten in the current run (hashed as `emn`).
    pub const fn emerald_run(&self) -> u8 {
        self.emerald_run
    }

    /// The monster-eating multiplier (hashed as `msc`).
    pub const fn eat_multiplier(&self) -> i16 {
        self.eat_multiplier
    }

    /// Whether the digger is invincible (hashed as `invin`).
    pub const fn invincible(&self) -> bool {
        self.invincible
    }

    /// Draws of invincibility left (hashed as `ivt`).
    pub const fn invincibility(&self) -> u16 {
        self.invincibility
    }

    /// Whether the death sequence has finished (hashed as `dead`).
    pub const fn dead(&self) -> bool {
        self.dead
    }

    /// Whether the digger is alive and controllable (hashed as
    /// `dob.alive`).
    pub const fn alive(&self) -> bool {
        self.alive
    }

    /// Puts the record at the start of a round in cell (`column`, row 9)
    /// facing `facing`, with no push pause, a ready fireball, no emerald
    /// run and an eat multiplier of 1 (digger spec, "Start position and
    /// facing"; research 02 §3.2, `initdigger`). The moving direction is
    /// left, as in the original.
    fn start_round(&mut self, column: u8, facing: Direction) {
        let cell = CellPos::new(column, START_ROW).unwrap_or_else(|| {
            debug_assert!(false, "start columns lie inside the grid");
            CellPos::new(0, START_ROW).unwrap_or(self.located.cell)
        });
        *self = Self {
            lives: self.lives,
            position: cell.to_px(),
            facing: Some(facing),
            moving: Some(Direction::Left),
            located: Located {
                cell,
                rem_x: 0,
                rem_y: 0,
            },
            bag_time: 0,
            recharge_time: 0,
            not_firing: true,
            fire_ready: true,
            death: Some(DeathStage::Bag),
            // The death counters keep their stale values (design D5).
            death_bag: self.death_bag,
            death_frame: self.death_frame,
            death_timer: self.death_timer,
            emerald_window: 0,
            emerald_run: 0,
            eat_multiplier: 1,
            invincible: false,
            invincibility: 0,
            dead: false,
            alive: true,
        };
    }

    /// Takes a record with no lives left out of play at round start
    /// (research 02 §3.2).
    fn retire(&mut self) {
        self.dead = true;
        self.invincible = false;
        self.invincibility = 0;
        self.death = Some(DeathStage::Done);
        self.alive = false;
        self.bag_time = 0;
        self.not_firing = true;
        self.recharge_time = 0;
        self.emerald_window = 0;
    }

    /// Stores the cell and remainders of the current position.
    fn relocate(&mut self) {
        if let Some(located) = locate(self.position) {
            self.located = located;
        } else {
            debug_assert!(false, "a digger stays within the playfield");
        }
    }

    /// Whether the digger takes a movement sample this tick: alive and not
    /// in a push pause (the visibility of all diggers is checked by the
    /// caller).
    const fn can_move(&self) -> bool {
        self.alive && self.bag_time == 0
    }
}

/// The direction a digger moves in this tick under the turning rules, and
/// its facing afterwards (digger spec, "Turning rules"; research 02 §3.4).
///
/// An up or down input is adopted as the facing only with a horizontal
/// remainder of 0, a left or right input only with a vertical remainder of
/// 0. With any input the digger moves along its (possibly new) facing;
/// with none it stands still.
fn turn(
    facing: Option<Direction>,
    located: Located,
    input: Option<Direction>,
) -> (Option<Direction>, Option<Direction>) {
    let Some(wanted) = input else {
        return (None, facing);
    };
    let aligned = if wanted.is_vertical() {
        located.rem_x == 0
    } else {
        located.rem_y == 0
    };
    let facing = if aligned { Some(wanted) } else { facing };
    (facing, facing)
}

/// Whether a digger at `at` may not move in `direction` because it is at
/// that edge of the playfield (digger spec, "Step size and playfield
/// limits").
const fn at_limit(at: Px, direction: Direction) -> bool {
    match direction {
        Direction::Right => at.x == MAX_X,
        Direction::Left => at.x == MIN_X,
        Direction::Down => at.y == MAX_Y,
        Direction::Up => at.y == MIN_Y,
    }
}

/// The position one step from `at` in `direction`: 4 pixels horizontally,
/// 3 vertically.
const fn step(at: Px, direction: Direction) -> Px {
    match direction {
        Direction::Right => Px::new(at.x + STEP_X, at.y),
        Direction::Up => Px::new(at.x, at.y - STEP_Y),
        Direction::Left => Px::new(at.x - STEP_X, at.y),
        Direction::Down => Px::new(at.x, at.y + STEP_Y),
    }
}

/// The image of a digger drawn by unit `unit` facing `facing` with
/// animation frame `frame`, or `None` without a facing (research 02 §2,
/// `drawdigger`): the walking frames in the order right, up, left, down,
/// each with the fire-ready look and without it.
fn walking_image(
    unit: DiggerId,
    facing: Option<Direction>,
    fire_ready: bool,
    frame: u8,
) -> Option<ImageId> {
    let turn_index: u8 = match facing? {
        Direction::Right => 0,
        Direction::Up => 1,
        Direction::Left => 2,
        Direction::Down => 3,
    };
    let look = turn_index * 2 + u8::from(!fire_ready);
    let image = ImageId::new(look * 3 + frame + 1)?;
    if unit.number() == 0 {
        Some(image)
    } else {
        image.for_second_digger()
    }
}

/// Removes the emerald a mover entering `cell` along `direction` has
/// reached, and redraws it on the way (quirk Q16; research 02 §3.5,
/// `hitemerald`). Returns whether it was eaten.
fn hit_emerald(
    gfx: &mut Gfx,
    emeralds: &mut EmeraldMap,
    penalty: &mut Penalty,
    player: PlayerId,
    at: Px,
    direction: Direction,
) -> bool {
    let Some(located) = locate(at) else {
        return false;
    };
    // Right and down moves with a remainder test the cell ahead; left and
    // up moves are already in the cell being entered.
    let cell = match direction {
        Direction::Right if located.rem_x != 0 => located.cell.neighbour(Direction::Right),
        Direction::Down if located.rem_y != 0 => located.cell.neighbour(Direction::Down),
        _ => Some(located.cell),
    };
    let Some(cell) = cell else {
        return false;
    };
    if !emeralds.has(player, cell) {
        return false;
    }
    let remainder = if direction.is_horizontal() {
        located.rem_x
    } else {
        located.rem_y
    };
    let steps = emerald_steps(direction);
    if remainder == steps.redraw {
        draw::draw_emerald(gfx, cell);
        penalty.charge(PenaltySite::EmeraldRedraw);
    }
    if remainder == steps.eat {
        draw::erase_emerald(gfx, cell);
        penalty.charge(PenaltySite::EmeraldEaten);
        emeralds.remove(player, cell);
        return true;
    }
    false
}

impl Sim {
    /// The record played by `unit` on this turn: record `turn + unit`.
    pub(crate) fn record_of_unit(&self, unit: DiggerId) -> DiggerId {
        DiggerId::new(self.turn.number().saturating_add(unit.number())).unwrap_or_else(|| {
            debug_assert!(false, "alternating players and two diggers never combine");
            unit
        })
    }

    /// The units in play: one, or two in a two-digger game.
    pub(crate) fn units(&self) -> impl Iterator<Item = DiggerId> + use<> {
        DiggerId::ALL
            .into_iter()
            .take(usize::from(self.cfg.mode.diggers()))
    }
}

impl Ctx<'_> {
    /// Resets both digger sprites to the blank image, disabled, and their
    /// animation counters, as when the sprites are set up for a round
    /// (research 02 §2, `initdbfspr`).
    pub(crate) fn reset_digger_sprites(&mut self) {
        for unit in DiggerId::ALL {
            self.gfx.reset(unit.sprite(), ImageId::BLANK);
            *unit.of_mut(&mut self.sim.digger_frames) = AnimFrame::START;
        }
    }

    /// Places every digger record in play at its start position, or takes
    /// it out of play when it has no lives left, and makes the diggers
    /// visible (digger spec, "Start position and facing"; research 02
    /// §3.2, `initdigger`).
    ///
    /// The start column is 7 with one digger and 8 − 2 × record index with
    /// two; record 0 faces right and record 1 left. Both use the absolute
    /// record index, so the second player of an alternating game starts
    /// facing left (quirk Q24).
    pub(crate) fn init_diggers(&mut self) {
        let solo = self.sim.cfg.mode.diggers() == 1;
        for unit in self.sim.units() {
            let record = self.sim.record_of_unit(unit);
            let digger = record.of_mut(&mut self.sim.diggers);
            if digger.lives == 0 {
                digger.retire();
                self.gfx.erase(unit.sprite());
                self.gfx.erase(unit.fireball().sprite());
                continue;
            }
            let column = if solo {
                SOLO_START_COLUMN
            } else {
                PAIR_START_COLUMN.saturating_sub(record.number().saturating_mul(2))
            };
            let facing = if record.number() == 0 {
                Direction::Right
            } else {
                Direction::Left
            };
            digger.start_round(column, facing);
            let at = digger.position;
            // The sprite shows the image it was reset to until its first
            // move draw.
            let image = self.gfx.sprites().pending_image(unit.sprite());
            self.gfx.place(unit.sprite(), image, at);
        }
        self.sim.diggers_visible = true;
    }

    /// The diggers' part of a tick, without the gauntlet clock and the
    /// bonus-mode timers: for each unit in order, read its controls, update
    /// its fireball, move it (or pause it after a push, or run its death),
    /// and count down its emerald window (game-flow spec, "Main-phase tick
    /// order"; research 02 §3.3, `dodigger`).
    pub(crate) fn update_diggers(&mut self, input: &FrameInput) {
        let tick = self.sim.ticks.wrapping_add(1);
        for unit in self.sim.units() {
            let record = self.sim.record_of_unit(unit);
            let slot = unit.slot();
            let raw = self.obs.controls(tick, slot, input.slot(slot));
            slot.of_mut(&mut self.sim.inputs).read(raw);
            // The fireball is updated here from task group 9 on.
            if self.sim.diggers_visible {
                let digger = *record.of(&self.sim.diggers);
                if digger.can_move() {
                    self.move_digger(unit, record);
                } else if digger.alive {
                    self.push_pause(unit, record);
                }
                // A dead digger's sequence runs here from task group 10 on.
            }
            let digger = record.of_mut(&mut self.sim.diggers);
            digger.emerald_window = digger.emerald_window.saturating_sub(1);
        }
    }

    /// The tick after a push: the digger is redrawn facing the way it
    /// moved, takes no movement sample and does not move (quirk Q15;
    /// research 02 §3.3).
    fn push_pause(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = record.of_mut(&mut self.sim.diggers);
        let facing = digger.facing;
        digger.facing = digger.moving;
        self.draw_digger(unit, record);
        let digger = record.of_mut(&mut self.sim.diggers);
        digger.facing = facing;
        self.sim.penalty.charge(PenaltySite::DiggerPushPause);
        digger.bag_time = digger.bag_time.saturating_sub(1);
    }

    /// One movement update of a digger that can move (research 02 §3.4,
    /// `updatedigger`).
    ///
    /// **Quirk Q15:** this is the only place a movement sample is taken,
    /// once per tick in which the digger is alive, visible and not in a
    /// push pause. The sample's direction is turned into a move under the
    /// turning rules and the playfield limits; a moving digger digs one
    /// slice of the cell ahead, carves its tunnel shape, steps, and may eat
    /// an emerald; then it is drawn.
    fn move_digger(&mut self, unit: DiggerId, record: DiggerId) {
        let slot = unit.slot();
        let sample = slot.of(&self.sim.inputs).sample();
        let sample = self.obs.movement(slot, sample);
        let wanted = slot.of_mut(&mut self.sim.inputs).apply_sample(sample);

        let digger = record.of_mut(&mut self.sim.diggers);
        let (moving, facing) = turn(digger.facing, digger.located, wanted);
        digger.facing = facing;
        let moving = moving.filter(|&dir| !at_limit(digger.position, dir));
        digger.moving = moving;
        let from = digger.position;
        if let Some(dir) = moving {
            self.sim.field.dig(from, Some(dir), &mut self.sim.penalty);
            draw::carve(self.gfx, Carve::for_direction(dir), from);
            let to = step(from, dir);
            record.of_mut(&mut self.sim.diggers).position = to;
            let player = self.sim.turn;
            if hit_emerald(
                self.gfx,
                &mut self.sim.emeralds,
                &mut self.sim.penalty,
                player,
                to,
                dir,
            ) {
                self.emerald_eaten(unit, record);
            }
        }
        // Meeting bags, monsters and the bonus is added by task groups 11
        // to 13 using these collisions.
        let _collisions = self.draw_digger(unit, record);
        self.sim.penalty.charge(PenaltySite::DiggerDraw);
        record.of_mut(&mut self.sim.diggers).relocate();
    }

    /// Scores an eaten emerald for `unit`'s record and advances its run:
    /// 25 points, the click and the note of the run position, and on the
    /// eighth emerald of a run 250 more and a new run (digger spec,
    /// "Eating emeralds", "Eight emeralds in a row"; research 02 §3.5).
    fn emerald_eaten(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = record.of_mut(&mut self.sim.diggers);
        if digger.emerald_window == 0 {
            digger.emerald_run = 0;
        }
        let position = digger.emerald_run;
        let score = ScoreId::credited(self.sim.turn, unit);
        self.add_score(score, ScoreEvent::Emerald);
        self.sound.push(SoundCommand::EmeraldClick);
        if let Some(note) = EmeraldStep::new(position) {
            self.sound.push(SoundCommand::EmeraldNote(note));
        }
        let digger = record.of_mut(&mut self.sim.diggers);
        digger.emerald_run = position.saturating_add(1);
        if digger.emerald_run >= EMERALD_RUN {
            digger.emerald_run = 0;
            self.add_score(score, ScoreEvent::EighthEmerald);
        }
        record.of_mut(&mut self.sim.diggers).emerald_window = EMERALD_WINDOW;
    }

    /// Draws `record` through `unit`'s sprite at its position, then applies
    /// the invincibility countdown and blink, and returns the collisions of
    /// the draw (research 02 §3.4, `drawdig`).
    ///
    /// The animation counter advances on every call. Without a facing
    /// nothing is drawn and the collisions are empty (research 02 §1
    /// rule 4).
    pub(crate) fn draw_digger(&mut self, unit: DiggerId, record: DiggerId) -> Collisions {
        let frame = unit.of_mut(&mut self.sim.digger_frames).advance();
        let digger = *record.of(&self.sim.diggers);
        let collisions =
            if let Some(image) = walking_image(unit, digger.facing, digger.fire_ready, frame) {
                self.gfx.move_digger(unit, image, digger.position)
            } else {
                self.gfx.clear_collisions();
                Collisions::EMPTY
            };
        let digger = record.of_mut(&mut self.sim.diggers);
        if digger.invincible {
            digger.invincibility = digger.invincibility.saturating_sub(1);
            if digger.invincibility == 0 {
                digger.invincible = false;
            } else if digger.invincibility % 10 < 5 {
                self.gfx.erase(unit.sprite());
            }
        }
        collisions
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::observer::Observer;
    use crate::sim::{FrameKind, Game, GameConfig, GameMode, Movement, SlotId};

    fn located(rem_x: u8, rem_y: u8) -> Located {
        Located {
            cell: CellPos::new(3, 3).unwrap_or_else(|| unreachable!()),
            rem_x,
            rem_y,
        }
    }

    #[test]
    fn turning_table() {
        use Direction::{Down, Left, Right, Up};
        // Perpendicular input between cells keeps the facing and moves on.
        assert_eq!(
            turn(Some(Right), located(8, 0), Some(Up)),
            (Some(Right), Some(Right))
        );
        // Aligned: the turn is taken.
        assert_eq!(
            turn(Some(Right), located(0, 0), Some(Up)),
            (Some(Up), Some(Up))
        );
        // Reversal along the axis is immediate.
        assert_eq!(
            turn(Some(Right), located(8, 0), Some(Left)),
            (Some(Left), Some(Left))
        );
        assert_eq!(
            turn(Some(Up), located(0, 9), Some(Down)),
            (Some(Down), Some(Down))
        );
        // No input: no move, facing kept.
        assert_eq!(turn(Some(Up), located(0, 9), None), (None, Some(Up)));
        // A facing of none is recovered only when aligned for the input.
        assert_eq!(turn(None, located(0, 9), Some(Right)), (None, None));
        assert_eq!(
            turn(None, located(0, 0), Some(Right)),
            (Some(Right), Some(Right))
        );
    }

    #[test]
    fn anim_frame_ping_pongs() {
        let mut frame = AnimFrame::START;
        let frames: Vec<u8> = (0..8).map(|_| frame.advance()).collect();
        assert_eq!(frames, [1, 2, 1, 0, 1, 2, 1, 0]);
    }

    #[test]
    fn walking_images_by_facing_and_look() {
        let [first, second] = DiggerId::ALL;
        let n = |unit, facing, ready, frame| {
            walking_image(unit, Some(facing), ready, frame).map(ImageId::number)
        };
        assert_eq!(n(first, Direction::Right, true, 0), Some(1));
        assert_eq!(n(first, Direction::Right, false, 2), Some(6));
        assert_eq!(n(first, Direction::Up, true, 1), Some(8));
        assert_eq!(n(first, Direction::Left, true, 0), Some(13));
        assert_eq!(n(first, Direction::Down, false, 2), Some(24));
        assert_eq!(n(second, Direction::Right, true, 0), Some(32));
        assert_eq!(n(second, Direction::Down, false, 2), Some(55));
        assert_eq!(walking_image(first, None, true, 0), None);
    }

    #[test]
    fn emerald_steps_table() {
        let pairs = EMERALD_STEPS.map(|s| (s.redraw, s.eat));
        assert_eq!(pairs, [(8, 12), (12, 9), (16, 12), (6, 9)]);
    }

    /// Steps `game` until the next frame is the round's first tick.
    fn to_first_tick(game: &mut Game) {
        while !matches!(game.phase, super::super::Phase::Playing) {
            game.step(&FrameInput::NONE);
        }
    }

    #[test]
    fn second_alternating_player_faces_left() {
        // Player 2's round: the record of player 2 plays through the first
        // unit's sprite at column 7.
        let config = GameConfig {
            mode: GameMode::TwoPlayers,
            ..GameConfig::default()
        };
        let mut game = Game::new(config, 0);
        to_first_tick(&mut game);
        game.sim.turn = PlayerId::ALL[1];
        let mut obs = super::super::NoopObserver;
        let mut cx = Ctx {
            sim: &mut game.sim,
            gfx: &mut game.gfx,
            sound: &mut game.sound,
            obs: &mut obs,
        };
        cx.reset_digger_sprites();
        cx.init_diggers();
        let [first, second] = DiggerId::ALL;
        let digger = second.of(&game.sim.diggers);
        assert_eq!(digger.position(), Px::new(152, 180));
        assert_eq!(digger.facing(), Some(Direction::Left));
        assert!(digger.alive());
        assert!(game.gfx.sprites().is_enabled(first.sprite()));
        assert!(!game.gfx.sprites().is_enabled(second.sprite()));
        assert_eq!(
            game.gfx.sprites().position(first.sprite()),
            Px::new(152, 180)
        );
        // The next tick draws player 2 with the first digger's left-facing
        // images.
        assert_eq!(game.step(&FrameInput::NONE).frame, FrameKind::Tick);
        let image = game.gfx.sprites().image(first.sprite()).number();
        assert!((13..=15).contains(&image), "image {image}");
    }

    /// Counts the movement samples taken.
    #[derive(Default)]
    struct Samples(Vec<SlotId>);

    impl Observer for Samples {
        fn movement(&mut self, slot: SlotId, sample: Movement) -> Movement {
            self.0.push(slot);
            sample
        }
    }

    #[test]
    fn quirk_q15_no_movement_sample_during_push_pause() {
        let mut game = Game::new(GameConfig::default(), 0);
        to_first_tick(&mut game);
        let right = FrameInput {
            slots: [
                crate::sim::Controls {
                    right: true,
                    ..crate::sim::Controls::NONE
                },
                crate::sim::Controls::NONE,
            ],
            ..FrameInput::NONE
        };
        let mut obs = Samples::default();
        game.step_observed(&right, &mut obs);
        assert_eq!(obs.0.len(), 1);
        let first = DiggerId::ALL[0];
        let before = first.of(&game.sim.diggers).position();
        // As after a push: one pause tick.
        first.of_mut(&mut game.sim.diggers).bag_time = 1;
        game.step_observed(&right, &mut obs);
        assert_eq!(obs.0.len(), 1, "no sample during the push pause");
        let digger = first.of(&game.sim.diggers);
        assert_eq!(digger.position(), before);
        assert_eq!(digger.bag_time(), 0);
        let penalty = game.state().penalty();
        assert_eq!(penalty.count_of(PenaltySite::DiggerPushPause), 1);
        assert_eq!(penalty.count_of(PenaltySite::DiggerDraw), 0);
        game.step_observed(&right, &mut obs);
        assert_eq!(obs.0.len(), 2, "sampling resumes after the pause");
        assert_eq!(
            first.of(&game.sim.diggers).position(),
            Px::new(before.x + 4, before.y)
        );

        // Invisible diggers (the wind-down) take no sample either.
        game.sim.diggers_visible = false;
        game.step_observed(&right, &mut obs);
        assert_eq!(obs.0.len(), 2);
    }

    #[test]
    fn out_of_lives_record_is_retired_at_round_start() {
        let mut game = Game::new(GameConfig::default(), 0);
        game.sim.diggers[0].lives = 0;
        to_first_tick(&mut game);
        let digger = DiggerId::ALL[0].of(&game.sim.diggers);
        assert!(digger.dead() && !digger.alive());
        assert_eq!(digger.death_stage(), Some(DeathStage::Done));
        assert!(!game.gfx.sprites().is_enabled(DiggerId::ALL[0].sprite()));
    }

    #[test]
    fn never_placed_record_keeps_zero_values() {
        let mut game = Game::new(GameConfig::default(), 0);
        to_first_tick(&mut game);
        let digger = DiggerId::ALL[1].of(&game.sim.diggers);
        assert_eq!(*digger, Digger::NEVER_PLACED);
        assert_eq!(digger.death_stage(), None);
        assert_eq!(digger.facing(), Some(Direction::Right));
    }

    #[test]
    fn invincible_digger_blinks_and_counts_down() {
        let mut game = Game::new(GameConfig::default(), 0);
        to_first_tick(&mut game);
        let first = DiggerId::ALL[0];
        {
            let digger = first.of_mut(&mut game.sim.diggers);
            digger.invincible = true;
            digger.invincibility = 7;
        }
        // ivt 6: 6 % 10 >= 5, drawn; ivt 5 drawn; ivt 4 erased.
        let mut shown = Vec::new();
        for _ in 0..3 {
            game.step(&FrameInput::NONE);
            shown.push(game.gfx.sprites().is_enabled(first.sprite()));
        }
        assert_eq!(shown, [true, true, false]);
        assert_eq!(first.of(&game.sim.diggers).invincibility(), 4);
        for _ in 0..4 {
            game.step(&FrameInput::NONE);
        }
        let digger = first.of(&game.sim.diggers);
        assert_eq!(digger.invincibility(), 0);
        assert!(!digger.invincible());
        assert!(game.gfx.sprites().is_enabled(first.sprite()));
    }

    #[test]
    fn digger_without_facing_draws_nothing() {
        let mut game = Game::new(GameConfig::default(), 0);
        to_first_tick(&mut game);
        let first = DiggerId::ALL[0];
        game.step(&FrameInput::NONE);
        first.of_mut(&mut game.sim.diggers).facing = None;
        let picture = game.gfx.fb().clone();
        game.step(&FrameInput::NONE);
        assert_eq!(game.gfx.fb(), &picture);
        assert!(game.gfx.sprites().collisions().is_empty());
    }
}
