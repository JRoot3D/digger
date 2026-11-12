//! The cast animation of the title screen's right-hand panel: a script of
//! 287 frames (0 to 286) as a data table, and the interpreter that plays it
//! on the title screen's own picture and sprite layer (design D2;
//! `desktop-app` spec, title animation; research 03 §9.3).
//!
//! The animation is purely cosmetic: it draws with the core's sprite layer
//! and text primitives but uses no simulation object, reads no collision
//! list and consumes no random number.
//!
//! # Frame order
//!
//! On every frame the interpreter (1) clears the cast when the frame is 0,
//! (2) applies every script entry whose frame span contains the frame, in
//! table order, and (3) animates each actor that appeared on an earlier
//! frame. The table is in the original's order, which is not sorted by
//! frame: where two entries share a frame, the earlier one acts first.
//!
//! # State kept across cycles
//!
//! The cycle restart only removes the actors. The Digger's and the
//! fireball's animation phases, the images the sprite slots last showed,
//! and the gold bag and bonus sprites (which are cleared off the picture
//! by the text erase but never erased from the layer) carry over into the
//! next cycle, as in the original; they are reset only when the title
//! screen is opened again.

use digger_core::Direction;
use digger_core::assets::ImageId;
use digger_core::geom::Px;
use digger_core::sprite::{SpriteId, SpriteLayer};
use digger_core::video::{Framebuffer, TextColor};

/// The last frame of the cycle; the frame after it is frame 0 again.
pub(crate) const LAST_FRAME: u16 = 286;

/// The text rows cleared when the cycle starts: 12 cells from x = 164 on
/// every 12th row from y = 54 to y = 162 (research 03 §9.3).
const CLEAR_X: i32 = 164;
/// The first row cleared.
const CLEAR_FIRST_Y: i32 = 54;
/// Below the last row cleared.
const CLEAR_END_Y: i32 = 174;
/// The cells cleared on each row.
const CLEAR_CELLS: usize = 12;
/// The distance between two cleared rows.
const CLEAR_STEP: usize = 12;

/// The labels' text colour.
const LABEL_COLOR: TextColor = TextColor::Two;

/// `id`, known to exist: the slot constructors only fail for numbers out
/// of range, which the constants below never use (a test checks them).
const fn slot(id: Option<SpriteId>) -> SpriteId {
    match id {
        Some(id) => id,
        None => SpriteId::BONUS,
    }
}

/// The sprite of the first monster (the nobbin).
const FIRST_MONSTER: SpriteId = slot(SpriteId::monster(0));
/// The sprite of the second monster (the one that becomes a hobbin).
const SECOND_MONSTER: SpriteId = slot(SpriteId::monster(1));
/// The Digger's sprite.
const DIGGER: SpriteId = slot(SpriteId::digger(0));
/// The fireball's sprite.
const FIREBALL: SpriteId = slot(SpriteId::fireball(0));
/// The gold bag's sprite.
const BAG: SpriteId = slot(SpriteId::bag(0));
/// The bonus sprite.
const BONUS: SpriteId = SpriteId::BONUS;

/// The image numbered `n`; the arithmetic below only forms numbers of the
/// table (a test walks every image the cast can show).
fn image(n: u8) -> ImageId {
    ImageId::new(n).unwrap_or(ImageId::BLANK)
}

/// One of the two monsters of the cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Who {
    /// The nobbin on the top row.
    First,
    /// The second nobbin, which bumps into the first and turns into a
    /// hobbin.
    Second,
}

impl Who {
    /// The monster's sprite slot.
    const fn sprite(self) -> SpriteId {
        match self {
            Self::First => FIRST_MONSTER,
            Self::Second => SECOND_MONSTER,
        }
    }
}

/// What a script entry does on each frame of its span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// Erase the cast panel's text rows.
    ClearText,
    /// A nobbin appears at a position, facing a direction; nothing happens
    /// if that monster is already there.
    Spawn(Who, Px, Direction),
    /// The monster moves by (dx, dy); it is drawn there when animated.
    MoveMonster(Who, i32, i32),
    /// The monster faces another direction.
    TurnMonster(Who, Direction),
    /// A nobbin becomes a hobbin, or a hobbin a nobbin.
    Mutate(Who),
    /// The monster dies and leaves the picture.
    Kill(Who),
    /// The Digger appears at a position, facing a direction, able to fire.
    SpawnDigger(Px, Direction),
    /// The Digger moves by (dx, dy).
    MoveDigger(i32, i32),
    /// The Digger faces another direction.
    TurnDigger(Direction),
    /// The Digger has fired and cannot fire again yet.
    Discharge,
    /// The Digger can fire again.
    Recharge,
    /// A fireball leaves the Digger, offset from its position by (dx, dy);
    /// a fireball already flying is removed first.
    Fire(i32, i32),
    /// The fireball moves by (dx, dy) unless it is exploding.
    MoveFireball(i32, i32),
    /// The fireball starts to explode.
    Explode,
    /// The fireball leaves the picture.
    RemoveFireball,
    /// A label in colour 2.
    Label(&'static str, Px),
    /// A gold bag, standing still.
    Gold(Px),
    /// An emerald, drawn into the background.
    Emerald(Px),
    /// The bonus sprite.
    Bonus(Px),
}

/// One line of the script: a step applied on every frame from `first` to
/// `last`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScriptEntry {
    /// The first frame the step applies on.
    first: u16,
    /// The last frame the step applies on.
    last: u16,
    /// What it does.
    step: Step,
}

/// A step on one frame.
const fn at(frame: u16, step: Step) -> ScriptEntry {
    ScriptEntry {
        first: frame,
        last: frame,
        step,
    }
}

/// A step on every frame from `first` to `last`.
const fn span(first: u16, last: u16, step: Step) -> ScriptEntry {
    ScriptEntry { first, last, step }
}

/// The animation script, in the original's order (research 03 §9.3).
const SCRIPT: &[ScriptEntry] = &[
    at(0, Step::ClearText),
    // The nobbin comes in from the right and turns round.
    at(
        50,
        Step::Spawn(Who::First, Px::new(292, 63), Direction::Left),
    ),
    span(51, 77, Step::MoveMonster(Who::First, -4, 0)),
    at(78, Step::TurnMonster(Who::First, Direction::Right)),
    at(83, Step::Label("NOBBIN", Px::new(216, 64))),
    // A second nobbin comes in on the row below, goes up into the first,
    // back and forth, turns into a hobbin and goes back down.
    at(
        90,
        Step::Spawn(Who::Second, Px::new(292, 82), Direction::Left),
    ),
    span(91, 117, Step::MoveMonster(Who::Second, -4, 0)),
    span(118, 126, Step::MoveMonster(Who::Second, 0, -3)),
    span(129, 134, Step::MoveMonster(Who::Second, 0, 3)),
    span(137, 142, Step::MoveMonster(Who::Second, 0, -3)),
    at(145, Step::Mutate(Who::Second)),
    span(146, 154, Step::MoveMonster(Who::Second, 0, 3)),
    at(155, Step::TurnMonster(Who::Second, Direction::Right)),
    at(156, Step::Label("HOBBIN", Px::new(216, 83))),
    // The Digger comes in on the third row.
    at(130, Step::SpawnDigger(Px::new(292, 101), Direction::Left)),
    span(131, 157, Step::MoveDigger(-4, 0)),
    at(163, Step::Label("DIGGER", Px::new(216, 102))),
    at(166, Step::TurnDigger(Direction::Right)),
    at(228, Step::TurnDigger(Direction::Up)),
    // Gold, emerald and bonus are shown below.
    at(178, Step::Gold(Px::new(184, 120))),
    at(183, Step::Label("GOLD", Px::new(216, 121))),
    at(198, Step::Emerald(Px::new(184, 141))),
    at(203, Step::Label("EMERALD", Px::new(216, 140))),
    at(218, Step::Bonus(Px::new(184, 158))),
    at(223, Step::Label("BONUS", Px::new(216, 159))),
    // The Digger shoots the hobbin...
    at(232, Step::Discharge),
    at(232, Step::Fire(4, 0)),
    span(233, 237, Step::MoveFireball(0, -4)),
    at(237, Step::Kill(Who::Second)),
    at(238, Step::Explode),
    at(243, Step::RemoveFireball),
    at(247, Step::Recharge),
    // ...and then the nobbin.
    at(251, Step::Discharge),
    at(251, Step::Fire(4, 0)),
    span(252, 260, Step::MoveFireball(0, -4)),
    at(260, Step::Kill(Who::First)),
    at(261, Step::Explode),
    at(266, Step::RemoveFireball),
    at(270, Step::Recharge),
    at(274, Step::TurnDigger(Direction::Right)),
];

/// An animation phase that runs 0, 1, 2, 1, 0, 1, … one step per call,
/// for the walking frames of monsters and the Digger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Bounce {
    phase: i8,
    step: i8,
}

impl Bounce {
    /// At phase 0, rising.
    const START: Self = Self { phase: 0, step: 1 };

    /// Advances one step and returns the new phase, 0 to 2.
    fn advance(&mut self) -> u8 {
        self.phase += self.step;
        if self.phase == 0 || self.phase == 2 {
            self.step = -self.step;
        }
        self.phase = self.phase.clamp(0, 2);
        self.phase.unsigned_abs()
    }
}

/// A monster of the cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Monster {
    /// Its position; drawn there when animated.
    pos: Px,
    /// The direction it faces.
    facing: Direction,
    /// A nobbin rather than a hobbin.
    nobbin: bool,
    /// Still alive (a killed monster is not drawn again).
    alive: bool,
    /// Its walking phase.
    walk: Bounce,
    /// The frame it appeared on; it is animated from the frame after.
    since: u16,
}

impl Monster {
    /// The image the monster shows: a nobbin's walking frame, or a hobbin's
    /// facing right or left. A hobbin facing up or down keeps the image it
    /// had (none of the script's turns does that).
    fn image(&self, phase: u8) -> Option<ImageId> {
        if self.nobbin {
            return Some(image(ImageId::NOBBIN_1.number() + phase));
        }
        match self.facing {
            Direction::Right => Some(image(ImageId::HOBBIN_RIGHT_1.number() + phase)),
            Direction::Left => Some(image(ImageId::HOBBIN_LEFT_1.number() + phase)),
            Direction::Up | Direction::Down => None,
        }
    }
}

/// The Digger of the cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Digger {
    pos: Px,
    facing: Direction,
    can_fire: bool,
    since: u16,
}

/// The fireball of the cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fireball {
    pos: Px,
    /// 0 while flying, then 1, 2, 3 for the explosion's images, 4 once
    /// it is over (the last image stays until the fireball is removed).
    explosion: u8,
    since: u16,
}

/// Image of the Digger facing `facing`, able to fire or not, in walking
/// phase `phase` (research 03 §5.2: three images per direction and state,
/// right, up, left, down, each first able to fire then not).
fn digger_image(facing: Direction, can_fire: bool, phase: u8) -> ImageId {
    let direction: u8 = match facing {
        Direction::Right => 0,
        Direction::Up => 1,
        Direction::Left => 2,
        Direction::Down => 3,
    };
    let group = direction * 2 + u8::from(!can_fire);
    image(group * 3 + phase + 1)
}

/// The cast and everything the animation keeps between frames.
#[derive(Debug, Clone)]
pub(crate) struct Cast {
    monsters: [Option<Monster>; 2],
    digger: Option<Digger>,
    fireball: Option<Fireball>,
    /// The Digger's walking phase; kept across cycles.
    digger_walk: Bounce,
    /// The fireball's flying image, 0 to 2; kept across cycles.
    fireball_frame: u8,
}

impl Cast {
    /// No actor on stage; the sprite slots are prepared on `sprites`.
    pub(crate) fn new(sprites: &mut SpriteLayer) -> Self {
        // As when a level is drawn: every slot forgotten with its class's
        // resting image (research 03 §7.3).
        for i in 0..7 {
            if let Some(id) = SpriteId::bag(i) {
                sprites.reset(id, ImageId::BAG_STILL);
            }
        }
        for i in 0..6 {
            if let Some(id) = SpriteId::monster(i) {
                sprites.reset(id, ImageId::NOBBIN_3);
            }
        }
        for i in 0..2 {
            if let Some(id) = SpriteId::digger(i) {
                sprites.reset(id, ImageId::BLANK);
            }
            if let Some(id) = SpriteId::fireball(i) {
                sprites.reset(id, ImageId::FIREBALL_1);
            }
        }
        sprites.reset(BONUS, ImageId::BONUS);
        Self {
            monsters: [None; 2],
            digger: None,
            fireball: None,
            digger_walk: Bounce::START,
            fireball_frame: 0,
        }
    }

    /// Plays `frame` (0 to [`LAST_FRAME`]) on `screen`.
    pub(crate) fn step(&mut self, frame: u16, screen: &mut Framebuffer, sprites: &mut SpriteLayer) {
        let mut stage = Stage { screen, sprites };
        if frame == 0 {
            self.remove_all(&mut stage);
        }
        for entry in SCRIPT {
            if (entry.first..=entry.last).contains(&frame) {
                self.apply(entry.step, frame, &mut stage);
            }
        }
        self.animate(frame, &mut stage);
    }

    /// Takes every actor off the stage.
    fn remove_all(&mut self, stage: &mut Stage<'_>) {
        self.remove_fireball(stage);
        if self.digger.take().is_some() {
            stage.sprites.erase(stage.screen, DIGGER);
        }
        for who in [Who::First, Who::Second] {
            if self.monster_slot(who).take().is_some() {
                stage.sprites.erase(stage.screen, who.sprite());
            }
        }
    }

    const fn monster_slot(&mut self, who: Who) -> &mut Option<Monster> {
        match who {
            Who::First => &mut self.monsters[0],
            Who::Second => &mut self.monsters[1],
        }
    }

    /// Applies one script step on `frame`.
    fn apply(&mut self, step: Step, frame: u16, stage: &mut Stage<'_>) {
        match step {
            Step::ClearText => stage.clear_text(),
            Step::Spawn(who, pos, facing) => self.spawn(who, pos, facing, frame, stage),
            Step::MoveMonster(who, dx, dy) => {
                if let Some(monster) = self.monster_slot(who) {
                    monster.pos = offset(monster.pos, dx, dy);
                }
            }
            Step::TurnMonster(who, facing) => {
                if let Some(monster) = self.monster_slot(who) {
                    monster.facing = facing;
                }
            }
            Step::Mutate(who) => self.mutate(who, stage),
            Step::Kill(who) => {
                if let Some(monster) = self.monster_slot(who) {
                    monster.alive = false;
                    stage.sprites.erase(stage.screen, who.sprite());
                }
            }
            Step::SpawnDigger(pos, facing) => {
                self.digger = Some(Digger {
                    pos,
                    facing,
                    can_fire: true,
                    since: frame,
                });
                // Placed with whatever image the slot last showed.
                stage.sprites.place(stage.screen, DIGGER, pos);
            }
            Step::MoveDigger(dx, dy) => {
                if let Some(digger) = &mut self.digger {
                    digger.pos = offset(digger.pos, dx, dy);
                }
            }
            Step::TurnDigger(facing) => {
                if let Some(digger) = &mut self.digger {
                    digger.facing = facing;
                }
            }
            Step::Discharge | Step::Recharge => {
                if let Some(digger) = &mut self.digger {
                    digger.can_fire = step == Step::Recharge;
                }
            }
            Step::Fire(dx, dy) => self.fire(dx, dy, frame, stage),
            Step::MoveFireball(dx, dy) => {
                if let Some(fireball) = &mut self.fireball
                    && fireball.explosion == 0
                {
                    fireball.pos = offset(fireball.pos, dx, dy);
                }
            }
            Step::Explode => {
                if let Some(fireball) = &mut self.fireball
                    && fireball.explosion == 0
                {
                    fireball.explosion = 1;
                }
            }
            Step::RemoveFireball => self.remove_fireball(stage),
            Step::Label(text, pos) => stage.screen.draw_text(text, pos, LABEL_COLOR),
            Step::Gold(pos) => {
                stage.sprites.place(stage.screen, BAG, pos);
                stage.sprites.set_pending(BAG, ImageId::BAG_STILL);
                stage.sprites.move_to(stage.screen, BAG, pos);
            }
            Step::Emerald(pos) => stage
                .sprites
                .draw_static(stage.screen, ImageId::EMERALD, pos),
            Step::Bonus(pos) => {
                stage.sprites.set_pending(BONUS, ImageId::BONUS);
                stage.sprites.place(stage.screen, BONUS, pos);
            }
        }
    }

    /// A nobbin appears, unless that monster is already there.
    fn spawn(&mut self, who: Who, pos: Px, facing: Direction, frame: u16, stage: &mut Stage<'_>) {
        let slot = self.monster_slot(who);
        if slot.is_some() {
            return;
        }
        let monster = Monster {
            pos,
            facing,
            nobbin: true,
            alive: true,
            walk: Bounce::START,
            since: frame,
        };
        if let Some(image) = monster.image(0) {
            stage.sprites.set_pending(who.sprite(), image);
        }
        stage.sprites.place(stage.screen, who.sprite(), pos);
        *slot = Some(monster);
    }

    /// A nobbin becomes a hobbin or back, redrawn at once in its current
    /// walking phase.
    fn mutate(&mut self, who: Who, stage: &mut Stage<'_>) {
        if let Some(monster) = self.monster_slot(who) {
            monster.nobbin = !monster.nobbin;
            let phase = monster.walk.phase.unsigned_abs();
            if monster.alive
                && let Some(image) = monster.image(phase)
            {
                stage.sprites.set_pending(who.sprite(), image);
            }
            stage
                .sprites
                .move_to(stage.screen, who.sprite(), monster.pos);
        }
    }

    /// A fireball leaves the Digger, replacing one still flying.
    fn fire(&mut self, dx: i32, dy: i32, frame: u16, stage: &mut Stage<'_>) {
        let Some(digger) = self.digger else {
            return;
        };
        self.remove_fireball(stage);
        let pos = offset(digger.pos, dx, dy);
        // Placed with whatever image the slot last showed.
        stage.sprites.place(stage.screen, FIREBALL, pos);
        self.fireball = Some(Fireball {
            pos,
            explosion: 0,
            since: frame,
        });
    }

    /// The fireball, if there is one, leaves the picture.
    fn remove_fireball(&mut self, stage: &mut Stage<'_>) {
        if self.fireball.take().is_some() {
            stage.sprites.erase(stage.screen, FIREBALL);
        }
    }

    /// Draws every actor that appeared before `frame` in its next image:
    /// the monsters, the Digger, then the fireball.
    fn animate(&mut self, frame: u16, stage: &mut Stage<'_>) {
        for who in [Who::First, Who::Second] {
            if let Some(monster) = self.monster_slot(who)
                && frame > monster.since
                && monster.alive
            {
                let phase = monster.walk.advance();
                if let Some(image) = monster.image(phase) {
                    stage.sprites.set_pending(who.sprite(), image);
                }
                stage
                    .sprites
                    .move_to(stage.screen, who.sprite(), monster.pos);
            }
        }
        if let Some(digger) = self.digger
            && frame > digger.since
        {
            let phase = self.digger_walk.advance();
            stage
                .sprites
                .set_pending(DIGGER, digger_image(digger.facing, digger.can_fire, phase));
            stage.sprites.move_to(stage.screen, DIGGER, digger.pos);
        }
        if let Some(fireball) = &mut self.fireball
            && frame > fireball.since
            && fireball.explosion < 4
        {
            let image = if fireball.explosion == 0 {
                self.fireball_frame = (self.fireball_frame + 1) % 3;
                image(ImageId::FIREBALL_1.number() + self.fireball_frame)
            } else {
                image(ImageId::FIREBALL_3.number() + fireball.explosion)
            };
            stage.sprites.set_pending(FIREBALL, image);
            stage.sprites.move_to(stage.screen, FIREBALL, fireball.pos);
            if fireball.explosion > 0 {
                fireball.explosion += 1;
            }
        }
    }
}

/// `pos` moved by (`dx`, `dy`).
const fn offset(pos: Px, dx: i32, dy: i32) -> Px {
    Px::new(pos.x + dx, pos.y + dy)
}

/// The picture and the sprite layer the cast plays on.
struct Stage<'a> {
    screen: &'a mut Framebuffer,
    sprites: &'a mut SpriteLayer,
}

impl Stage<'_> {
    /// Erases the cast panel's text rows.
    fn clear_text(&mut self) {
        for y in (CLEAR_FIRST_Y..CLEAR_END_Y).step_by(CLEAR_STEP) {
            self.screen.erase_text(CLEAR_CELLS, Px::new(CLEAR_X, y));
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    #[test]
    fn cast_sprite_slots() {
        assert_eq!(FIRST_MONSTER.number(), 8);
        assert_eq!(SECOND_MONSTER.number(), 9);
        assert_eq!(FIREBALL.number(), 14);
        assert_eq!(DIGGER.number(), 16);
        assert_eq!(BAG.number(), 1);
    }

    #[test]
    fn bounce_runs_up_and_down() {
        let mut bounce = Bounce::START;
        let phases: Vec<u8> = (0..8).map(|_| bounce.advance()).collect();
        assert_eq!(phases, [1, 2, 1, 0, 1, 2, 1, 0]);
    }

    #[test]
    fn digger_images_by_direction_and_charge() {
        assert_eq!(digger_image(Direction::Right, true, 0).number(), 1);
        assert_eq!(digger_image(Direction::Right, false, 2).number(), 6);
        assert_eq!(digger_image(Direction::Up, true, 1).number(), 8);
        assert_eq!(digger_image(Direction::Up, false, 0).number(), 10);
        assert_eq!(digger_image(Direction::Left, true, 0).number(), 13);
        assert_eq!(digger_image(Direction::Down, false, 2).number(), 24);
    }

    #[test]
    fn script_spans_are_inside_the_cycle() {
        for entry in SCRIPT {
            assert!(
                entry.first <= entry.last && entry.last <= LAST_FRAME,
                "{entry:?}"
            );
        }
    }
}
