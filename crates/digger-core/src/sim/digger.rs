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
use crate::sound_cmd::{EmeraldStep, SoundCommand, Tempo, Tune};
use crate::sprite::{Collisions, SpriteClass};

use super::draw::{self, Carve};
use super::field::EmeraldMap;
use super::fireball::Fireball;
use super::fireball::FireballStage;
use super::geom::{CellPos, Direction, Located, MAX_X, MAX_Y, MIN_X, MIN_Y, locate, reverse_of};
use super::gfx::Gfx;
use super::ids::{BagId, DiggerId, MonsterSlot, PlayerId};
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

/// How far above its position a digger killed by a monster or a fireball
/// is drawn on each of the 7 ticks of its death arc (digger spec, "Death by
/// monster or fireball"; research 02 §3.7).
const DEATH_ARC: [i32; 7] = [3, 5, 6, 6, 5, 3, 0];
/// Ticks the grave sequence waits before its first tombstone frame
/// (research 02 §3.7).
const GRAVE_WAIT: u16 = 5;
/// Ticks between two tombstone frames besides the tick of the frame itself,
/// so the frames are drawn three ticks apart (research 02 §3.7).
const TOMBSTONE_GAP: u16 = 2;
/// The last of the five tombstone frames, counted from 0.
const LAST_TOMBSTONE: u8 = 4;
/// Ticks the resting tombstone stays after the last frame before the death
/// is resolved, at G + 73 (research 02 §3.7).
const TOMBSTONE_REST: u16 = 60;
/// **Quirk Q23:** in a one-digger game the death is resolved no earlier
/// than 89 ticks after the first tombstone frame, at G + 89, to leave time
/// for the dirge; the wait is counted in ticks, never in real time (digger
/// spec, "Grave sequence"; research 02 §3.7, §9 item 23).
pub(crate) const DIRGE_TICKS: u16 = 89;
/// Draws of invincibility a respawned digger is given (digger spec,
/// "Invincibility"; research 02 §3.7).
const RESPAWN_INVINCIBILITY: u16 = 50;
/// **Quirk Q21 (digger side):** a digger crushed by a bag is held no higher
/// than this many pixels below the bag's top while carried, and raised by
/// as much when the bag stops (research 02 §3.7, §9 item 6).
const CARRIED_BELOW_BAG: i32 = 6;

/// What a dead digger's sprite shows (research 02 §2, `drawdigger`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeathPicture {
    /// The dead digger, upside down.
    Dead,
    /// Tombstone frame 0..=4, growing.
    Tombstone(u8),
}

/// The image of `picture` for the digger drawn by unit `unit`: the dead
/// digger is image 25 and the tombstone frames are 26 to 30, each 31
/// further on for the second digger (research 03 §5.2).
fn death_image(unit: DiggerId, picture: DeathPicture) -> Option<ImageId> {
    let dead = ImageId::DIGGER_DEAD.number();
    let number = match picture {
        DeathPicture::Dead => dead,
        DeathPicture::Tombstone(frame) => dead + 1 + frame.min(LAST_TOMBSTONE),
    };
    let image = ImageId::new(number)?;
    if unit.number() == 0 {
        Some(image)
    } else {
        image.for_second_digger()
    }
}

/// The start column and facing of digger record `record`: column 7 with
/// one digger and 8 − 2 × record index with two; record 0 faces right and
/// every other record left (digger spec, "Start position and facing";
/// research 02 §3.2).
///
/// **Quirk Q24:** both use the absolute record index, so the second player
/// of an alternating game starts facing left, and a respawn uses the same
/// rule.
fn start_of(solo: bool, record: DiggerId) -> (u8, Direction) {
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
    (column, facing)
}

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

    /// The current frame, 0..=2.
    pub(crate) const fn frame(self) -> u8 {
        self.frame
    }

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
    /// Ticks still to wait for the dirge before a one-digger death is
    /// resolved (`dirgetime`; not hashed). Set at the first tombstone frame,
    /// counted down on every tick of the death sequence and cleared at
    /// round start (`initdigger`); a respawn keeps it.
    pub(crate) dirge_wait: u16,
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
    /// The digger's fireball (`bob`).
    pub(crate) fireball: Fireball,
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
        dirge_wait: 0,
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
        fireball: Fireball::ZEROED,
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

    /// Doubles the monster-eating multiplier after an eat (research 02
    /// §3.8, `sceatm`).
    ///
    /// **Quirk Q12:** the multiplier is a 16-bit value and doubles with
    /// wrap-around.
    pub(crate) const fn bump_eat_multiplier(&mut self) {
        self.eat_multiplier = self.eat_multiplier.wrapping_mul(2);
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

    /// The digger's fireball (hashed as `bob`), stale values included
    /// while none is in play.
    pub const fn fireball(&self) -> &Fireball {
        &self.fireball
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
            dirge_wait: 0,
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
            fireball: Fireball::ZEROED,
        };
    }

    /// Takes one of the digger's lives, except in gauntlet mode, where
    /// lives never change (research 01 §3.2, `declife`).
    pub(crate) const fn lose_life(&mut self, gauntlet: bool) {
        if !gauntlet {
            self.lives = self.lives.saturating_sub(1);
        }
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
        self.dirge_wait = 0;
        self.emerald_window = 0;
    }

    /// Brings a digger back at its start cell in a two-digger game, alive and
    /// invincible for 50 draws, with no push pause, a ready fireball, no
    /// emerald run and an eat multiplier of 1 (digger spec, "Respawn with
    /// invincibility in two-digger games"; research 02 §3.7).
    ///
    /// **Quirk Q25:** the fire-ready look is not restored, and the death
    /// counters, the dirge wait and the fireball's last position keep their
    /// values.
    fn respawn(&mut self, column: u8, facing: Direction) {
        let cell = CellPos::new(column, START_ROW).unwrap_or_else(|| {
            debug_assert!(false, "start columns lie inside the grid");
            self.located.cell
        });
        self.position = cell.to_px();
        self.facing = Some(facing);
        self.moving = Some(Direction::Left);
        self.located = Located {
            cell,
            rem_x: 0,
            rem_y: 0,
        };
        self.bag_time = 0;
        self.alive = true;
        self.dead = false;
        self.invincible = true;
        self.invincibility = RESPAWN_INVINCIBILITY;
        self.death = Some(DeathStage::Bag);
        self.not_firing = true;
        self.emerald_window = 0;
        self.fireball.stage = FireballStage::Flying;
        self.recharge_time = 0;
        self.emerald_run = 0;
        self.eat_multiplier = 1;
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

/// Whether a digger or monster at `at` may not move in `direction` because
/// it is at that edge of the playfield (digger spec, "Step size and
/// playfield limits"; monsters spec, "Movement step and move scheduling").
pub(super) const fn at_limit(at: Px, direction: Direction) -> bool {
    match direction {
        Direction::Right => at.x == MAX_X,
        Direction::Left => at.x == MIN_X,
        Direction::Down => at.y == MAX_Y,
        Direction::Up => at.y == MIN_Y,
    }
}

/// The position one step from `at` in `direction`: 4 pixels horizontally,
/// 3 vertically.
pub(super) const fn step(at: Px, direction: Direction) -> Px {
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
pub(super) fn hit_emerald(
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
    /// Kills digger record `record`, dying in the manner of `stage`, by bag
    /// `bag` when crushed (research 02 §3.7, `killdigger`).
    ///
    /// Nothing happens to an invincible digger or to one in its grave
    /// stages (waiting or growing tombstone, the undrawn tick, the resting
    /// tombstone); a digger carried by a bag or in its death arc dies again
    /// in the new manner (quirk Q20). The death sequence then runs in the
    /// digger's updates.
    ///
    /// **Quirk Q20:** the kill is applied only outside the grave stages
    /// (2 to 4); stage 1 (carried by a bag) and stage 5 (the arc) restart.
    pub(crate) fn kill_digger(&mut self, record: DiggerId, stage: DeathStage, bag: BagId) {
        let digger = record.of_mut(&mut self.diggers);
        if digger.invincible {
            return;
        }
        if matches!(
            digger.death,
            Some(DeathStage::Grave | DeathStage::Monster | DeathStage::Tombstone)
        ) {
            return;
        }
        digger.alive = false;
        digger.death = Some(stage);
        digger.death_bag = bag;
    }

    /// Whether the digger of any unit in play is alive and controllable
    /// (research 02 §3.1, `isalive`).
    pub(crate) fn any_alive(&self) -> bool {
        self.units()
            .any(|unit| self.record_of_unit(unit).of(&self.diggers).alive)
    }

    /// Whether every digger's death sequence has finished, for the end of a
    /// round (research 02 §3.7).
    ///
    /// **Quirk Q24:** the records tested are 0 up to the number of diggers,
    /// whoever's turn it is, so for the second player of an alternating
    /// game it is the first player's record that is tested. That record is
    /// always dead by then, because the turn passes only after a death.
    pub(crate) fn all_records_dead(&self) -> bool {
        DiggerId::ALL
            .into_iter()
            .take(usize::from(self.cfg.mode.diggers()))
            .all(|record| record.of(&self.diggers).dead)
    }

    /// The record played by `unit` on this turn: record `turn + unit`.
    pub(crate) fn record_of_unit(&self, unit: DiggerId) -> DiggerId {
        DiggerId::new(self.turn.number().saturating_add(unit.number())).unwrap_or_else(|| {
            debug_assert!(false, "alternating players and two diggers never combine");
            unit
        })
    }

    /// **Quirk Q36:** whether a living, non-invincible digger that last
    /// moved up or down is in the column of `below` (the cell under a
    /// bag), in that cell's row or the row above it, so that the bag does
    /// not start to wobble (gold-bags spec, "When a bag starts to wobble";
    /// research 02 §5.3, `checkdiggerunderbag`). The digger's cell is taken
    /// from its current position.
    pub(crate) fn digger_under_bag(&self, below: CellPos) -> bool {
        self.units().any(|unit| {
            let digger = self.record_of_unit(unit).of(&self.diggers);
            digger.alive
                && !digger.invincible
                && digger.moving.is_some_and(Direction::is_vertical)
                && locate(digger.position).is_some_and(|at| {
                    at.cell.h() == below.h()
                        && (at.cell.v() == below.v() || at.cell.v() + 1 == below.v())
                })
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
    /// animation counters, and the fireball sprites likewise, as when the sprites are set up for a round
    /// (research 02 §2, `initdbfspr`).
    pub(crate) fn reset_digger_sprites(&mut self) {
        for unit in DiggerId::ALL {
            self.gfx.reset(unit.sprite(), ImageId::BLANK);
            *unit.of_mut(&mut self.sim.digger_frames) = AnimFrame::START;
        }
        self.reset_fireball_sprites();
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
            let (column, facing) = start_of(solo, record);
            digger.start_round(column, facing);
            let at = digger.position;
            // The sprite shows the image it was reset to until its first
            // move draw.
            let image = self.gfx.sprites().pending_image(unit.sprite());
            self.gfx.place(unit.sprite(), image, at);
        }
        self.sim.diggers_visible = true;
        self.sim.reset_bonus();
    }

    /// The diggers' part of a tick: in gauntlet mode the clock is drawn and
    /// counted down first (quirk Q13); then for each unit in order, read
    /// its controls, update its fireball, move it (or pause it after a
    /// push, or run its death), and count down its emerald window; then
    /// the bonus-mode timers (game-flow spec, "Update order of a
    /// main-phase tick", "Gauntlet clock"; research 02 §3.3, research 01
    /// §7.4, `dodigger`).
    pub(crate) fn update_diggers(&mut self, input: &FrameInput) {
        if self.sim.gauntlet.is_some() {
            // The clock is drawn with the value before this tick's step.
            self.draw_lives();
            if let Some(clock) = &mut self.sim.gauntlet {
                clock.count_down();
            }
        }
        let tick = self.sim.ticks.wrapping_add(1);
        for unit in self.sim.units() {
            let record = self.sim.record_of_unit(unit);
            let slot = unit.slot();
            let raw = self.obs.controls(tick, slot, input.slot(slot));
            slot.of_mut(&mut self.sim.inputs).read(raw);
            self.update_fireball(unit, record);
            if self.sim.diggers_visible {
                let digger = *record.of(&self.sim.diggers);
                if digger.can_move() {
                    self.move_digger(unit, record);
                } else if digger.alive {
                    self.push_pause(unit, record);
                } else {
                    self.digger_die(unit, record);
                }
            }
            let digger = record.of_mut(&mut self.sim.diggers);
            digger.emerald_window = digger.emerald_window.saturating_sub(1);
        }
        self.update_bonus_mode();
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
        let hits = self.draw_digger(unit, record);
        self.sim.penalty.charge(PenaltySite::DiggerDraw);
        self.meet_bags(unit, record, from, hits);
        self.meet_monsters_as_digger(unit, record, hits);
        if hits.first(SpriteClass::Bonus).is_some() {
            self.collect_bonus(unit);
        }
        record.of_mut(&mut self.sim.diggers).relocate();
    }

    /// The digger side of meeting monsters after a move whose draw gave
    /// `hits` (digger spec, "Eating monsters by moving into them"; research
    /// 02 §3.4 step 11).
    ///
    /// Outside bonus mode nothing happens: only a monster's own move kills
    /// (quirk Q26). In bonus mode a digger still alive removes every
    /// monster the draw touched, in slot order, and for each one the eat
    /// cue sounds and the digger scores its next eating step.
    fn meet_monsters_as_digger(&mut self, unit: DiggerId, record: DiggerId, hits: Collisions) {
        if !self.sim.bonus_mode || !record.of(&self.sim.diggers).alive {
            return;
        }
        for sprite in hits.of(SpriteClass::Monster) {
            if let Some(slot) = MonsterSlot::from_sprite(sprite) {
                self.remove_monster(slot);
            }
        }
        for _ in hits.of(SpriteClass::Monster) {
            self.sound.push(SoundCommand::EatMonster);
            self.eat_monster(unit, record);
        }
    }

    /// The digger side of meeting bags after a move whose draw gave `hits`
    /// (digger spec, "Meeting a bag while moving", "Digger left without a
    /// facing"; research 02 §3.4).
    ///
    /// Nothing happens unless the draw touched a bag, other than a falling
    /// bag while the digger is invincible. A left or right move pushes
    /// every touched bag; any other move, standing still included, collects
    /// gold and is blocked by an unbroken bag. A blocked or failed move
    /// puts the digger back at `from`, where it is drawn once more facing
    /// the way it moved, and then faces the other way; a digger that did
    /// not move is left without a facing.
    ///
    /// **Quirk Q17:** every horizontal push attempt, successful or not,
    /// sets the push pause for the next tick.
    ///
    /// **Quirk Q18:** the blocked digger is redrawn, which counts down its
    /// invincibility a second time, and its facing is reversed.
    fn meet_bags(&mut self, unit: DiggerId, record: DiggerId, from: Px, hits: Collisions) {
        let digger = *record.of(&self.sim.diggers);
        let met = hits.of(SpriteClass::Bag).any(|sprite| {
            BagId::from_sprite(sprite).is_some_and(|id| {
                let bag = id.of(&self.sim.bags);
                bag.exists() && !(digger.invincible && bag.is_falling())
            })
        });
        if !met {
            return;
        }
        let moving = digger.moving;
        let pushed = match moving {
            Some(direction) if direction.is_horizontal() => {
                let pushed = self.push_bags(direction, hits);
                let digger = record.of_mut(&mut self.sim.diggers);
                digger.bag_time = digger.bag_time.saturating_add(1);
                pushed
            }
            _ => self.push_bags_vertically(hits),
        };
        if !pushed {
            let digger = record.of_mut(&mut self.sim.diggers);
            digger.position = from;
            digger.facing = moving;
            self.draw_digger(unit, record);
            self.sim.penalty.charge(PenaltySite::DiggerBlockedRedraw);
            record.of_mut(&mut self.sim.diggers).facing = reverse_of(moving);
        }
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

    /// One tick of a dead digger's sequence (digger spec, "Death by monster
    /// or fireball", "Death by falling bag", "Grave sequence"; research 02
    /// §3.7, `diggerdie`). The dirge wait counts down first, on every tick
    /// of the sequence.
    fn digger_die(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = record.of_mut(&mut self.sim.diggers);
        digger.dirge_wait = digger.dirge_wait.saturating_sub(1);
        match digger.death {
            Some(DeathStage::Bag) => self.carried_by_bag(unit, record),
            Some(DeathStage::Monster) => {
                // The undrawn tick.
                digger.death = Some(DeathStage::Arc);
                digger.death_frame = 0;
                digger.death_timer = 0;
            }
            Some(DeathStage::Arc) => self.death_arc(unit, record),
            Some(DeathStage::Grave) => self.grave(unit, record),
            Some(DeathStage::Tombstone) => self.tombstone(unit, record),
            Some(DeathStage::Done) | None => {}
        }
    }

    /// Draws `unit`'s sprite at `at` showing `picture`, advancing its
    /// animation counter as every digger draw does, and returns the
    /// collisions of the draw. No invincibility countdown applies.
    fn draw_dead(&mut self, unit: DiggerId, picture: DeathPicture, at: Px) -> Collisions {
        unit.of_mut(&mut self.sim.digger_frames).advance();
        if let Some(image) = death_image(unit, picture) {
            self.gfx.move_digger(unit, image, at)
        } else {
            debug_assert!(false, "every death picture has an image");
            Collisions::EMPTY
        }
    }

    /// A digger crushed by a bag: held no higher than 6 pixels below the
    /// bag's top and drawn dead; once the bag has stopped or no longer
    /// exists, the death cue sounds, the digger is raised 6 pixels and the
    /// grave sequence begins (quirk Q21; research 02 §3.7, §5.7).
    fn carried_by_bag(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = *record.of(&self.sim.diggers);
        let bag = *digger.death_bag.of(&self.sim.bags);
        let mut at = digger.position;
        at.y = at.y.max(bag.position.y + CARRIED_BELOW_BAG);
        record.of_mut(&mut self.sim.diggers).position = at;
        self.draw_dead(unit, DeathPicture::Dead, at);
        self.sim.penalty.charge(PenaltySite::CarriedDigger);
        if bag.has_stopped() {
            self.sound.push(SoundCommand::DiggerDeath);
            let digger = record.of_mut(&mut self.sim.diggers);
            digger.death_timer = GRAVE_WAIT;
            digger.death = Some(DeathStage::Grave);
            digger.death_frame = 0;
            digger.position.y -= CARRIED_BELOW_BAG;
        }
    }

    /// One tick of the 7-tick death arc: the dead digger is drawn 3, 5, 6,
    /// 6, 5, 3 and 0 pixels above its position, the death cue sounds after
    /// the first draw, the music stops at the last if no digger is alive,
    /// and the grave sequence follows (research 02 §3.7).
    fn death_arc(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = *record.of(&self.sim.diggers);
        let Some(&lift) = DEATH_ARC.get(usize::from(digger.death_frame)) else {
            return;
        };
        let at = Px::new(digger.position.x, digger.position.y - lift);
        self.draw_dead(unit, DeathPicture::Dead, at);
        let last = usize::from(digger.death_frame) + 1 == DEATH_ARC.len();
        if last && !self.sim.any_alive() {
            self.sim.main_tune = None;
            self.sound.push(SoundCommand::MusicOff);
        }
        self.sim.penalty.charge(PenaltySite::DeathArc);
        let digger = record.of_mut(&mut self.sim.diggers);
        digger.death_frame += 1;
        if digger.death_frame == 1 {
            self.sound.push(SoundCommand::DiggerDeath);
        }
        if last {
            digger.death_timer = GRAVE_WAIT;
            digger.death_frame = 0;
            digger.death = Some(DeathStage::Grave);
        }
    }

    /// One tick of the grave sequence: 5 ticks of waiting, then five
    /// growing tombstone frames three ticks apart, then the resting
    /// tombstone (digger spec, "Grave sequence"; research 02 §3.7).
    ///
    /// With the first frame, on tick G, the dirge is requested (at the
    /// quick tempo with two diggers) and the dirge wait of quirk Q23
    /// starts. **Quirk Q22:** every monster the first frame's draw touches
    /// is removed, and nobody scores for it.
    fn grave(&mut self, unit: DiggerId, record: DiggerId) {
        let digger = record.of_mut(&mut self.sim.diggers);
        if digger.death_timer != 0 {
            digger.death_timer -= 1;
            return;
        }
        let frame = digger.death_frame;
        let at = digger.position;
        if frame == 0 {
            digger.dirge_wait = DIRGE_TICKS;
            let tempo = if self.sim.cfg.mode.diggers() == 1 {
                Tempo::Standard
            } else {
                Tempo::Quick
            };
            self.sim.main_tune = None;
            self.sound.push(SoundCommand::Music {
                tune: Tune::Dirge,
                tempo,
            });
        }
        let hits = self.draw_dead(unit, DeathPicture::Tombstone(frame), at);
        self.sim.penalty.charge(PenaltySite::TombstoneFrame);
        if frame == 0 {
            for sprite in hits.of(SpriteClass::Monster) {
                if let Some(slot) = MonsterSlot::from_sprite(sprite) {
                    self.remove_monster(slot);
                }
            }
        }
        let digger = record.of_mut(&mut self.sim.diggers);
        if frame < LAST_TOMBSTONE {
            digger.death_frame = frame + 1;
            digger.death_timer = TOMBSTONE_GAP;
        } else {
            digger.death = Some(DeathStage::Tombstone);
            digger.death_timer = TOMBSTONE_REST;
        }
    }

    /// The resting tombstone and the resolution of the death (digger spec,
    /// "Lives", "Respawn with invincibility in two-digger games"; research
    /// 02 §3.7).
    ///
    /// After 60 ticks (and, with one digger, once the dirge wait of quirk
    /// Q23 is over) the death sequence is finished. When every digger's has
    /// finished (quirk Q24) the round is over. Otherwise, while the partner
    /// is alive, the digger loses a life (never in gauntlet mode), the lives
    /// are redrawn, a digger with a life left respawns, and the main music
    /// is requested again; a digger without lives stays a tombstone for one
    /// more tick and is then erased and taken out of play.
    ///
    /// The original also forgets a fire press reported since the last read
    /// for the record's index here (`clearfire`). The slot's read earlier
    /// in the same update has already taken any such press, so nothing is
    /// left to forget.
    fn tombstone(&mut self, unit: DiggerId, record: DiggerId) {
        let solo = self.sim.cfg.mode.diggers() == 1;
        let digger = record.of_mut(&mut self.sim.diggers);
        if digger.death_timer != 0 {
            digger.death_timer -= 1;
            return;
        }
        if solo && digger.dirge_wait != 0 {
            return;
        }
        digger.dead = true;
        let lives = digger.lives;
        if self.sim.all_records_dead() {
            self.sim.all_dead = true;
        } else if self.sim.any_alive() && lives > 0 {
            let lives = if self.sim.cfg.mode.gauntlet().is_some() {
                lives
            } else {
                lives - 1
            };
            record.of_mut(&mut self.sim.diggers).lives = lives;
            self.draw_lives();
            if lives > 0 {
                let (column, facing) = start_of(solo, record);
                let digger = record.of_mut(&mut self.sim.diggers);
                digger.respawn(column, facing);
                let at = digger.position;
                // Shown with the image it was last given until its first
                // move draw.
                let sprite = unit.sprite();
                let image = self.gfx.sprites().pending_image(sprite);
                self.gfx.erase(sprite);
                self.gfx.place(sprite, image, at);
            }
            if self.sim.bonus_mode {
                self.request_bonus_music();
            } else {
                self.request_main_music();
            }
        } else {
            self.gfx.erase(unit.sprite());
            record.of_mut(&mut self.sim.diggers).death = Some(DeathStage::Done);
        }
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

    /// A game in `mode` stepped to its first tick.
    fn playing(mode: GameMode) -> Box<Game> {
        let config = GameConfig {
            mode,
            ..GameConfig::default()
        };
        let mut game = Game::new(config, 0);
        to_first_tick(&mut game);
        game
    }

    /// Steps `game` by `n` frames with no input.
    fn steps(game: &mut Game, n: u32) {
        for _ in 0..n {
            game.step(&FrameInput::NONE);
        }
    }

    #[test]
    fn quirk_q20_kill_ignored_in_grave_stages() {
        let mut game = playing(GameMode::OnePlayer);
        let first = DiggerId::ALL[0];
        let [bag_a, _, _, _, _, bag_b, _] = BagId::ALL;
        for stage in [
            DeathStage::Grave,
            DeathStage::Monster,
            DeathStage::Tombstone,
        ] {
            {
                let digger = first.of_mut(&mut game.sim.diggers);
                digger.alive = false;
                digger.death = Some(stage);
                digger.death_bag = bag_a;
            }
            game.sim.kill_digger(first, DeathStage::Bag, bag_b);
            game.sim.kill_digger(first, DeathStage::Monster, bag_b);
            let digger = first.of(&game.sim.diggers);
            assert_eq!(digger.death_stage(), Some(stage));
            assert_eq!(digger.death_bag(), bag_a);
        }

        // An invincible living digger is not killed either.
        let digger = first.of_mut(&mut game.sim.diggers);
        digger.alive = true;
        digger.death = Some(DeathStage::Bag);
        digger.invincible = true;
        digger.invincibility = 20;
        game.sim.kill_digger(first, DeathStage::Monster, bag_a);
        let digger = first.of(&game.sim.diggers);
        assert!(digger.alive());
        assert_eq!(digger.death_stage(), Some(DeathStage::Bag));
    }

    #[test]
    fn quirk_q20_carried_digger_can_be_rekilled() {
        let mut game = playing(GameMode::OnePlayer);
        let first = DiggerId::ALL[0];
        let bag = BagId::ALL[3];
        // Carried by a bag, then touched by a monster.
        game.sim.kill_digger(first, DeathStage::Bag, bag);
        let digger = first.of(&game.sim.diggers);
        assert!(!digger.alive());
        assert_eq!(digger.death_stage(), Some(DeathStage::Bag));
        assert_eq!(digger.death_bag(), bag);
        game.sim
            .kill_digger(first, DeathStage::Monster, BagId::ALL[0]);
        let digger = first.of(&game.sim.diggers);
        assert_eq!(digger.death_stage(), Some(DeathStage::Monster));
        assert_eq!(digger.death_bag(), BagId::ALL[0]);

        // In the arc, then crushed by a bag.
        first.of_mut(&mut game.sim.diggers).death = Some(DeathStage::Arc);
        game.sim.kill_digger(first, DeathStage::Bag, bag);
        let digger = first.of(&game.sim.diggers);
        assert_eq!(digger.death_stage(), Some(DeathStage::Bag));
        assert_eq!(digger.death_bag(), bag);
    }

    #[test]
    fn quirk_q22_grave_removal_scores_nothing() {
        let mut game = playing(GameMode::OnePlayer);
        steps(&mut game, 1);
        let first = DiggerId::ALL[0];
        game.sim
            .kill_digger(first, DeathStage::Monster, BagId::ALL[0]);
        // A monster on the digger, held in place while no digger is alive.
        let slot = MonsterSlot::ALL[2];
        let at = first.of(&game.sim.diggers).position();
        {
            let mut obs = super::super::NoopObserver;
            let mut cx = Ctx {
                sim: &mut game.sim,
                gfx: &mut game.gfx,
                sound: &mut game.sound,
                obs: &mut obs,
            };
            cx.put_monster(slot, at, true, Some(Direction::Left), 0, 0, 0);
        }
        // The undrawn tick, the 7-tick arc and the 5-tick wait.
        steps(&mut game, 13);
        assert!(slot.of(&game.sim.monsters.slots).occupied());
        let score = game.sim.scores.visible(ScoreId::ALL[0]);
        // Tick G: the first tombstone frame removes it.
        steps(&mut game, 1);
        assert_eq!(first.of(&game.sim.diggers).death_frame(), 1);
        let monster = slot.of(&game.sim.monsters.slots);
        assert!(!monster.occupied() && !monster.alive());
        assert!(!game.gfx.sprites().is_enabled(slot.sprite()));
        assert_eq!(game.sim.scores.visible(ScoreId::ALL[0]), score);
        let penalty = game.state().penalty();
        assert_eq!(penalty.count_of(PenaltySite::ScoreCall), 0);
        assert_eq!(penalty.count_of(PenaltySite::TombstoneFrame), 1);
    }

    #[test]
    fn quirk_q24_all_dead_uses_absolute_indices() {
        // The second player's turn of an alternating game: only record 0
        // is tested.
        let mut game = playing(GameMode::TwoPlayers);
        game.sim.turn = PlayerId::ALL[1];
        let [first, second] = DiggerId::ALL;
        first.of_mut(&mut game.sim.diggers).dead = false;
        second.of_mut(&mut game.sim.diggers).dead = true;
        assert!(!game.sim.all_records_dead());
        first.of_mut(&mut game.sim.diggers).dead = true;
        second.of_mut(&mut game.sim.diggers).dead = false;
        assert!(game.sim.all_records_dead());

        // With two diggers both records are tested.
        let mut game = playing(GameMode::TwoDiggers);
        first.of_mut(&mut game.sim.diggers).dead = true;
        assert!(!game.sim.all_records_dead());
        second.of_mut(&mut game.sim.diggers).dead = true;
        assert!(game.sim.all_records_dead());
    }

    #[test]
    fn quirk_q25_respawn_keeps_fire_look() {
        let mut game = playing(GameMode::TwoDiggers);
        steps(&mut game, 1);
        let second = DiggerId::ALL[1];
        {
            // As after a shot whose recharge would outlast the death.
            let digger = second.of_mut(&mut game.sim.diggers);
            digger.fire_ready = false;
            digger.recharge_time = 500;
        }
        game.sim
            .kill_digger(second, DeathStage::Monster, BagId::ALL[0]);
        // Resolved at G + 73 = T + 87.
        steps(&mut game, 86);
        assert!(!second.of(&game.sim.diggers).alive());
        steps(&mut game, 1);
        let digger = second.of(&game.sim.diggers);
        assert!(digger.alive() && digger.invincible());
        assert_eq!(digger.recharge_time(), 0);
        assert!(digger.not_firing());
        assert!(!digger.fire_ready(), "the fire-ready look is not restored");
        // It is drawn without the look from then on, and can fire.
        steps(&mut game, 1);
        let image = game.gfx.sprites().image(second.sprite());
        let without_look: Vec<_> = (0..=2)
            .filter_map(|frame| walking_image(second, Some(Direction::Left), false, frame))
            .collect();
        assert!(without_look.contains(&image), "{image:?}");
        assert!(!second.of(&game.sim.diggers).fire_ready());
    }

    #[test]
    fn round_start_clears_dirge_wait() {
        let mut digger = Digger::NEVER_PLACED;
        digger.dirge_wait = 16;
        digger.start_round(SOLO_START_COLUMN, Direction::Right);
        assert_eq!(digger.dirge_wait, 0);
        digger.dirge_wait = 16;
        digger.retire();
        assert_eq!(digger.dirge_wait, 0);
    }

    #[test]
    fn carried_digger_goes_to_grave_once_bag_rests() {
        let mut game = playing(GameMode::OnePlayer);
        let first = DiggerId::ALL[0];
        let bag = BagId::ALL[1];
        *bag.of_mut(&mut game.sim.bags) = crate::sim::Bag {
            exists: true,
            position: Px::new(152, 177),
            direction: None,
            ..crate::sim::Bag::ZEROED
        };
        game.sim.kill_digger(first, DeathStage::Bag, bag);
        steps(&mut game, 1);
        let digger = first.of(&game.sim.diggers);
        // Held at the bag's top + 6 = 183 for the draw, then raised by 6.
        assert_eq!(digger.death_stage(), Some(DeathStage::Grave));
        assert_eq!(digger.death_timer(), GRAVE_WAIT);
        assert_eq!(digger.position(), Px::new(152, 177));
        assert_eq!(
            game.gfx.sprites().position(first.sprite()),
            Px::new(152, 183)
        );
        assert_eq!(
            game.gfx.sprites().image(first.sprite()),
            ImageId::DIGGER_DEAD
        );
        assert_eq!(
            game.state().penalty().count_of(PenaltySite::CarriedDigger),
            1
        );
        assert!(game.drain_sound().any(|s| s == SoundCommand::DiggerDeath));
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
