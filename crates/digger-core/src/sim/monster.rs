//! Monsters: slots, spawning, decisions, moves, transformations and deaths
//! (design D1; monsters spec; research 02 §4).
//!
//! Six slots are updated in slot order on every tick. A slot's monster is
//! a nobbin, which follows tunnels, or a hobbin, which digs its own. A
//! living monster without delay makes one move, and a nobbin sometimes a
//! second; at every cell corner it decides where to go next. A monster
//! squashed by a falling bag stays in its slot, dead, until the bag stops.
//!
//! The slot record keeps every variable of the upstream state hash at the
//! original's granularity, stale values included (design D5). Only this
//! module draws random numbers (design D7).

use crate::assets::ImageId;
use crate::geom::Px;
use crate::sound_cmd::SoundCommand;
use crate::sprite::{Collisions, SpriteClass};

use super::digger::{AnimFrame, at_limit, hit_emerald, step};
use super::draw::{self, Carve};
use super::geom::{CellPos, Direction, Located, locate, reverse_of};
use super::ids::{BagId, DiggerId, MonsterSlot};
use super::level::Difficulty;
use super::penalty::{Penalty, PenaltySite};
use super::score::{self, ScoreEvent, ScoreId};
use super::{Ctx, DeathStage, Sim};

/// The cell a monster spawns in: the top-right corner, (14, 0).
const SPAWN_CELL: (u8, u8) = (14, 0);
/// Ticks from a round's start before the spawn schedule first acts, so the
/// first monster appears on the 11th tick (research 02 §4.2).
const FIRST_SPAWN_DELAY: u16 = 10;
/// Move decisions for which a new monster stands still (research 02 §4.2,
/// `stime`).
const SPAWN_STANDSTILL: u8 = 5;
/// The hobbin counter stops rising here (research 02 §4.4 step J).
const HOBBIN_COUNTER_CAP: u16 = 100;
/// A hobbin removes the bags it touches once its counter is above this,
/// from its second move as a hobbin on (research 02 §4.4 step M).
const HOBBIN_EATS_BAGS_ABOVE: u16 = 1;
/// A dead monster less than this many pixels below its bag's top is set
/// level with it (research 02 §4.7, quirk Q21).
const SNAP_RANGE: i32 = 6;
/// Ticks a squashed monster stays after its bag has stopped, besides the
/// tick of its removal (research 02 §4.7, `dtime`).
const END_DELAY: u8 = 1;

/// The spawn gap at difficulty `d`: 45 − 2d ticks, 43 down to 25 (monsters
/// spec, "Level parameters"; research 02 §4.2, §7).
pub(crate) const fn spawn_gap(d: Difficulty) -> u16 {
    45 - 2 * d.get() as u16
}

/// Monsters per round at difficulty `d`: d + 5, 6 to 15 (research 02 §4.2,
/// §7).
pub(crate) const fn monsters_per_round(d: Difficulty) -> u16 {
    d.get() as u16 + 5
}

/// The most monster slots occupied at once at difficulty `d`: 3 at 1, 4 at
/// 2 to 7, 5 from 8 on (research 02 §4.2, §7).
pub(crate) const fn max_on_screen(d: Difficulty) -> u8 {
    match d.get() {
        1 => 3,
        2..=7 => 4,
        _ => 5,
    }
}

/// A nobbin whose counter is above this becomes a hobbin: 10 − d (monsters
/// spec, "Nobbin to hobbin"; research 02 §4.3, §4.5, §7).
pub(crate) const fn hobbin_threshold(d: Difficulty) -> u16 {
    10 - d.get() as u16
}

/// A hobbin whose counter is above this turns back into a nobbin at its
/// next decision: 30 + 2d (monsters spec, "Hobbin to nobbin"; research 02
/// §4.4, §4.5, §7).
pub(crate) const fn nobbin_threshold(d: Difficulty) -> u16 {
    30 + 2 * d.get() as u16
}

/// What became of a dead monster (`death`), with the original's numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MonsterDeath {
    /// No death recorded in this round (0).
    #[default]
    None,
    /// Squashed and carried by a falling bag (1).
    Carried,
    /// The bag has stopped; the final delay runs (4).
    Ending,
}

impl MonsterDeath {
    /// The original's number of the stage: 0, 1 or 4.
    pub const fn number(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Carried => 1,
            Self::Ending => 4,
        }
    }
}

/// One monster slot (`mondat[]` with its object).
///
/// The values stay in the slot after the monster is removed, until the
/// next round start clears every slot (design D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags of the state hash, kept at the original's granularity (design D5)"
)]
pub struct Monster {
    /// Whether the slot is in use (`flag`).
    pub(crate) occupied: bool,
    /// Whether a monster was spawned into the slot this round, so that its
    /// object exists (`mop != NULL`).
    pub(crate) spawned: bool,
    /// Whether the monster is alive (`mop.alive`).
    pub(crate) alive: bool,
    /// Whether the monster is dead and still shown (`mop.zombie`; not
    /// hashed).
    pub(crate) dead: bool,
    /// Whether the monster is a nobbin rather than a hobbin (`mop.nobf`).
    pub(crate) nobbin: bool,
    /// The position (`mop.pos.x`, `mop.pos.y`).
    pub(crate) position: Px,
    /// The way a hobbin faces, after its last horizontal move
    /// (`mop.pos.dir`): left or right.
    pub(crate) facing: Direction,
    /// The cell and remainders as stored at the end of the last move (`h`,
    /// `v`, `xr`, `yr`); decisions read these.
    pub(crate) located: Located,
    /// The direction of movement (`dir`), or none.
    pub(crate) direction: Option<Direction>,
    /// Ticks to wait before the next move (`t`).
    pub(crate) delay: u16,
    /// The transformation counter (`hnt`).
    pub(crate) counter: u16,
    /// What became of the monster after a squash (`death`).
    pub(crate) death: MonsterDeath,
    /// The bag that squashed the monster (`bag`).
    pub(crate) death_bag: BagId,
    /// The final delay after the bag stopped (`dtime`).
    pub(crate) death_delay: u8,
    /// Move decisions still to stand still for (`stime`).
    pub(crate) standstill: u8,
    /// The digger record the monster chases (`chase`).
    pub(crate) chase: DiggerId,
    /// The sprite's animation counter (`monspr`, `monspd`; not hashed).
    pub(crate) frame: AnimFrame,
}

impl Default for Monster {
    fn default() -> Self {
        Self::ZEROED
    }
}

impl Monster {
    /// A cleared slot: the original's zero-filled record, in which
    /// direction 0 is right (research 02 §4.2, `initmonsters`).
    pub(crate) const ZEROED: Self = Self {
        occupied: false,
        spawned: false,
        alive: false,
        dead: false,
        nobbin: false,
        position: Px::new(0, 0),
        facing: Direction::Right,
        located: Located {
            cell: match CellPos::new(0, 0) {
                Some(cell) => cell,
                None => unreachable!(),
            },
            rem_x: 0,
            rem_y: 0,
        },
        direction: Some(Direction::Right),
        delay: 0,
        counter: 0,
        death: MonsterDeath::None,
        death_bag: BagId::ALL[0],
        death_delay: 0,
        standstill: 0,
        chase: DiggerId::ALL[0],
        frame: AnimFrame::START,
    };

    /// A new nobbin chasing `chase`, in the top-right cell heading left,
    /// with no delay, a counter of 0 and the spawn standstill (monsters
    /// spec, "Monster slots and spawn state"; research 02 §4.2,
    /// `createmonster`). The death fields keep the slot's values.
    const fn new_nobbin(slot: &Self, chase: DiggerId) -> Self {
        let Some(cell) = CellPos::new(SPAWN_CELL.0, SPAWN_CELL.1) else {
            unreachable!()
        };
        Self {
            occupied: true,
            spawned: true,
            alive: true,
            dead: false,
            nobbin: true,
            position: cell.to_px(),
            facing: Direction::Left,
            located: Located {
                cell,
                rem_x: 0,
                rem_y: 0,
            },
            direction: Some(Direction::Left),
            delay: 0,
            counter: 0,
            death: slot.death,
            death_bag: slot.death_bag,
            death_delay: slot.death_delay,
            standstill: SPAWN_STANDSTILL,
            chase,
            frame: AnimFrame::START,
        }
    }

    /// Whether the slot is in use (hashed as `flag`).
    pub const fn occupied(&self) -> bool {
        self.occupied
    }

    /// Whether a monster was spawned into the slot this round, so that the
    /// hash covers its object's values (position, facing, alive, nobbin).
    pub const fn spawned(&self) -> bool {
        self.spawned
    }

    /// Whether the monster is alive (hashed as `alive`).
    pub const fn alive(&self) -> bool {
        self.alive
    }

    /// Whether the monster is dead but still shown, carried by a bag or in
    /// its final delay.
    pub const fn dead(&self) -> bool {
        self.dead
    }

    /// Whether the monster is a nobbin (hashed as `nobf`).
    pub const fn nobbin(&self) -> bool {
        self.nobbin
    }

    /// The position (hashed as `x`, `y`).
    pub const fn position(&self) -> Px {
        self.position
    }

    /// The way a hobbin faces (hashed as `pos.dir`): left or right.
    pub const fn facing(&self) -> Direction {
        self.facing
    }

    /// The cell and remainders as stored at the end of the last move
    /// (hashed as `h`, `v`, `xr`, `yr`).
    pub const fn located(&self) -> Located {
        self.located
    }

    /// The direction of movement (hashed as `dir`), or none.
    pub const fn direction(&self) -> Option<Direction> {
        self.direction
    }

    /// Ticks to wait before the next move (hashed as `t`).
    pub const fn delay(&self) -> u16 {
        self.delay
    }

    /// The transformation counter (hashed as `hnt`).
    pub const fn counter(&self) -> u16 {
        self.counter
    }

    /// What became of the monster after a squash (hashed as `death`).
    pub const fn death(&self) -> MonsterDeath {
        self.death
    }

    /// The bag that squashed the monster (hashed as `bag`).
    pub const fn death_bag(&self) -> BagId {
        self.death_bag
    }

    /// The final delay after the bag stopped (hashed as `dtime`).
    pub const fn death_delay(&self) -> u8 {
        self.death_delay
    }

    /// Move decisions still to stand still for (hashed as `stime`).
    pub const fn standstill(&self) -> u8 {
        self.standstill
    }

    /// The digger record the monster chases (hashed as `chase`).
    pub const fn chase(&self) -> DiggerId {
        self.chase
    }

    /// Raises the delay by one tick.
    const fn add_delay(&mut self) {
        self.delay = self.delay.saturating_add(1);
    }

    /// Raises the transformation counter by one.
    const fn count(&mut self) {
        self.counter = self.counter.saturating_add(1);
    }
}

/// The image of `monster`'s sprite showing animation frame `frame`, or
/// `None` for a slot with neither a living nor a dead monster, whose
/// sprite keeps its image (research 02 §2, `monster_obj_updspr`).
///
/// **Quirk Q30:** a dead left-facing hobbin shows image 80, which is 14
/// pixels tall instead of 15, so it collides one pixel shorter.
fn sprite_for(monster: &Monster, frame: u8) -> Option<ImageId> {
    let image = if monster.alive {
        let first = match (monster.nobbin, monster.facing) {
            (true, _) => ImageId::NOBBIN_1,
            (false, Direction::Right) => ImageId::HOBBIN_RIGHT_1,
            (false, _) => ImageId::HOBBIN_LEFT_1,
        };
        ImageId::new(first.number() + frame.min(2))?
    } else if monster.dead {
        match (monster.nobbin, monster.facing) {
            (true, _) => ImageId::NOBBIN_DEAD,
            (false, Direction::Right) => ImageId::HOBBIN_RIGHT_DEAD,
            (false, _) => ImageId::HOBBIN_LEFT_DEAD,
        }
    } else {
        return None;
    };
    Some(image)
}

/// The four directions a monster at `at` prefers at a decision, with
/// `target` the position of the digger it chases, before the no-reverse
/// rule (monsters spec, "Direction priorities at cell corners"; research
/// 02 §4.4 steps B3, B4).
///
/// When the target is further away vertically than horizontally the
/// order is vertical toward, horizontal toward, horizontal away, vertical
/// away; otherwise horizontal toward, vertical toward, vertical away,
/// horizontal away. "Toward" on an equal coordinate is down or right. In
/// bonus mode the order is reversed.
pub(crate) fn priorities(at: Px, target: Px, fleeing: bool) -> [Direction; 4] {
    let vertical = if target.y < at.y {
        Direction::Up
    } else {
        Direction::Down
    };
    let horizontal = if target.x < at.x {
        Direction::Left
    } else {
        Direction::Right
    };
    let mut list = if (target.y - at.y).abs() > (target.x - at.x).abs() {
        [
            vertical,
            horizontal,
            horizontal.reverse(),
            vertical.reverse(),
        ]
    } else {
        [
            horizontal,
            vertical,
            vertical.reverse(),
            horizontal.reverse(),
        ]
    };
    if fleeing {
        list.reverse();
    }
    list
}

/// The no-reverse rule: the reverse of the current direction `reverse`
/// moves to last place, the others keeping their order (research 02 §4.4
/// step B5).
pub(crate) fn reverse_last(list: [Direction; 4], reverse: Option<Direction>) -> [Direction; 4] {
    let Some(reverse) = reverse else {
        return list;
    };
    let mut out = list;
    let mut next = 0;
    for direction in list.into_iter().filter(|&d| d != reverse) {
        if let Some(slot) = out.get_mut(next) {
            *slot = direction;
        }
        next += 1;
    }
    if next < out.len()
        && let Some(last) = out.last_mut()
    {
        *last = reverse;
    }
    out
}

/// The random element of a decision: with a draw of 1 below difficulty 6
/// the first and third priorities change places (monsters spec, "Random
/// element in the choice"; research 02 §4.4 step B6).
///
/// **Quirk Q03:** the caller draws the number at every difficulty; from 6
/// on it is discarded here.
pub(crate) const fn apply_noise(
    mut list: [Direction; 4],
    draw: i16,
    d: Difficulty,
) -> [Direction; 4] {
    if draw == 1 && d.get() < 6 {
        list.swap(0, 2);
    }
    list
}

/// The monster slots with the round's spawn schedule (`mondat[]` and the
/// globals of `monster.c`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Monsters {
    /// The six slots.
    pub(crate) slots: [Monster; MonsterSlot::COUNT],
    /// Monsters spawned this round (`nextmonster`).
    spawned: u16,
    /// Monsters due this round (`totalmonsters`).
    total: u16,
    /// The most slots occupied at once (`maxmononscr`).
    max_on_screen: u8,
    /// Ticks until the schedule next acts (`nextmontime`).
    spawn_timer: u16,
    /// The spawn gap (`mongaptime`).
    spawn_gap: u16,
    /// The unit the next monster chases (`chase`).
    next_chase: u8,
    /// Whether the bonus is still to appear this round (`unbonusflag`).
    bonus_pending: bool,
}

impl Default for Monsters {
    fn default() -> Self {
        Self {
            slots: [Monster::ZEROED; MonsterSlot::COUNT],
            spawned: 0,
            total: 0,
            max_on_screen: 0,
            spawn_timer: 0,
            spawn_gap: 0,
            next_chase: 0,
            bonus_pending: false,
        }
    }
}

impl Monsters {
    /// Every slot cleared and the schedule set up for a round at
    /// difficulty `d` (monsters spec, "Level parameters"; research 02 §4.2,
    /// `initmonsters`).
    pub(crate) fn for_round(d: Difficulty) -> Self {
        Self {
            slots: [Monster::ZEROED; MonsterSlot::COUNT],
            spawned: 0,
            total: monsters_per_round(d),
            max_on_screen: max_on_screen(d),
            spawn_timer: FIRST_SPAWN_DELAY,
            spawn_gap: spawn_gap(d),
            next_chase: 0,
            bonus_pending: true,
        }
    }

    /// The monster in `slot`.
    pub fn slot(&self, slot: MonsterSlot) -> &Monster {
        slot.of(&self.slots)
    }

    /// Monsters spawned this round (hashed as `nextmonster`).
    pub const fn spawned(&self) -> u16 {
        self.spawned
    }

    /// Monsters due this round (hashed as `totalmonsters`).
    pub const fn total(&self) -> u16 {
        self.total
    }

    /// The most slots occupied at once (hashed as `maxmononscr`).
    pub const fn max_on_screen(&self) -> u8 {
        self.max_on_screen
    }

    /// Ticks until the schedule next acts (hashed as `nextmontime`).
    pub const fn spawn_timer(&self) -> u16 {
        self.spawn_timer
    }

    /// The spawn gap (hashed as `mongaptime`).
    pub const fn spawn_gap(&self) -> u16 {
        self.spawn_gap
    }

    /// The unit the next monster chases, 0 or 1 (hashed as `chase`).
    pub const fn next_chase(&self) -> u8 {
        self.next_chase
    }

    /// Whether the bonus is still to appear this round (hashed as
    /// `unbonusflag`).
    pub const fn bonus_pending(&self) -> bool {
        self.bonus_pending
    }

    /// The occupied slots; a dead monster still carried counts.
    pub fn on_screen(&self) -> u16 {
        let occupied = self.slots.iter().filter(|m| m.occupied).count();
        u16::try_from(occupied).unwrap_or(u16::MAX)
    }

    /// The monsters remaining in the round: the occupied slots plus those
    /// not yet spawned (monsters spec, "Monsters remaining"; research 02
    /// §4.2, `monleft`).
    pub fn left(&self) -> u16 {
        (self.on_screen() + self.total).saturating_sub(self.spawned)
    }

    /// Delays the slots the penalty counter's overflow names by one tick,
    /// occupied or not (scoring spec, "Penalty overflow delays monsters";
    /// research 02 §6, `incmont`).
    ///
    /// **Quirk Q02:** slot 0 is never delayed; see
    /// [`Penalty::overflow_slots`].
    pub(crate) fn apply_penalty_overflow(&mut self, penalty: Penalty) {
        for slot in penalty.overflow_slots() {
            slot.of_mut(&mut self.slots).add_delay();
        }
    }
}

impl Sim {
    /// The current player's difficulty index.
    fn difficulty(&self) -> Difficulty {
        self.current_level().difficulty()
    }

    /// The digger record a monster chasing `chase` heads for: `chase`, or
    /// the current player's other record when `chase` is not alive
    /// (monsters spec, "Chase target"; research 02 §4.4 step B2).
    fn chase_target(&self, chase: DiggerId) -> DiggerId {
        if chase.of(&self.diggers).alive {
            return chase;
        }
        // The original's 2 × player + (diggers − 1) − chase: the partner
        // with two diggers, the same record with one.
        let other = 2 * i32::from(self.turn.number()) + i32::from(self.cfg.mode.diggers())
            - 1
            - i32::from(chase.number());
        u8::try_from(other)
            .ok()
            .and_then(DiggerId::new)
            .unwrap_or(chase)
    }
}

impl Ctx<'_> {
    /// Clears every slot and sets up the round's schedule from the current
    /// player's difficulty, as every round start does after placing the
    /// diggers (research 02 §4.2, `initmonsters`); the monster sprites get
    /// the image they are reset to (`initmbspr`).
    pub(crate) fn reset_monsters(&mut self) {
        self.sim.monsters = Monsters::for_round(self.sim.difficulty());
        for slot in MonsterSlot::ALL {
            self.gfx.set_pending(slot.sprite(), ImageId::NOBBIN_3);
        }
    }

    /// Resets the monster sprites to their initial image, disabled, as the
    /// full level draw does (research 02 §2, `creatembspr`).
    pub(crate) fn reset_monster_sprites(&mut self) {
        for slot in MonsterSlot::ALL {
            self.gfx.reset(slot.sprite(), ImageId::NOBBIN_3);
        }
    }

    /// Erases the sprite of every occupied slot, as the round end does
    /// (research 02 §4, `erasemonsters`).
    pub(crate) fn erase_monsters(&mut self) {
        for slot in MonsterSlot::ALL {
            if slot.of(&self.sim.monsters.slots).occupied {
                self.gfx.erase(slot.sprite());
            }
        }
    }

    /// The monsters' part of a tick: the spawn schedule, then every
    /// occupied slot in order (monsters spec, "Spawn schedule", "Movement
    /// step and move scheduling", "Random double move for nobbins";
    /// research 02 §4.2, §4.3, `domonsters`).
    ///
    /// In each slot a nobbin whose counter is above 10 − L first becomes a
    /// hobbin. Then a living monster with no delay makes a move and one
    /// random number is drawn with modulus 15 − L; a draw of 0 gives a
    /// nobbin that is still alive a second move. A living monster with a
    /// delay counts it down instead, and a dead one runs its death.
    ///
    /// **Quirk Q29:** the transformation test also runs for a dead monster
    /// still carried by a bag. **Quirk Q03:** the double-move draw is taken
    /// for hobbins too, and acted on only for nobbins.
    pub(crate) fn update_monsters(&mut self) {
        self.run_spawn_schedule();
        let d = self.sim.difficulty();
        for slot in MonsterSlot::ALL {
            let monster = *slot.of(&self.sim.monsters.slots);
            if !monster.occupied {
                continue;
            }
            if monster.counter > hobbin_threshold(d) && monster.nobbin {
                self.mutate(slot);
                slot.of_mut(&mut self.sim.monsters.slots).counter = 0;
            }
            let monster = *slot.of(&self.sim.monsters.slots);
            if !monster.alive {
                self.monster_die(slot);
            } else if monster.delay == 0 {
                self.monster_move(slot);
                let draw = self.sim.rng.draw(d.double_move_modulus());
                let monster = slot.of(&self.sim.monsters.slots);
                if draw == 0 && monster.nobbin && monster.alive {
                    self.monster_move(slot);
                }
            } else {
                slot.of_mut(&mut self.sim.monsters.slots).delay -= 1;
            }
        }
    }

    /// The head of the monsters' tick: the spawn timer counts down, and at
    /// 0 a monster spawns when fewer than the round's total have spawned,
    /// fewer slots are occupied than the on-screen limit, a digger is alive
    /// and bonus mode is off (monsters spec, "Spawn schedule"; research 02
    /// §4.2).
    ///
    /// From the same point the bonus appears, once per round, when every
    /// monster of the round has spawned, the timer is still 0 (it was not
    /// just restarted by a spawn) and a digger is alive (digger spec, "Bonus
    /// appearance"; research 02 §3.8).
    fn run_spawn_schedule(&mut self) {
        let monsters = &mut self.sim.monsters;
        if monsters.spawn_timer > 0 {
            monsters.spawn_timer -= 1;
            return;
        }
        if monsters.spawned < monsters.total
            && monsters.on_screen() < u16::from(monsters.max_on_screen)
            && self.sim.any_alive()
            && !self.sim.bonus_mode
        {
            self.spawn_monster();
        }
        let monsters = &self.sim.monsters;
        if monsters.bonus_pending
            && monsters.spawned == monsters.total
            && monsters.spawn_timer == 0
            && self.sim.any_alive()
        {
            self.sim.monsters.bonus_pending = false;
            self.show_bonus();
        }
    }

    /// Spawns a nobbin into the lowest free slot, chasing the next unit's
    /// record, and places its sprite (research 02 §4.2, `createmonster`).
    fn spawn_monster(&mut self) {
        let Some(slot) = MonsterSlot::ALL
            .into_iter()
            .find(|slot| !slot.of(&self.sim.monsters.slots).occupied)
        else {
            return;
        };
        let monsters = &mut self.sim.monsters;
        let unit = monsters.next_chase;
        let chase =
            DiggerId::new(unit.saturating_add(self.sim.turn.number())).unwrap_or_else(|| {
                debug_assert!(false, "alternating players and two diggers never combine");
                DiggerId::ALL[0]
            });
        let diggers = self.sim.cfg.mode.diggers().max(1);
        monsters.next_chase = (unit + 1) % diggers;
        monsters.spawned = monsters.spawned.saturating_add(1);
        monsters.spawn_timer = monsters.spawn_gap;
        let monster = slot.of_mut(&mut monsters.slots);
        *monster = Monster::new_nobbin(monster, chase);
        let monster = *monster;
        if let Some(image) = sprite_for(&monster, monster.frame.frame()) {
            self.gfx.place(slot.sprite(), image, monster.position);
        }
    }

    /// Draws `slot`'s monster at its position and returns the collisions
    /// of the draw: a living one advancing its animation, a dead one
    /// without; an empty slot draws nothing and reports no collisions
    /// (research 02 §2, `animate`).
    fn draw_monster(&mut self, slot: MonsterSlot) -> Collisions {
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        let frame = if monster.alive {
            monster.frame.advance()
        } else {
            monster.frame.frame()
        };
        let monster = *monster;
        match sprite_for(&monster, frame) {
            Some(image) => self.gfx.move_monster(slot, image, monster.position),
            None => Collisions::EMPTY,
        }
    }

    /// Changes `slot`'s monster between nobbin and hobbin and redraws it;
    /// the collisions of that draw are not used (research 02 §2, `mutate`).
    fn mutate(&mut self, slot: MonsterSlot) {
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        monster.nobbin = !monster.nobbin;
        let monster = *monster;
        if let Some(image) = sprite_for(&monster, monster.frame.frame()) {
            self.gfx.move_monster(slot, image, monster.position);
        }
    }

    /// The decision of a monster on a cell corner (monsters spec,
    /// "Direction priorities at cell corners", "Random element in the
    /// choice", "Nobbin passability and choice", "Hobbin movement";
    /// research 02 §4.4 step B).
    ///
    /// A hobbin whose counter is above 30 + 2L first becomes a nobbin.
    /// Then the priorities toward (or, in bonus mode, away from) the target
    /// are set, the reverse of the current direction is moved last, a
    /// random number is drawn with modulus L + 5, and the direction is
    /// chosen; a change of direction costs a tick of delay.
    ///
    /// **Quirk Q03:** the noise draw is taken at every difficulty.
    /// **Quirk Q28:** a nobbin with no passable priority takes the reverse
    /// of its direction (none stays none); a hobbin takes its first
    /// priority even off the playfield, where it then stands still.
    fn decide(&mut self, slot: MonsterSlot, d: Difficulty) {
        let monster = *slot.of(&self.sim.monsters.slots);
        if monster.counter > nobbin_threshold(d) && !monster.nobbin {
            slot.of_mut(&mut self.sim.monsters.slots).counter = 0;
            self.mutate(slot);
        }
        let target = self.sim.chase_target(monster.chase);
        let target_at = target.of(&self.sim.diggers).position;
        let reverse = reverse_of(monster.direction);
        let list = reverse_last(
            priorities(monster.position, target_at, self.sim.bonus_mode),
            reverse,
        );
        let noise = self.sim.rng.draw(d.noise_modulus());
        let list = apply_noise(list, noise, d);
        let monster = *slot.of(&self.sim.monsters.slots);
        let chosen = if monster.nobbin {
            let from = monster.located.cell;
            list.into_iter()
                .find(|&dir| self.sim.field.is_passable(from, dir))
                .or(reverse)
        } else {
            Some(list[0])
        };
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if monster.direction != chosen {
            monster.add_delay();
        }
        monster.direction = chosen;
    }

    /// One move of a living monster (research 02 §4.4, `monai`).
    ///
    /// On a cell corner it decides first. At the playfield edge its
    /// direction becomes none. A hobbin faces the way of a horizontal
    /// move, digs (charged even without a direction, quirk Q38), carves
    /// its tunnel and destroys an emerald it reaches without score. The
    /// monster steps, unless no digger is alive or it is in its spawn
    /// standstill, which undo the step. It is drawn, and what the draw
    /// touched is met: other monsters, bags and diggers. A bag that blocks
    /// it puts it back and redraws it. Finally its cell and remainders are
    /// stored.
    ///
    /// **Quirk Q27:** an undone step still has every side effect: the
    /// decision and its draw, the digging, the emerald, delays and the
    /// collisions of the draw, made where the monster is held. A bag that
    /// blocks the move sends it back only after the draw, so those
    /// collisions are met at the attempted position. **Quirk Q26:**
    /// outside bonus mode it is this move, never the digger's, that kills
    /// a digger it overlaps.
    fn monster_move(&mut self, slot: MonsterSlot) {
        let d = self.sim.difficulty();
        let before = *slot.of(&self.sim.monsters.slots);
        let from = before.position;
        if before.located.rem_x == 0 && before.located.rem_y == 0 {
            self.decide(slot, d);
        }
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if monster.direction.is_some_and(|dir| at_limit(from, dir)) {
            monster.direction = None;
        }
        let direction = monster.direction;
        if let Some(dir) = direction.filter(|dir| dir.is_horizontal()) {
            monster.facing = dir;
        }
        let hobbin = !monster.nobbin;
        if hobbin {
            // Quirk Q38: charged even with no direction.
            self.sim.field.dig(from, direction, &mut self.sim.penalty);
        }
        let mut to = from;
        if let Some(dir) = direction {
            if hobbin {
                draw::carve(self.gfx, Carve::for_direction(dir), from);
            }
            to = step(from, dir);
            if hobbin {
                // Quirk Q38: the emerald is destroyed and nobody scores.
                let player = self.sim.turn;
                hit_emerald(
                    self.gfx,
                    &mut self.sim.emeralds,
                    &mut self.sim.penalty,
                    player,
                    to,
                    dir,
                );
            }
        }
        // Quirk Q27: the step is undone while no digger is alive and
        // during the standstill; everything above has happened.
        if !self.sim.any_alive() {
            to = from;
        }
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if monster.standstill != 0 {
            monster.standstill -= 1;
            to = from;
        }
        if hobbin && monster.counter < HOBBIN_COUNTER_CAP {
            monster.count();
        }
        monster.position = to;

        let hits = self.draw_monster(slot);
        self.sim.penalty.charge(PenaltySite::MonsterDraw);
        self.meet_monsters(slot, hits);
        let pushed = self.meet_bags_as_monster(slot, hits);
        let monster = *slot.of(&self.sim.monsters.slots);
        if monster.nobbin && hits.first(SpriteClass::Monster).is_some() && self.sim.any_alive() {
            slot.of_mut(&mut self.sim.monsters.slots).count();
        }
        if !pushed {
            self.blocked_by_bag(slot, from);
        }
        self.meet_diggers_as_monster(slot, hits);
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if let Some(located) = locate(monster.position) {
            monster.located = located;
        } else {
            debug_assert!(false, "a monster stays within the playfield");
        }
    }

    /// The monsters a move's draw touched (monsters spec, "Monsters meeting
    /// monsters", "Sources of delay"; research 02 §4.4 step L): the mover
    /// is delayed by one tick, and each overlapped monster heading the same
    /// way, with neither in its standstill, is turned around; each charges
    /// the penalty counter once.
    ///
    /// **Quirk Q28:** the overlapped monster is reversed even between
    /// cells.
    fn meet_monsters(&mut self, slot: MonsterSlot, hits: Collisions) {
        if hits.first(SpriteClass::Monster).is_none() {
            return;
        }
        slot.of_mut(&mut self.sim.monsters.slots).add_delay();
        for sprite in hits.of(SpriteClass::Monster) {
            let Some(other) = MonsterSlot::from_sprite(sprite) else {
                continue;
            };
            let mover = *slot.of(&self.sim.monsters.slots);
            let met = other.of_mut(&mut self.sim.monsters.slots);
            if mover.direction == met.direction && met.standstill == 0 && mover.standstill == 0 {
                met.direction = reverse_of(met.direction);
            }
            self.sim.penalty.charge(PenaltySite::MonsterOverlap);
        }
    }

    /// The bags a move's draw touched, and whether the move may stand
    /// (monsters spec, "Monsters meeting bags", "Sources of delay";
    /// research 02 §4.4 step M).
    ///
    /// Touching an existing bag costs a tick of delay. A left or right move
    /// pushes every touched bag, as a digger does, and costs one more tick;
    /// any other move collects gold and is blocked by an unbroken bag. Gold
    /// taken by the monster clears its delay. A hobbin whose counter is
    /// above 1 then removes every bag and gold it touched.
    fn meet_bags_as_monster(&mut self, slot: MonsterSlot, hits: Collisions) -> bool {
        let met = hits.of(SpriteClass::Bag).any(|sprite| {
            BagId::from_sprite(sprite).is_some_and(|id| id.of(&self.sim.bags).exists())
        });
        if !met {
            return true;
        }
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        monster.add_delay();
        let direction = monster.direction;
        self.sim.monster_got_gold = false;
        let pushed = match direction {
            Some(dir) if dir.is_horizontal() => {
                let pushed = self.push_bags(dir, hits);
                slot.of_mut(&mut self.sim.monsters.slots).add_delay();
                pushed
            }
            _ => self.push_bags_vertically(hits),
        };
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if self.sim.monster_got_gold {
            monster.delay = 0;
        }
        if !monster.nobbin && monster.counter > HOBBIN_EATS_BAGS_ABOVE {
            for sprite in hits.of(SpriteClass::Bag) {
                if let Some(id) = BagId::from_sprite(sprite) {
                    self.remove_bag(id);
                }
            }
        }
        pushed
    }

    /// A move a bag blocked (monsters spec, "Monsters meeting bags";
    /// research 02 §4.4 step O): the monster goes back to `from` and is
    /// redrawn; a nobbin counts towards becoming a hobbin and, when it was
    /// moving up or down, turns back.
    fn blocked_by_bag(&mut self, slot: MonsterSlot, from: Px) {
        slot.of_mut(&mut self.sim.monsters.slots).position = from;
        self.draw_monster(slot);
        self.sim.penalty.charge(PenaltySite::MonsterBlockedRedraw);
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if monster.nobbin {
            monster.count();
            if monster.direction.is_some_and(Direction::is_vertical) {
                monster.direction = reverse_of(monster.direction);
            }
        }
    }

    /// The diggers a move's draw touched, at the position the monster
    /// attempted, while any digger is alive (monsters spec, "Monsters
    /// killing diggers", "Being eaten in bonus mode"; research 02 §4.4
    /// step P).
    ///
    /// Outside bonus mode every overlapped living digger is killed by the
    /// monster (invincible ones are immune). In bonus mode the monster is
    /// removed once, every overlapped living digger scores its next eating
    /// step, and the eat cue sounds once.
    fn meet_diggers_as_monster(&mut self, slot: MonsterSlot, hits: Collisions) {
        if hits.first(SpriteClass::Digger).is_none() || !self.sim.any_alive() {
            return;
        }
        let bonus_mode = self.sim.bonus_mode;
        let mut eaten = false;
        for sprite in hits.of(SpriteClass::Digger) {
            let Some(unit) = DiggerId::from_sprite(sprite) else {
                continue;
            };
            let record = self.sim.record_of_unit(unit);
            if !record.of(&self.sim.diggers).alive {
                continue;
            }
            if bonus_mode {
                if !eaten {
                    self.remove_monster(slot);
                    eaten = true;
                }
                self.eat_monster(unit, record);
            } else {
                self.sim
                    .kill_digger(record, DeathStage::Monster, BagId::ALL[0]);
            }
        }
        if eaten {
            self.sound.push(SoundCommand::EatMonster);
        }
    }

    /// Scores a monster eaten by `unit`'s record at its eating step, 200 ×
    /// its multiplier, and doubles the multiplier (research 02 §3.8,
    /// `sceatm`).
    pub(crate) fn eat_monster(&mut self, unit: DiggerId, record: DiggerId) {
        let multiplier = record.of(&self.sim.diggers).eat_multiplier;
        let score = ScoreId::credited(self.sim.turn, unit);
        self.add_score(score, ScoreEvent::EatMonster { multiplier });
        record.of_mut(&mut self.sim.diggers).bump_eat_multiplier();
    }

    /// One tick of a dead monster (monsters spec, "Death under a falling
    /// bag"; research 02 §4.7, `mondie`).
    ///
    /// While carried it is set level with its bag's top when its own top
    /// is less than 6 pixels below it, drawn and charged; once the bag has
    /// stopped or is gone, a 1-tick delay starts. After that delay the
    /// monster is removed and the kill is scored.
    ///
    /// **Quirk Q21 (monster side):** the monster is set to the bag's top
    /// itself, without the 6 pixels a crushed digger is held below it.
    fn monster_die(&mut self, slot: MonsterSlot) {
        let monster = *slot.of(&self.sim.monsters.slots);
        match monster.death {
            MonsterDeath::Carried => {
                let bag = *monster.death_bag.of(&self.sim.bags);
                let top = bag.position().y;
                if top + SNAP_RANGE > monster.position.y {
                    slot.of_mut(&mut self.sim.monsters.slots).position.y = top;
                }
                self.draw_monster(slot);
                self.sim.penalty.charge(PenaltySite::CarriedMonster);
                if bag.has_stopped() {
                    let monster = slot.of_mut(&mut self.sim.monsters.slots);
                    monster.death_delay = END_DELAY;
                    monster.death = MonsterDeath::Ending;
                }
            }
            MonsterDeath::Ending => {
                if monster.death_delay != 0 {
                    slot.of_mut(&mut self.sim.monsters.slots).death_delay -= 1;
                } else {
                    self.remove_monster(slot);
                    self.score_bag_kill();
                }
            }
            MonsterDeath::None => {}
        }
    }

    /// The scoring of a monster killed by a bag: 250 for the current player
    /// with one digger, the two calls of the bag-kill crediting with two
    /// (scoring spec, "Bag kills with two diggers"; research 01 §6.1,
    /// `scorekill`, `scorekill2`).
    fn score_bag_kill(&mut self) {
        if self.sim.cfg.mode.diggers() == 2 {
            let lives = self.sim.diggers.map(|digger| digger.lives);
            for (id, points) in score::bag_kill_calls(lives) {
                let lives = Sim::record_of(id).of(&self.sim.diggers).lives;
                let effect = self.sim.scores.add(
                    id,
                    points,
                    lives,
                    &mut self.sim.penalty,
                    self.gfx.fb_mut(),
                );
                self.apply_bonus(id, effect);
            }
        } else {
            let score = ScoreId::credited(self.sim.turn, DiggerId::ALL[0]);
            self.add_score(score, ScoreEvent::BagKill);
        }
    }

    /// Squashes the monsters falling bag `bag`'s draw touched: each whose
    /// top is at or below the bag's top dies, is redrawn dead and is
    /// carried by the bag (monsters spec, "Death under a falling bag";
    /// research 02 §4.7, `squashmonsters`).
    pub(crate) fn squash_monsters(&mut self, bag: BagId, hits: Collisions) {
        let top = bag.of(&self.sim.bags).position().y;
        for sprite in hits.of(SpriteClass::Monster) {
            let Some(slot) = MonsterSlot::from_sprite(sprite) else {
                continue;
            };
            let monster = slot.of_mut(&mut self.sim.monsters.slots);
            if monster.position.y < top {
                continue;
            }
            monster.alive = false;
            monster.dead = true;
            let monster = *monster;
            if let Some(image) = sprite_for(&monster, monster.frame.frame()) {
                self.gfx.move_monster(slot, image, monster.position);
            }
            let monster = slot.of_mut(&mut self.sim.monsters.slots);
            monster.death = MonsterDeath::Carried;
            monster.death_bag = bag;
        }
    }

    /// Turns every monster in `column` that is heading up to head down, as
    /// a bag falling in that column does on each tick; the slots are not
    /// checked for use (monsters spec, "Frightened by falling bags";
    /// research 02 §4.6, `checkmonscared`).
    pub(crate) fn frighten_monsters(&mut self, column: u8) {
        for monster in &mut self.sim.monsters.slots {
            if monster.located.cell.h() == column && monster.direction == Some(Direction::Up) {
                monster.direction = Some(Direction::Down);
            }
        }
    }

    /// Removes the monster in `slot` if the slot is in use: the slot is
    /// freed, the monster dies and its sprite is erased (research 02 §4.7,
    /// `killmon`).
    ///
    /// **Quirk Q31:** in bonus mode every removal, whatever its cause,
    /// raises the round's total by one, so the monster comes back.
    pub(crate) fn remove_monster(&mut self, slot: MonsterSlot) {
        let monster = slot.of_mut(&mut self.sim.monsters.slots);
        if !monster.occupied {
            return;
        }
        monster.occupied = false;
        monster.alive = false;
        monster.dead = false;
        self.gfx.erase(slot.sprite());
        if self.sim.bonus_mode {
            let monsters = &mut self.sim.monsters;
            monsters.total = monsters.total.saturating_add(1);
        }
    }

    /// Puts a monster into `slot` at `at` as described, replacing whatever
    /// was there, and places its sprite, for the test kit.
    #[cfg(any(test, feature = "testkit"))]
    #[expect(
        clippy::too_many_arguments,
        reason = "the test kit's set-up fields, passed through"
    )]
    pub(crate) fn put_monster(
        &mut self,
        slot: MonsterSlot,
        at: Px,
        nobbin: bool,
        direction: Option<Direction>,
        delay: u16,
        standstill: u8,
        counter: u16,
    ) {
        let chase = DiggerId::ALL[0];
        let base = *slot.of(&self.sim.monsters.slots);
        let mut monster = Monster::new_nobbin(&base, chase);
        monster.nobbin = nobbin;
        monster.position = at;
        if let Some(located) = locate(at) {
            monster.located = located;
        }
        monster.direction = direction;
        if let Some(dir) = direction.filter(|dir| dir.is_horizontal()) {
            monster.facing = dir;
        }
        monster.delay = delay;
        monster.standstill = standstill;
        monster.counter = counter;
        *slot.of_mut(&mut self.sim.monsters.slots) = monster;
        self.gfx.erase(slot.sprite());
        if let Some(image) = sprite_for(&monster, 0) {
            self.gfx.place(slot.sprite(), image, at);
        }
    }

    /// Mutable access to the monster slots and schedule, for the test kit.
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) const fn monsters_mut(&mut self) -> &mut Monsters {
        &mut self.sim.monsters
    }
}

#[cfg(any(test, feature = "testkit"))]
impl Monsters {
    /// Sets the spawn timer, for the test kit.
    pub(crate) const fn set_spawn_timer(&mut self, ticks: u16) {
        self.spawn_timer = ticks;
    }

    /// Marks the round's bonus as having appeared, for the test kit.
    pub(crate) const fn clear_bonus_pending(&mut self) {
        self.bonus_pending = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::observer::NoopObserver;
    use crate::sim::testkit::{MonsterSetup, Run, Scenario};
    use crate::sim::{GameMode, GameRng, LevelNumber, Modulus};
    use Direction::{Down, Left, Right, Up};

    fn difficulty(n: u16) -> Difficulty {
        LevelNumber::new(n)
            .unwrap_or_else(|| unreachable!())
            .difficulty()
    }

    fn slot(n: u8) -> MonsterSlot {
        MonsterSlot::new(n).unwrap_or_else(|| unreachable!())
    }

    /// Runs `f` on a context over the run's game.
    fn with_ctx(run: &mut Run, f: impl FnOnce(&mut Ctx<'_>)) {
        let game = run.game_mut();
        let mut obs = NoopObserver;
        let mut cx = Ctx {
            sim: &mut game.sim,
            gfx: &mut game.gfx,
            sound: &mut game.sound,
            obs: &mut obs,
        };
        f(&mut cx);
    }

    /// Solid earth with a horizontal tunnel along row 4 from column
    /// `from` to column `to`, and the digger's start cell open.
    fn row_4(from: usize, to: usize) -> [String; 10] {
        let mut rows = [[b' '; 15]; 10];
        for h in from..=to {
            if let Some(c) = rows.get_mut(4).and_then(|row| row.get_mut(h)) {
                *c = b'H';
            }
        }
        rows.map(|row| String::from_utf8(row.to_vec()).unwrap_or_default())
    }

    /// A started one-player run at `level` on `rows`, with seed `seed`.
    fn started(rows: &[String; 10], level: i64, seed: u64) -> Run {
        let mut run = scenario(rows, level).seed(seed).build();
        run.start();
        run
    }

    fn scenario(rows: &[String; 10], level: i64) -> Scenario {
        Scenario::new(GameMode::OnePlayer)
            .rows(rows.each_ref().map(String::as_str))
            .start_level(level)
    }

    /// The first `n` draws of the round just started, with `moduli`.
    fn first_draws(run: &Run, moduli: &[u8]) -> Vec<i16> {
        let mut rng = GameRng::new(run.state().rng_state());
        moduli
            .iter()
            .map(|&m| rng.draw(Modulus::new(m).unwrap_or_else(|| unreachable!())))
            .collect()
    }

    fn place(run: &mut Run, n: u8, at: Px, setup: MonsterSetup) {
        assert_eq!(run.place_monster_as(slot(n), at, setup), Ok(()));
    }

    #[test]
    fn monster_parameter_table() {
        let gaps: Vec<u16> = (1..=10).map(|l| spawn_gap(difficulty(l))).collect();
        assert_eq!(gaps, [43, 41, 39, 37, 35, 33, 31, 29, 27, 25]);
        let totals: Vec<u16> = (1..=10)
            .map(|l| monsters_per_round(difficulty(l)))
            .collect();
        assert_eq!(totals, [6, 7, 8, 9, 10, 11, 12, 13, 14, 15]);
        let limits: Vec<u8> = (1..=10).map(|l| max_on_screen(difficulty(l))).collect();
        assert_eq!(limits, [3, 4, 4, 4, 4, 4, 4, 5, 5, 5]);
        // Beyond level 10 the difficulty, and so every value, stays.
        assert_eq!(spawn_gap(difficulty(1000)), 25);

        let round = Monsters::for_round(difficulty(1));
        assert_eq!(round.spawn_timer(), 10);
        assert_eq!(round.spawned(), 0);
        assert_eq!(round.total(), 6);
        assert_eq!(round.left(), 6);
        assert!(round.bonus_pending());
        assert!(round.slots.iter().all(|m| !m.occupied && !m.spawned));
    }

    #[test]
    fn priorities_target_mostly_below() {
        // 40 right of and 54 below a monster heading right.
        let at = Px::new(100, 54);
        let list = priorities(at, Px::new(140, 108), false);
        assert_eq!(list, [Down, Right, Left, Up]);
        assert_eq!(reverse_last(list, Some(Left)), [Down, Right, Up, Left]);
        // Mostly sideways: horizontal first, equal coordinates count as
        // down and right.
        assert_eq!(
            priorities(at, Px::new(40, 60), false),
            [Left, Down, Up, Right]
        );
        assert_eq!(priorities(at, at, false), [Right, Down, Up, Left]);
        assert_eq!(
            priorities(at, Px::new(100, 0), false),
            [Up, Right, Left, Down]
        );
    }

    #[test]
    fn reverse_direction_goes_last() {
        // The target directly behind a monster heading left: right comes
        // first by position and is tried last.
        let list = priorities(Px::new(100, 90), Px::new(200, 90), false);
        assert_eq!(list, [Right, Down, Up, Left]);
        assert_eq!(reverse_last(list, Some(Right)), [Down, Up, Left, Right]);
        for reverse in Direction::ALL {
            let out = reverse_last(list, Some(reverse));
            assert_eq!(out[3], reverse);
            let mut sorted = out.map(|d| d as u8);
            sorted.sort_unstable();
            assert_eq!(sorted, [0, 1, 2, 3], "a permutation");
        }
        // With no direction nothing moves.
        assert_eq!(reverse_last(list, None), list);
    }

    #[test]
    fn fleeing_reverses_priority_list() {
        let at = Px::new(100, 54);
        let target = Px::new(140, 108);
        let mut normal = priorities(at, target, false);
        normal.reverse();
        assert_eq!(priorities(at, target, true), normal);
        assert_eq!(priorities(at, target, true), [Up, Left, Right, Down]);
    }

    #[test]
    fn noise_swaps_first_and_third_below_level_6() {
        let list = [Down, Right, Left, Up];
        // Level 3, draw 1: first and third change places.
        assert_eq!(apply_noise(list, 1, difficulty(3)), [Left, Right, Down, Up]);
        assert_eq!(apply_noise(list, 1, difficulty(5)), [Left, Right, Down, Up]);
        // Any other draw, or difficulty 6 and above, changes nothing.
        assert_eq!(apply_noise(list, 0, difficulty(3)), list);
        assert_eq!(apply_noise(list, 2, difficulty(1)), list);
        assert_eq!(apply_noise(list, 1, difficulty(6)), list);
        assert_eq!(apply_noise(list, 1, difficulty(10)), list);
    }

    /// Row 4 open, column 7 open, crossing at (7,4).
    fn crossroads() -> [String; 10] {
        let mut rows = row_4(0, 14);
        for (v, row) in rows.iter_mut().enumerate() {
            let mut bytes = row.clone().into_bytes();
            if let Some(c) = bytes.get_mut(7) {
                *c = if v == 4 { b'S' } else { b'V' };
            }
            *row = String::from_utf8(bytes).unwrap_or_default();
        }
        rows
    }

    #[test]
    fn quirk_q03_noise_draw_taken_at_level_8_without_effect() {
        // A nobbin heading left on the crossing (7,4) with the digger
        // straight below: down, left, up, right after the no-reverse rule.
        // Below level 6 a noise draw of 1 would make it go up.
        let rows = crossroads();
        let seed = scenario(&rows, 8)
            .search_seed(10_000, |mut run| {
                run.start();
                first_draws(&run, &[13, 7]) == [1, 1]
            })
            .unwrap_or_else(|| unreachable!("a seed exists"));
        let mut run = started(&rows, 8, seed);
        place(&mut run, 0, Px::new(152, 90), MonsterSetup::default());
        let before = run.state().rng_draws();
        assert_eq!(run.run_ticks(1), 1);
        assert_eq!(run.state().rng_draws() - before, 2, "noise and double move");
        let monster = run.state().monster(slot(0));
        assert_eq!(monster.direction(), Some(Down), "the noise had no effect");
        assert_eq!(monster.position(), Px::new(152, 93));
        assert_eq!(monster.delay(), 1, "turning costs a tick");

        // The same draws at level 3 swap: the nobbin goes up.
        let seed = scenario(&rows, 3)
            .search_seed(10_000, |mut run| {
                run.start();
                first_draws(&run, &[8, 12]).first() == Some(&1)
                    && first_draws(&run, &[8, 12]).get(1) != Some(&0)
            })
            .unwrap_or_else(|| unreachable!("a seed exists"));
        let mut run = started(&rows, 3, seed);
        place(&mut run, 0, Px::new(152, 90), MonsterSetup::default());
        assert_eq!(run.run_ticks(1), 1);
        assert_eq!(run.state().monster(slot(0)).direction(), Some(Up));
    }

    #[test]
    fn quirk_q28_dead_end_reverses() {
        // A dead end: the tunnel behind is the only way on.
        let mut run = started(&row_4(4, 8), 1, 0);
        place(&mut run, 0, Px::new(92, 90), MonsterSetup::default());
        assert_eq!(run.run_ticks(1), 1);
        assert_eq!(run.state().monster(slot(0)).direction(), Some(Right));

        // A closed cell: nothing qualifies, so the reverse is taken anyway.
        let mut run = started(&row_4(4, 4), 1, 0);
        place(&mut run, 0, Px::new(92, 90), MonsterSetup::default());
        assert_eq!(run.run_ticks(1), 1);
        let monster = run.state().monster(slot(0));
        assert_eq!(monster.direction(), Some(Right));
        assert_eq!(monster.delay(), 1, "the change of direction costs a tick");

        // With no direction there is no reverse: it stays without one.
        let mut run = started(&row_4(4, 4), 1, 0);
        let none = MonsterSetup {
            direction: None,
            ..MonsterSetup::default()
        };
        place(&mut run, 0, Px::new(92, 90), none);
        assert_eq!(run.run_ticks(1), 1);
        let monster = run.state().monster(slot(0));
        assert_eq!(monster.direction(), None);
        assert_eq!(monster.position(), Px::new(92, 90));
    }

    #[test]
    fn hobbin_thresholds_by_level() {
        // The counter a nobbin must reach to become a hobbin, and the one a
        // hobbin must pass to turn back.
        for (level, reach, back) in [(1, 10, 32), (5, 6, 40), (9, 2, 48), (10, 1, 50)] {
            assert_eq!(
                hobbin_threshold(difficulty(level)) + 1,
                reach,
                "level {level}"
            );
            assert_eq!(nobbin_threshold(difficulty(level)), back, "level {level}");
        }

        // At level 1 a nobbin with a counter of 10 changes on the next
        // tick; one with 9 does not.
        let rows = row_4(0, 14);
        for (counter, nobbin_after) in [(10, false), (9, true)] {
            let mut run = started(&rows, 1, 0);
            let setup = MonsterSetup {
                counter,
                ..MonsterSetup::default()
            };
            place(&mut run, 0, Px::new(100, 90), setup);
            assert_eq!(run.run_ticks(1), 1);
            let monster = run.state().monster(slot(0));
            assert_eq!(monster.nobbin(), nobbin_after, "counter {counter}");
            if !nobbin_after {
                assert_eq!(monster.counter(), 1, "reset, then one hobbin move");
            }
        }
    }

    #[test]
    fn hobbin_reverts_at_corner_above_30_plus_2l() {
        let rows = row_4(0, 14);
        let hobbin = |counter| MonsterSetup {
            hobbin: true,
            counter,
            ..MonsterSetup::default()
        };
        // On a corner at level 1: 33 reverts, 32 does not.
        let mut run = started(&rows, 1, 0);
        place(&mut run, 0, Px::new(112, 90), hobbin(33));
        assert_eq!(run.run_ticks(1), 1);
        let monster = run.state().monster(slot(0));
        assert!(monster.nobbin());
        assert_eq!(monster.counter(), 0);

        let mut run = started(&rows, 1, 0);
        place(&mut run, 0, Px::new(112, 90), hobbin(32));
        assert_eq!(run.run_ticks(1), 1);
        let monster = run.state().monster(slot(0));
        assert!(!monster.nobbin());
        assert_eq!(monster.counter(), 33);

        // Between corners it stays a hobbin whatever its counter.
        let mut run = started(&rows, 1, 0);
        place(&mut run, 0, Px::new(116, 90), hobbin(40));
        assert_eq!(run.run_ticks(1), 1);
        let monster = run.state().monster(slot(0));
        assert!(!monster.nobbin());
        assert_eq!(monster.counter(), 41);

        // At level 5 the limit is 40.
        let mut run = started(&rows, 5, 0);
        place(&mut run, 0, Px::new(112, 90), hobbin(41));
        assert_eq!(run.run_ticks(1), 1);
        assert!(run.state().monster(slot(0)).nobbin());
    }

    #[test]
    fn quirk_q21_squashed_monster_snaps_to_bag_top() {
        let mut run = started(&row_4(0, 14), 1, 0);
        let bag = BagId::ALL[0];
        assert_eq!(run.place_bag(bag, Px::new(100, 96)), Ok(()));
        place(&mut run, 0, Px::new(100, 100), MonsterSetup::default());
        place(&mut run, 1, Px::new(100, 102), MonsterSetup::default());
        with_ctx(&mut run, |cx| {
            bag.of_mut(&mut cx.sim.bags).direction = Some(Down);
            for n in [0, 1] {
                let monster = slot(n).of_mut(&mut cx.sim.monsters.slots);
                monster.alive = false;
                monster.dead = true;
                monster.death = MonsterDeath::Carried;
                monster.death_bag = bag;
            }
            cx.monster_die(slot(0));
            cx.monster_die(slot(1));
        });
        // 4 pixels below the bag's top: set to the top itself, not top + 6.
        assert_eq!(run.state().monster(slot(0)).position(), Px::new(100, 96));
        // 6 pixels below: left where it is.
        assert_eq!(run.state().monster(slot(1)).position(), Px::new(100, 102));
        assert_eq!(run.state().monster(slot(0)).death(), MonsterDeath::Carried);
    }

    #[test]
    fn quirk_q30_dead_left_hobbin_is_14_tall() {
        let mut run = started(&row_4(0, 14), 1, 0);
        let bag = BagId::ALL[0];
        assert_eq!(run.place_bag(bag, Px::new(100, 80)), Ok(()));
        let kinds = [(true, Left), (false, Right), (false, Left)];
        for (n, &(nobbin, facing)) in (0u8..).zip(&kinds) {
            let setup = MonsterSetup {
                hobbin: !nobbin,
                direction: Some(facing),
                ..MonsterSetup::default()
            };
            place(&mut run, n, Px::new(100, 90), setup);
        }
        with_ctx(&mut run, |cx| {
            // The bag falls onto the three, whose tops are below its top.
            bag.of_mut(&mut cx.sim.bags).position = Px::new(100, 84);
            let hits = cx.gfx.move_bag(bag, ImageId::BAG_FALLING, Px::new(100, 84));
            cx.squash_monsters(bag, hits);
        });
        let heights: Vec<i32> = (0..3)
            .map(|n| run.state().sprites().rect(slot(n).sprite()).height())
            .collect();
        assert_eq!(heights, [15, 15, 14]);
        assert_eq!(
            run.state().sprites().image(slot(2).sprite()),
            ImageId::HOBBIN_LEFT_DEAD
        );
        for n in 0..3 {
            let monster = run.state().monster(slot(n));
            assert!(monster.occupied() && !monster.alive() && monster.dead());
            assert_eq!(monster.death(), MonsterDeath::Carried);
        }
    }

    #[test]
    fn quirk_q31_bonus_mode_removal_raises_total() {
        let mut run = started(&row_4(0, 14), 1, 0);
        place(&mut run, 0, Px::new(100, 90), MonsterSetup::default());
        place(&mut run, 1, Px::new(160, 90), MonsterSetup::default());
        let total = run.state().monsters().total();
        assert_eq!(run.remove_monster(slot(0)), Ok(()));
        assert_eq!(run.state().monsters().total(), total, "outside bonus mode");
        assert_eq!(run.set_bonus_mode(true), Ok(()));
        assert_eq!(run.remove_monster(slot(1)), Ok(()));
        assert_eq!(run.state().monsters().total(), total + 1);
        // Removing a free slot again changes nothing.
        assert_eq!(run.remove_monster(slot(1)), Ok(()));
        assert_eq!(run.state().monsters().total(), total + 1);
    }

    #[test]
    fn quirk_q29_carried_dead_nobbin_still_transforms() {
        // A dead nobbin carried by a falling bag, with a counter above
        // 10 − 1: the tick's transformation test still makes it a hobbin,
        // shown with the dead hobbin's image.
        let mut run = started(&row_4(0, 14), 1, 0);
        let bag = BagId::ALL[0];
        assert_eq!(run.place_bag(bag, Px::new(100, 72)), Ok(()));
        let setup = MonsterSetup {
            counter: 10,
            ..MonsterSetup::default()
        };
        place(&mut run, 0, Px::new(100, 90), setup);
        with_ctx(&mut run, |cx| {
            bag.of_mut(&mut cx.sim.bags).direction = Some(Down);
            let monster = slot(0).of_mut(&mut cx.sim.monsters.slots);
            monster.alive = false;
            monster.dead = true;
            monster.death = MonsterDeath::Carried;
            monster.death_bag = bag;
        });
        let draws = run.state().rng_draws();
        assert_eq!(run.run_ticks(1), 1);
        let monster = run.state().monster(slot(0));
        assert!(!monster.nobbin(), "transformed although dead");
        assert_eq!(monster.counter(), 0);
        assert!(monster.dead() && monster.occupied());
        assert_eq!(
            run.state().sprites().image(slot(0).sprite()),
            ImageId::HOBBIN_LEFT_DEAD
        );
        assert_eq!(
            run.state().rng_draws(),
            draws,
            "a dead monster draws nothing"
        );
    }
}
