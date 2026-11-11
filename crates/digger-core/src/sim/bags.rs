//! Gold bags: creation, wobble, fall, landing, gold, push, chain push and
//! the round-end tidy (design D1; gold-bags spec; research 02 §5).
//!
//! The seven bags are updated in number order on every tick. A bag at rest
//! starts to wobble when the cell below it has a tunnel in it, falls 16
//! ticks later, falls 6 pixels per tick until it lands, and breaks into gold
//! when it fell two cells or more. Bags are pushed sideways by diggers and
//! monsters, push the bags they touch in turn, and slide on to the next cell
//! corner by themselves.
//!
//! The bag record keeps every variable of the upstream state hash at the
//! original's granularity, stale values included (design D5).

use crate::assets::ImageId;
use crate::geom::Px;
use crate::sound_cmd::SoundCommand;
use crate::sprite::{Collisions, SpriteClass};

use super::draw::{self, Carve};
use super::geom::{CellPos, Direction, Located, MAX_X, MAX_Y, MIN_X, MIN_Y, locate};
use super::ids::{BagId, DiggerId, PlayerId};
use super::level::{LevelLayout, LevelNumber};
use super::penalty::PenaltySite;
use super::score::{ScoreEvent, ScoreId};
use super::{Ctx, DeathStage, Sim};

/// The layout character of a bag cell.
const BAG_CHAR: u8 = b'B';
/// The wobble count a resting bag holds; a wobbling bag counts it down
/// and falls on the tick after it reached 0 (research 02 §5.3).
pub(crate) const WOBBLE_COUNT: u8 = 15;
/// Pixels a falling bag descends per tick (research 02 §0).
const FALL_STEP: i32 = 6;
/// Pixels a pushed or sliding bag moves per tick (research 02 §0).
const PUSH_STEP: i32 = 4;
/// Cells a bag must fall to break into gold (research 02 §5.4).
const BREAK_HEIGHT: u8 = 2;
/// The gold lifetime at difficulty 0: the lifetime is 150 − 10 × the
/// difficulty index (gold-bags spec, "Gold lifetime"; research 02 §5.1,
/// §7).
const GOLD_TIME_BASE: u16 = 150;
/// The lifetime lost per difficulty step.
const GOLD_TIME_STEP: u16 = 10;
/// How many ticks before the end of its lifetime gold over a dug cell is
/// put, so that it disappears within this many ticks (research 02 §5.2).
const GOLD_EARLY_END: u16 = 10;
/// Gold younger than this keeps the round's wind-down going (research 02
/// §5.8).
const YOUNG_GOLD: u16 = 10;
/// **Quirk Q37:** a bag in this cell at the end of a round is removed.
const TIDY_CELL: (u8, u8) = (7, 9);

/// The images a wobbling bag is redrawn with, indexed by (count mod 8) / 2:
/// tilted left, upright, tilted right, upright (research 02 §5.3,
/// `wblanim`).
const WOBBLE_IMAGES: [ImageId; 4] = [
    ImageId::BAG_WOBBLE_LEFT,
    ImageId::BAG_STILL,
    ImageId::BAG_WOBBLE_RIGHT,
    ImageId::BAG_STILL,
];

/// One gold bag (`bagdat[]`).
///
/// The values stay in the record after the bag is removed (design D5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bag {
    /// Whether the bag exists (`exist`).
    pub(crate) exists: bool,
    /// The position (`x`, `y`).
    pub(crate) position: Px,
    /// The cell and remainders as stored with the position (`h`, `v`,
    /// `xr`, `yr`); all zero in a record never filled.
    pub(crate) located: Located,
    /// The direction the bag is moving in (`dir`): none at rest, left or
    /// right while sliding, down while falling.
    pub(crate) direction: Option<Direction>,
    /// The wobble count (`wt`), 15 at rest.
    pub(crate) wobble: u8,
    /// The gold age (`gt`): 0 for an unbroken bag, from 1 on the landing
    /// tick for gold.
    pub(crate) gold_age: u16,
    /// Cell corners reached while falling (`fallh`).
    pub(crate) cells_fallen: u8,
    /// Whether the bag is wobbling (`wobbling`); it stays set through the
    /// fall that follows a wobble.
    pub(crate) wobbling: bool,
    /// Whether the bag has never taken a fall step (`unfallen`).
    pub(crate) unfallen: bool,
}

impl Default for Bag {
    fn default() -> Self {
        Self::ZEROED
    }
}

impl Bag {
    /// A record never filled: the original's zero-initialised values, in
    /// which direction 0 is right (research 02 §5.1).
    pub(crate) const ZEROED: Self = Self {
        exists: false,
        position: Px::new(0, 0),
        located: Located {
            cell: match CellPos::new(0, 0) {
                Some(cell) => cell,
                None => unreachable!(),
            },
            rem_x: 0,
            rem_y: 0,
        },
        direction: Some(Direction::Right),
        wobble: 0,
        gold_age: 0,
        cells_fallen: 0,
        wobbling: false,
        unfallen: false,
    };

    /// A new bag at rest on the corner of `cell`: unbroken, never fallen,
    /// with a full wobble count (research 02 §5.1, `initbags`).
    pub(crate) const fn resting_at(cell: CellPos) -> Self {
        Self {
            exists: true,
            position: cell.to_px(),
            located: Located {
                cell,
                rem_x: 0,
                rem_y: 0,
            },
            direction: None,
            wobble: WOBBLE_COUNT,
            gold_age: 0,
            cells_fallen: 0,
            wobbling: false,
            unfallen: true,
        }
    }

    /// Whether the bag exists (hashed as `exist`).
    pub const fn exists(&self) -> bool {
        self.exists
    }

    /// The position (hashed as `x`, `y`).
    pub const fn position(&self) -> Px {
        self.position
    }

    /// The cell and remainders stored with the position (hashed as `h`,
    /// `v`, `xr`, `yr`).
    pub const fn located(&self) -> Located {
        self.located
    }

    /// The direction the bag is moving in (hashed as `dir`), or none at
    /// rest.
    pub const fn direction(&self) -> Option<Direction> {
        self.direction
    }

    /// The wobble count (hashed as `wt`).
    pub const fn wobble(&self) -> u8 {
        self.wobble
    }

    /// The gold age (hashed as `gt`): 0 for an unbroken bag.
    pub const fn gold_age(&self) -> u16 {
        self.gold_age
    }

    /// Whether the bag has broken into gold.
    pub const fn is_gold(&self) -> bool {
        self.gold_age != 0
    }

    /// Cell corners reached while falling (hashed as `fallh`).
    pub const fn cells_fallen(&self) -> u8 {
        self.cells_fallen
    }

    /// Whether the bag is wobbling (hashed as `wobbling`).
    pub const fn wobbling(&self) -> bool {
        self.wobbling
    }

    /// Whether the bag has never taken a fall step (hashed as `unfallen`).
    pub const fn unfallen(&self) -> bool {
        self.unfallen
    }

    /// Whether the bag is falling.
    pub const fn is_falling(&self) -> bool {
        matches!(self.direction, Some(Direction::Down))
    }

    /// Whether a digger crushed by this bag stops being carried: the bag
    /// no longer exists or is at rest (research 02 §3.7, `getbagdir`).
    pub(crate) const fn has_stopped(&self) -> bool {
        !self.exists || self.direction.is_none()
    }

    /// Whether the bag counts as still moving for the wind-down: it exists
    /// and is wobbling (the fall after a wobble included) or is gold
    /// younger than 10 ticks (gold-bags spec, "Moving-bag count for the
    /// wind-down"; research 02 §5.8, `getnmovingbags`).
    pub const fn is_moving(&self) -> bool {
        self.exists && self.gold_age < YOUNG_GOLD && (self.gold_age != 0 || self.wobbling)
    }

    /// **Quirk Q37:** whether the round-end tidy removes this bag: it is in
    /// cell (7,9), is not on a cell corner, is gold, has a fall recorded or
    /// is wobbling (gold-bags spec, "Bags persist per player across
    /// rounds"; research 02 §5.8, `cleanupbags`).
    const fn tidied_away(&self) -> bool {
        let cell = self.located.cell;
        self.exists
            && ((cell.h() == TIDY_CELL.0 && cell.v() == TIDY_CELL.1)
                || self.located.rem_x != 0
                || self.located.rem_y != 0
                || self.gold_age != 0
                || self.cells_fallen != 0
                || self.wobbling)
    }

    /// Moves the bag to `at` and stores the cell and remainders with it.
    fn move_to(&mut self, at: Px) {
        self.position = at;
        if let Some(located) = locate(at) {
            self.located = located;
        } else {
            debug_assert!(false, "a bag stays within the playfield");
        }
    }
}

/// The bags of `layout`: one at rest on the corner of every `B` cell,
/// scanning columns left to right and each column top to bottom, at most
/// seven; the other records are zeroed (gold-bags spec, "Bag creation";
/// research 02 §5.1, `initbags`).
pub(crate) fn bags_of(layout: &LevelLayout) -> [Bag; BagId::COUNT] {
    let mut bags = [Bag::ZEROED; BagId::COUNT];
    let cells = CellPos::all_column_major().filter(|&cell| layout.cell_char(cell) == BAG_CHAR);
    for (bag, cell) in bags.iter_mut().zip(cells) {
        *bag = Bag::resting_at(cell);
    }
    bags
}

/// The gold lifetime on `level`: 150 − 10 × its difficulty index, and 150
/// for the level 0 placeholder of quirk Q06 (gold-bags spec, "Gold
/// lifetime"; research 02 §5.1, §7).
pub(crate) fn gold_time(level: Option<LevelNumber>) -> u16 {
    let difficulty = level.map_or(0, |level| u16::from(level.difficulty().get()));
    GOLD_TIME_BASE - GOLD_TIME_STEP * difficulty
}

/// Whether a bag at `at` may not move in `direction` because it is at that
/// edge of the playfield (research 02 §5.5).
const fn at_edge(at: Px, direction: Direction) -> bool {
    match direction {
        Direction::Right => at.x == MAX_X,
        Direction::Left => at.x == MIN_X,
        Direction::Down => at.y == MAX_Y,
        Direction::Up => at.y == MIN_Y,
    }
}

/// Which bag sound cues were started and not yet stopped (not hashed), so
/// that a cue is stopped only while it runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct BagCues {
    /// The fall cue.
    pub(crate) fall: bool,
    /// The wobble cue.
    pub(crate) wobble: bool,
}

/// Each player's saved bags (`bagdat1`, `bagdat2`).
pub(crate) type PlayerBags = [[Bag; BagId::COUNT]; PlayerId::COUNT];

impl Sim {
    /// The number of bags still moving, for the end of the round's
    /// wind-down (research 02 §5.8, `getnmovingbags`).
    pub(crate) fn moving_bags(&self) -> usize {
        self.bags.iter().filter(|bag| bag.is_moving()).count()
    }

    /// Creates `player`'s bags from `layout` into play and into the
    /// player's save, clears the push counter and sets the gold lifetime
    /// from `level` (research 02 §5.1, `initbags`).
    ///
    /// The lifetime is one value for the whole game, set whenever a level
    /// is set up (quirk Q40).
    pub(crate) fn create_bags(
        &mut self,
        player: PlayerId,
        layout: &LevelLayout,
        level: Option<LevelNumber>,
    ) {
        self.push_count = 0;
        self.gold_time = gold_time(level);
        self.bags = bags_of(layout);
        *player.of_mut(&mut self.saved_bags) = self.bags;
    }
}

impl Ctx<'_> {
    /// Puts the current player's saved bags into play and resets the bag
    /// sprites to the upright image, disabled, as a full draw does before
    /// it draws them (research 02 §5.1, `creatembspr`, `drawbags`).
    pub(crate) fn restore_bags(&mut self) {
        self.sim.bags = *self.sim.turn.of(&self.sim.saved_bags);
        for id in BagId::ALL {
            self.gfx.reset(id.sprite(), ImageId::BAG_STILL);
        }
    }

    /// Gives every bag sprite the upright image for its next draw, as every
    /// round start does (research 02 §1, `initmbspr`).
    pub(crate) fn reset_bag_images(&mut self) {
        for id in BagId::ALL {
            self.gfx.set_pending(id.sprite(), ImageId::BAG_STILL);
        }
    }

    /// The bags' part of a tick: every existing bag in number order, gold
    /// aging and the others moving, then the fall and wobble cues are
    /// stopped when no bag needs them any more (gold-bags spec; research 02
    /// §5.2, `dobags`).
    pub(crate) fn update_bags(&mut self) {
        for id in BagId::ALL {
            let bag = *id.of(&self.sim.bags);
            if !bag.exists {
                continue;
            }
            if bag.is_gold() {
                self.update_gold(id);
            } else {
                self.update_bag(id);
            }
        }
        self.stop_bag_cues();
    }

    /// Stops the fall cue unless a bag is falling and the wobble cue unless
    /// a bag is wobbling without falling (gold-bags spec, "Bag sound cues").
    /// A cue is stopped only while it runs, so a quiet field sends nothing.
    fn stop_bag_cues(&mut self) {
        let bags = &self.sim.bags;
        let falling = bags.iter().any(|bag| bag.exists && bag.is_falling());
        let wobbling = bags
            .iter()
            .any(|bag| bag.exists && !bag.is_falling() && bag.wobbling);
        if !falling && self.sim.bag_cues.fall {
            self.sim.bag_cues.fall = false;
            self.sound.push(SoundCommand::FallStop);
        }
        if !wobbling && self.sim.bag_cues.wobble {
            self.sim.bag_cues.wobble = false;
            self.sound.push(SoundCommand::WobbleStop);
        }
    }

    /// Emits the fall cue for a bag that starts falling.
    fn start_fall_cue(&mut self) {
        self.sim.bag_cues.fall = true;
        self.sound.push(SoundCommand::FallStart);
    }

    /// Draws bag `id` at its position with `image` and returns the
    /// collisions of the draw (research 02 §5, `drawgold`).
    fn draw_bag(&mut self, id: BagId, image: ImageId) -> Collisions {
        let at = id.of(&self.sim.bags).position;
        self.gfx.move_bag(id, image, at)
    }

    /// One tick of gold: the opening frames on its 1st, 3rd and 5th tick
    /// (with the break cue on the first), aging, removal at the end of its
    /// lifetime, and the early end over a dug cell (gold-bags spec, "Gold
    /// lifetime", "Gold over a dug cell vanishes early"; research 02 §5.2).
    ///
    /// **Quirk Q34:** gold above the bottom row whose cell below is no
    /// longer untouched, while more than 10 ticks short of its lifetime,
    /// jumps to 10 ticks short.
    fn update_gold(&mut self, id: BagId) {
        let age = id.of(&self.sim.bags).gold_age;
        let frame = match age {
            1 => Some(ImageId::GOLD_1),
            3 => Some(ImageId::GOLD_2),
            5 => Some(ImageId::GOLD_3),
            _ => None,
        };
        if let Some(image) = frame {
            if age == 1 {
                self.sound.push(SoundCommand::Break);
            }
            self.draw_bag(id, image);
            self.sim.penalty.charge(PenaltySite::GoldFrame);
        }
        let gold_time = self.sim.gold_time;
        let bag = id.of_mut(&mut self.sim.bags);
        bag.gold_age = bag.gold_age.saturating_add(1);
        if bag.gold_age == gold_time {
            self.remove_bag(id);
            return;
        }
        let early = gold_time.saturating_sub(GOLD_EARLY_END);
        if bag.gold_age < early
            && let Some(below) = bag.located.cell.neighbour(Direction::Down)
            && !self.sim.field.cell(below).is_untouched()
        {
            id.of_mut(&mut self.sim.bags).gold_age = early;
        }
    }

    /// One tick of an unbroken bag (research 02 §5.3, `updatebag`).
    ///
    /// At rest on a cell corner above the bottom row it starts to wobble
    /// when the cell below has a tunnel (unless a digger climbs underneath,
    /// quirk Q36), counts the wobble down and starts to fall; at rest
    /// elsewhere its wobble is reset. Sliding, on reaching a cell corner it
    /// drops at once over a tunnel or comes to rest. Falling, it counts the
    /// cell corners it reaches and lands on the bottom row or above solid
    /// earth. A moving bag then takes its step, except that the first
    /// sliding bag updated while the shared push counter is set clears the
    /// counter instead (quirk Q32).
    fn update_bag(&mut self, id: BagId) {
        let bag = *id.of(&self.sim.bags);
        let below = bag.located.cell.neighbour(Direction::Down);
        let tunnel_below = below.is_some_and(|cell| self.sim.field.cell(cell).has_tunnel());
        let above_bottom = bag.position.y < MAX_Y;
        match bag.direction {
            None => {
                if above_bottom && bag.located.rem_x == 0 {
                    if bag.wobbling {
                        self.wobble(id);
                    } else if tunnel_below
                        && !below.is_some_and(|cell| self.sim.digger_under_bag(cell))
                    {
                        id.of_mut(&mut self.sim.bags).wobbling = true;
                    }
                } else {
                    let bag = id.of_mut(&mut self.sim.bags);
                    bag.wobble = WOBBLE_COUNT;
                    bag.wobbling = false;
                }
            }
            Some(Direction::Left | Direction::Right) => {
                if bag.located.rem_x == 0 {
                    if above_bottom && tunnel_below {
                        // The end of a slide over a tunnel: no wobble.
                        let bag = id.of_mut(&mut self.sim.bags);
                        bag.direction = Some(Direction::Down);
                        bag.wobble = 0;
                        self.start_fall_cue();
                    } else {
                        self.land(id);
                    }
                }
            }
            Some(Direction::Down) => {
                if bag.located.rem_y == 0 {
                    let bag = id.of_mut(&mut self.sim.bags);
                    bag.cells_fallen = bag.cells_fallen.saturating_add(1);
                }
                let solid_below = below.is_some_and(|cell| !self.sim.field.cell(cell).has_tunnel());
                if !above_bottom || (solid_below && bag.located.rem_y == 0) {
                    self.land(id);
                }
                // Monsters below a falling bag are frightened by task 12.8.
            }
            Some(Direction::Up) => {
                debug_assert!(false, "bags never move up");
            }
        }
        let bag = id.of(&self.sim.bags);
        if let Some(direction) = bag.direction {
            if direction != Direction::Down && self.sim.push_count != 0 {
                self.sim.push_count -= 1;
            } else {
                self.push_bag(id, direction);
            }
        }
    }

    /// One tick of a wobbling bag: on the tick after its count reached 0 it
    /// starts to fall with the fall cue; otherwise the count goes down by 1
    /// and, when the new count modulo 8 is even, the bag is redrawn
    /// alternately upright and tilted with the wobble cue (gold-bags spec,
    /// "Wobble timing"; research 02 §5.3).
    fn wobble(&mut self, id: BagId) {
        let bag = id.of_mut(&mut self.sim.bags);
        if bag.wobble == 0 {
            bag.direction = Some(Direction::Down);
            self.start_fall_cue();
            return;
        }
        bag.wobble -= 1;
        let phase = bag.wobble % 8;
        if phase.is_multiple_of(2) {
            let image = WOBBLE_IMAGES
                .get(usize::from(phase / 2))
                .copied()
                .unwrap_or(ImageId::BAG_STILL);
            self.draw_bag(id, image);
            self.sim.penalty.charge(PenaltySite::WobbleFrame);
            self.sim.bag_cues.wobble = true;
            self.sound.push(SoundCommand::WobbleStart);
        }
    }

    /// A bag comes to rest: after a fall of two cells or more it breaks
    /// into gold, otherwise its fall count is cleared; it is drawn upright,
    /// and every other bag the draw touches is removed (gold-bags spec,
    /// "Landing", "Breaking after two or more cells"; research 02 §5.4,
    /// `baghitground`).
    ///
    /// **Quirk Q34:** the bags overlapping the landed bag are removed, and
    /// a bag breaks after falling two cells.
    fn land(&mut self, id: BagId) {
        let bag = id.of_mut(&mut self.sim.bags);
        if bag.is_falling() && bag.cells_fallen >= BREAK_HEIGHT {
            bag.gold_age = 1;
        } else {
            bag.cells_fallen = 0;
        }
        bag.direction = None;
        bag.wobble = WOBBLE_COUNT;
        bag.wobbling = false;
        let hits = self.draw_bag(id, ImageId::BAG_STILL);
        self.sim.penalty.charge(PenaltySite::BagLanding);
        for sprite in hits.of(SpriteClass::Bag) {
            if let Some(other) = BagId::from_sprite(sprite) {
                self.remove_bag(other);
            }
        }
    }

    /// Removes bag `id` if it exists: it no longer exists and its sprite is
    /// erased (research 02 §5.8, `removebag`).
    pub(crate) fn remove_bag(&mut self, id: BagId) {
        let bag = id.of_mut(&mut self.sim.bags);
        if bag.exists {
            bag.exists = false;
            self.gfx.erase(id.sprite());
        }
    }

    /// Pushes every bag `hits` touched in `direction`, attempting all of
    /// them, and returns whether every push succeeded (gold-bags spec,
    /// "Push chains"; research 02 §5.5, `pushbags`).
    pub(crate) fn push_bags(&mut self, direction: Direction, hits: Collisions) -> bool {
        let mut pushed = true;
        for sprite in hits.of(SpriteClass::Bag) {
            if let Some(id) = BagId::from_sprite(sprite)
                && !self.push_bag(id, direction)
            {
                pushed = false;
            }
        }
        pushed
    }

    /// Meets every bag `hits` touched with an up, down or no move: gold is
    /// collected, and any unbroken bag blocks the move. Returns whether no
    /// bag blocked it (gold-bags spec, "Horizontal push", "Collecting
    /// gold"; research 02 §5.5, `pushudbags`).
    pub(crate) fn push_bags_vertically(&mut self, hits: Collisions) -> bool {
        let mut pushed = true;
        for sprite in hits.of(SpriteClass::Bag) {
            if let Some(id) = BagId::from_sprite(sprite) {
                if id.of(&self.sim.bags).is_gold() {
                    self.collect_gold(id);
                } else {
                    pushed = false;
                }
            }
        }
        pushed
    }

    /// Pushes bag `id` one step in `direction`, for a pusher or for the
    /// bag's own slide or fall, and returns whether the push succeeded
    /// (research 02 §5.5, `pushbag`).
    ///
    /// Gold is collected and the push succeeds. A falling bag pushed
    /// sideways does not move (quirk Q33). At the playfield edge the push
    /// fails and nothing changes. A fall step carves the shaft, digs one
    /// slice of the cell below, removes its emerald without score (quirk
    /// Q38) and crushes; a sideways step stops any wobble and may push a
    /// chain of bags.
    ///
    /// **Quirk Q32:** every sideways step sets the push counter shared by
    /// all bags; a sideways push that fails leaves the bag sliding the
    /// opposite way, and a chain that fails and also touches a digger or a
    /// monster restores the bag twice.
    fn push_bag(&mut self, id: BagId, direction: Direction) -> bool {
        let bag = *id.of(&self.sim.bags);
        if bag.is_gold() {
            self.collect_gold(id);
            return true;
        }
        if bag.is_falling() && direction.is_horizontal() {
            self.push_falling_bag(id);
            return true;
        }
        if at_edge(bag.position, direction) {
            return false;
        }
        let pushed = match direction {
            Direction::Down => {
                self.fall_step(id);
                true
            }
            Direction::Left | Direction::Right => self.slide_step(id, direction),
            Direction::Up => {
                debug_assert!(false, "bags are never pushed up");
                return false;
            }
        };
        id.of_mut(&mut self.sim.bags).direction = Some(if pushed {
            direction
        } else {
            direction.reverse()
        });
        pushed
    }

    /// One fall step of bag `id` (gold-bags spec, "Falling"; research 02
    /// §5.5): the first carves the toothed opening with the top cap below
    /// it, later ones the furry shape; then one slice of the cell below is
    /// dug, its emerald removed without score, the bag moves 6 pixels down,
    /// is drawn falling and crushes what it overlaps.
    ///
    /// **Quirk Q38:** the emerald in the cell below is removed and nobody
    /// scores for it.
    fn fall_step(&mut self, id: BagId) {
        let bag = *id.of(&self.sim.bags);
        let at = bag.position;
        if bag.unfallen {
            id.of_mut(&mut self.sim.bags).unfallen = false;
            draw::carve(self.gfx, Carve::Square, at);
            draw::carve(self.gfx, Carve::Top, Px::new(at.x, at.y + 21));
        } else {
            draw::carve(self.gfx, Carve::Furry, at);
        }
        self.sim
            .field
            .dig(at, Some(Direction::Down), &mut self.sim.penalty);
        if let Some(below) = bag.located.cell.neighbour(Direction::Down) {
            let player = self.sim.turn;
            if self.sim.emeralds.remove(player, below) {
                draw::erase_emerald(self.gfx, below);
            }
        }
        id.of_mut(&mut self.sim.bags)
            .move_to(Px::new(at.x, at.y + FALL_STEP));
        let hits = self.draw_bag(id, ImageId::BAG_FALLING);
        self.sim.penalty.charge(PenaltySite::FallStepDraw);
        self.crush(id, hits);
    }

    /// **Quirk Q33:** a falling bag pushed sideways is redrawn in place,
    /// falling, and crushes what it overlaps; it does not move and the
    /// push succeeds (gold-bags spec, "Pushing against a falling bag";
    /// research 02 §5.5, §9 item 10).
    fn push_falling_bag(&mut self, id: BagId) {
        let hits = self.draw_bag(id, ImageId::BAG_FALLING);
        self.sim.penalty.charge(PenaltySite::FallingBagPushed);
        self.crush(id, hits);
    }

    /// Crushes what the falling bag `id`'s draw touched: every overlapped
    /// digger whose top is at or below the bag's top is killed by the bag
    /// (invincible ones are immune), and the overlapped monsters likewise
    /// are squashed (gold-bags spec, "Crushing"; research 02 §5.5, §5.7).
    fn crush(&mut self, id: BagId, hits: Collisions) {
        let top = id.of(&self.sim.bags).position.y;
        for sprite in hits.of(SpriteClass::Digger) {
            let Some(unit) = DiggerId::from_sprite(sprite) else {
                continue;
            };
            let record = self.sim.record_of_unit(unit);
            if record.of(&self.sim.diggers).position.y >= top {
                self.sim.kill_digger(record, DeathStage::Bag, id);
            }
        }
        // Squashing the overlapped monsters is added by task 12.8.
    }

    /// One sideways step of bag `id` (gold-bags spec, "Horizontal push",
    /// "Push chains"; research 02 §5.5): the wobble stops, the bag moves 4
    /// pixels and is drawn upright, and the shared push counter is set.
    /// The bags it then touches are pushed on; if one of them fails, or the
    /// bag touches a living digger or any monster, it is put back. Returns
    /// whether it moved.
    fn slide_step(&mut self, id: BagId, direction: Direction) -> bool {
        let from = id.of(&self.sim.bags).position;
        let dx = if direction == Direction::Right {
            PUSH_STEP
        } else {
            -PUSH_STEP
        };
        let bag = id.of_mut(&mut self.sim.bags);
        bag.wobble = WOBBLE_COUNT;
        bag.wobbling = false;
        bag.move_to(Px::new(from.x + dx, from.y));
        let hits = self.draw_bag(id, ImageId::BAG_STILL);
        self.sim.penalty.charge(PenaltySite::SlideStepDraw);
        self.sim.push_count = 1;
        let mut pushed = true;
        if hits.first(SpriteClass::Bag).is_some() && !self.push_bags(direction, hits) {
            self.restore_bag(id, from);
            pushed = false;
        }
        let digger_in_way = hits.of(SpriteClass::Digger).any(|sprite| {
            DiggerId::from_sprite(sprite)
                .is_some_and(|unit| self.sim.record_of_unit(unit).of(&self.sim.diggers).alive)
        });
        if digger_in_way || hits.first(SpriteClass::Monster).is_some() {
            self.restore_bag(id, from);
            pushed = false;
        }
        pushed
    }

    /// Puts bag `id` back at `at` after a failed push and redraws it
    /// upright (research 02 §5.5).
    fn restore_bag(&mut self, id: BagId, at: Px) {
        id.of_mut(&mut self.sim.bags).move_to(at);
        self.draw_bag(id, ImageId::BAG_STILL);
        self.sim.penalty.charge(PenaltySite::PushRestore);
    }

    /// Collects gold `id` (gold-bags spec, "Collecting gold"; research 02
    /// §5.6, `getgold`): it is drawn once more, then every living digger
    /// that draw touches scores 500, with the gold cue, and its push pause
    /// is cleared; when none does, a monster has taken it and no one
    /// scores. The gold is removed.
    ///
    /// **Quirk Q35:** the overlapping diggers score whoever pushed against
    /// the gold, so a digger wins a tie with a monster.
    pub(crate) fn collect_gold(&mut self, id: BagId) {
        let hits = self.draw_bag(id, ImageId::GOLD_3);
        self.sim.penalty.charge(PenaltySite::GoldCollected);
        let mut taken_by_digger = false;
        for sprite in hits.of(SpriteClass::Digger) {
            let Some(unit) = DiggerId::from_sprite(sprite) else {
                continue;
            };
            let record = self.sim.record_of_unit(unit);
            if !record.of(&self.sim.diggers).alive {
                continue;
            }
            let score = ScoreId::credited(self.sim.turn, unit);
            self.add_score(score, ScoreEvent::Gold);
            self.sound.push(SoundCommand::Gold);
            record.of_mut(&mut self.sim.diggers).bag_time = 0;
            taken_by_digger = true;
        }
        if !taken_by_digger {
            self.sim.monster_got_gold = true;
        }
        self.remove_bag(id);
    }

    /// The round-end tidy: the bags of quirk Q37 are removed and their
    /// sprites erased, and the bags are saved for the current player
    /// (gold-bags spec, "Bags persist per player across rounds"; research
    /// 02 §5.8, `cleanupbags`).
    pub(crate) fn tidy_bags(&mut self) {
        for id in BagId::ALL {
            if id.of(&self.sim.bags).tidied_away() {
                id.of_mut(&mut self.sim.bags).exists = false;
                self.gfx.erase(id.sprite());
            }
        }
        // The round end has stopped every sound already.
        self.sim.bag_cues.fall = false;
        self.sim.bag_cues.wobble = false;
        let turn = self.sim.turn;
        *turn.of_mut(&mut self.sim.saved_bags) = self.sim.bags;
    }

    /// Puts resting bag `id` at `at` and places its sprite, for the test
    /// kit.
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn put_bag(&mut self, id: BagId, at: Px) {
        let mut bag = Bag::ZEROED;
        bag.exists = true;
        bag.direction = None;
        bag.wobble = WOBBLE_COUNT;
        bag.unfallen = true;
        bag.move_to(at);
        *id.of_mut(&mut self.sim.bags) = bag;
        self.gfx.erase(id.sprite());
        self.gfx.place(id.sprite(), ImageId::BAG_STILL, at);
    }
}

/// Places every existing bag of `bags` with its upright image, as the full
/// draw does (research 02 §5.1, `drawbags`). A place computes no
/// collisions.
pub(crate) fn draw_bags(gfx: &mut super::gfx::Gfx, bags: &[Bag; BagId::COUNT]) {
    for (id, bag) in BagId::ALL.into_iter().zip(bags) {
        if bag.exists {
            gfx.place(id.sprite(), ImageId::BAG_STILL, bag.position);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::observer::NoopObserver;
    use crate::sim::testkit::Scenario;
    use crate::sim::{FrameInput, FrameKind, Game, GameConfig, GameMode, MonsterSlot};

    /// Runs `f` on a context over `game`.
    fn with_ctx(game: &mut Game, f: impl FnOnce(&mut Ctx<'_>)) {
        let mut obs = NoopObserver;
        let mut cx = Ctx {
            sim: &mut game.sim,
            gfx: &mut game.gfx,
            sound: &mut game.sound,
            obs: &mut obs,
        };
        f(&mut cx);
    }

    /// Steps `game` until its first tick is next.
    fn to_first_tick(game: &mut Game) {
        while !game.phase.in_round() {
            assert_eq!(game.step(&FrameInput::NONE).frame, FrameKind::Wait);
        }
    }

    fn cell(h: u8, v: u8) -> CellPos {
        CellPos::new(h, v).unwrap_or_else(|| unreachable!())
    }

    /// A bag at rest on the corner of cell (`h`, `v`).
    fn resting(h: u8, v: u8) -> Bag {
        Bag::resting_at(cell(h, v))
    }

    #[test]
    fn moving_count_includes_wobblers_and_young_gold() {
        let wobbling = Bag {
            wobbling: true,
            ..resting(2, 2)
        };
        let falling_after_wobble = Bag {
            direction: Some(Direction::Down),
            wobble: 0,
            ..wobbling
        };
        let dropped_after_slide = Bag {
            direction: Some(Direction::Down),
            wobble: 0,
            ..resting(3, 3)
        };
        let young_gold = Bag {
            gold_age: 9,
            ..resting(4, 4)
        };
        let old_gold = Bag {
            gold_age: 10,
            ..resting(5, 5)
        };
        let removed = Bag {
            exists: false,
            ..wobbling
        };
        assert!(wobbling.is_moving());
        assert!(falling_after_wobble.is_moving());
        assert!(!dropped_after_slide.is_moving());
        assert!(young_gold.is_moving());
        assert!(!old_gold.is_moving());
        assert!(!removed.is_moving());
        assert!(!resting(1, 1).is_moving());

        let mut game = Game::new(GameConfig::default(), 0);
        game.sim.bags = [
            wobbling,
            falling_after_wobble,
            dropped_after_slide,
            young_gold,
            old_gold,
            removed,
            resting(1, 1),
        ];
        assert_eq!(game.state().moving_bags(), 3);
    }

    #[test]
    fn quirk_q37_tidy_removes_wobbling_broken_offgrid_and_7_9_bags() {
        let mut game = Game::new(GameConfig::default(), 0);
        to_first_tick(&mut game);
        let mut off_grid = resting(4, 4);
        off_grid.move_to(Px::new(96, 90));
        let bags = [
            resting(7, 9),
            Bag {
                wobbling: true,
                ..resting(1, 1)
            },
            Bag {
                gold_age: 30,
                ..resting(2, 2)
            },
            off_grid,
            Bag {
                cells_fallen: 1,
                ..resting(5, 5)
            },
            resting(5, 0),
            Bag {
                exists: false,
                ..resting(6, 6)
            },
        ];
        game.sim.bags = bags;
        for (id, bag) in BagId::ALL.into_iter().zip(&bags) {
            game.gfx.erase(id.sprite());
            if bag.exists {
                game.gfx
                    .place(id.sprite(), ImageId::BAG_STILL, bag.position);
            }
        }
        with_ctx(&mut game, |cx| {
            cx.tidy_bags();
        });
        let exists: Vec<bool> = game.sim.bags.iter().map(|bag| bag.exists).collect();
        assert_eq!(exists, [false, false, false, false, false, true, false]);
        for (id, bag) in BagId::ALL.into_iter().zip(&game.sim.bags) {
            assert_eq!(game.gfx.sprites().is_enabled(id.sprite()), bag.exists);
        }
        // The kept and removed bags are saved for the player, removed ones
        // with their last values.
        let first = PlayerId::ALL[0];
        assert_eq!(*first.of(&game.sim.saved_bags), game.sim.bags);
        assert_eq!(game.sim.bags[2].gold_age, 30);
    }

    #[test]
    fn fall_stop_emitted_when_last_falling_bag_lands() {
        // A bag over a one-cell shaft: falling on ticks 17 to 19, landing
        // on tick 20.
        let mut rows = ["               "; 10];
        rows[1] = "   B           ";
        rows[2] = "   V           ";
        let config = Scenario::new(GameMode::OnePlayer).rows(rows).config();
        let mut game = Game::new(config, 0);
        to_first_tick(&mut game);
        let mut events = Vec::new();
        for tick in 1..=21 {
            game.step(&FrameInput::NONE);
            events.extend(
                game.drain_sound()
                    .filter(|s| matches!(s, SoundCommand::FallStop | SoundCommand::FallStart))
                    .map(|s| (tick, s)),
            );
        }
        // Started on tick 17, stopped on the landing tick 20 only: a quiet
        // field sends nothing.
        assert_eq!(
            events,
            [(17, SoundCommand::FallStart), (20, SoundCommand::FallStop)]
        );
        assert_eq!(game.sim.bags[0].direction, None);
    }

    #[test]
    fn wobble_stop_follows_the_last_wobble() {
        let mut rows = ["               "; 10];
        rows[1] = "   B           ";
        rows[2] = "   V           ";
        let config = Scenario::new(GameMode::OnePlayer).rows(rows).config();
        let mut game = Game::new(config, 0);
        to_first_tick(&mut game);
        let mut wobble_stops = Vec::new();
        for tick in 1..=20 {
            game.step(&FrameInput::NONE);
            if game.drain_sound().any(|s| s == SoundCommand::WobbleStop) {
                wobble_stops.push(tick);
            }
        }
        // The fall that follows the wobble does not need the wobble cue.
        assert_eq!(wobble_stops, [17]);
    }

    #[test]
    fn quirk_q35_digger_wins_gold_tie_with_monster() {
        for digger_on_gold in [true, false] {
            let mut game = Game::new(GameConfig::default(), 0);
            to_first_tick(&mut game);
            let id = BagId::ALL[0];
            let at = Px::new(100, 100);
            let mut gold = resting(1, 1);
            gold.move_to(at);
            gold.gold_age = 20;
            game.sim.bags[0] = gold;
            game.gfx.place(id.sprite(), ImageId::GOLD_3, at);
            let unit = DiggerId::ALL[0];
            let digger_at = if digger_on_gold {
                Px::new(104, 100)
            } else {
                Px::new(200, 180)
            };
            game.sim.diggers[0].position = digger_at;
            game.gfx.erase(unit.sprite());
            game.gfx.place(
                unit.sprite(),
                ImageId::new(1).unwrap_or(ImageId::BLANK),
                digger_at,
            );
            game.sim.monster_got_gold = false;
            let mut pushed = false;
            with_ctx(&mut game, |cx| {
                // A monster's move to the left of the gold touches it.
                let slot = MonsterSlot::ALL[0];
                let hits = cx
                    .gfx
                    .move_monster(slot, ImageId::NOBBIN_1, Px::new(88, 100));
                assert!(hits.contains(id.sprite()));
                pushed = cx.push_bags(Direction::Right, hits);
            });
            assert!(pushed, "gold never blocks");
            assert!(!game.sim.bags[0].exists);
            let score = game.sim.scores.visible(ScoreId::ALL[0]);
            if digger_on_gold {
                assert_eq!(score, 500, "the digger scores");
                assert!(!game.sim.monster_got_gold, "the monster gets nothing");
            } else {
                assert_eq!(score, 0);
                assert!(game.sim.monster_got_gold, "the monster took it");
            }
        }
    }

    #[test]
    fn gold_lifetime_table_140_to_50() {
        let times: Vec<u16> = (1..=10).map(|n| gold_time(LevelNumber::new(n))).collect();
        assert_eq!(times, [140, 130, 120, 110, 100, 90, 80, 70, 60, 50]);
        // Levels beyond 10 stay at difficulty 10; the level 0 placeholder
        // gives 150.
        assert_eq!(gold_time(LevelNumber::new(57)), 50);
        assert_eq!(gold_time(None), 150);
    }
}
