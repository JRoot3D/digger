//! Per-slot controls, the direction resolver, the fire latch and movement
//! samples (design D8).
//!
//! The caller hands the simulation raw control states once per frame in a
//! [`FrameInput`]. On every tick each control slot resolves its four
//! direction controls to one direction or none, and records the fire
//! control as *pending*; the pending fire state becomes *active* once per
//! tick, so fire read on tick N takes effect on tick N+1 (digger spec,
//! "Direction resolution from held controls", "Controls are reset at round
//! start", "Fire control is latched one tick late"; research 05 §1.6, §1.7;
//! compat-quirks Q15).

use super::geom::Direction;
use super::ids::SlotId;

/// The raw state of one control slot on one frame.
///
/// A control counts as held when it is down now **or** was pressed at any
/// moment since the previous frame: the frontend ORs "pressed since the
/// last frame" into "held now" before calling the step, so a tap shorter
/// than a frame is still seen once.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "five independent controls, as the original and eDRF recordings hold them (design D2)"
)]
pub struct Controls {
    /// The up control.
    pub up: bool,
    /// The down control.
    pub down: bool,
    /// The left control.
    pub left: bool,
    /// The right control.
    pub right: bool,
    /// The fire control.
    pub fire: bool,
}

impl Controls {
    /// No control held.
    pub const NONE: Self = Self {
        up: false,
        down: false,
        left: false,
        right: false,
        fire: false,
    };

    /// Whether the control for `direction` is held.
    pub const fn holds(self, direction: Direction) -> bool {
        match direction {
            Direction::Up => self.up,
            Direction::Down => self.down,
            Direction::Left => self.left,
            Direction::Right => self.right,
        }
    }
}

/// Everything the caller supplies for one frame.
///
/// Slot 1 (`slots[0]`) drives the first digger and both players of an
/// alternating game; slot 2 (`slots[1]`) drives the second digger of a
/// two-digger game. Controls are read on tick frames only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct FrameInput {
    /// The raw controls of the two slots.
    pub slots: [Controls; 2],
    /// A pause request: pauses on a main-phase tick, resumes while paused.
    pub pause: bool,
    /// A quit request: ends the game on this frame.
    pub quit: bool,
}

impl FrameInput {
    /// No control held and no request.
    pub const NONE: Self = Self {
        slots: [Controls::NONE; 2],
        pause: false,
        quit: false,
    };

    /// The raw controls of `slot`.
    pub fn slot(&self, slot: SlotId) -> Controls {
        *slot.of(&self.slots)
    }
}

/// One movement sample: what a digger reads when it can move on a tick.
///
/// This is what a DRF direction record holds. A sample replaced from
/// outside overwrites the slot's pending fire state, which still becomes
/// active only on the next tick (design D8).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Movement {
    /// The resolved direction, or none.
    pub direction: Option<Direction>,
    /// The fire control state read on this tick (the pending fire state).
    pub fire: bool,
}

/// The order in which presses beginning on the same tick are ranked: the
/// last one wins, so right beats left beats down beats up (research 05
/// §1.6).
const PRESS_ORDER: [Direction; 4] = [
    Direction::Up,
    Direction::Down,
    Direction::Left,
    Direction::Right,
];

/// The remembered control state of one slot: direction resolver and fire
/// latch.
///
/// All of it is cleared at round start by [`SlotInput::reset`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct SlotInput {
    /// The direction controls held on the previous read.
    held: Controls,
    /// The held direction that was pressed most recently, if any.
    latest: Option<Direction>,
    /// The direction resolved by the last read.
    resolved: Option<Direction>,
    /// The fire state read on the current tick.
    fire_pending: bool,
    /// The fire state read on the previous tick, which decides firing.
    fire_active: bool,
}

impl SlotInput {
    /// A slot with nothing remembered.
    pub(crate) const fn new() -> Self {
        Self {
            held: Controls::NONE,
            latest: None,
            resolved: None,
            fire_pending: false,
            fire_active: false,
        }
    }

    /// Forgets everything, including both fire states, so a control held
    /// across the round boundary counts as newly pressed on the next read.
    pub(crate) fn reset(&mut self) {
        *self = Self::new();
    }

    /// Reads the slot's raw controls for this tick: resolves the direction
    /// and records the fire control as pending.
    ///
    /// A press of a control not held on the previous read makes its
    /// direction the latest; presses on the same read are ranked up, down,
    /// left, right with the last winning. When the latest direction's
    /// control is released, the still-held control with the highest
    /// priority right, left, down, up takes over. A direction pressed on
    /// this read is resolved for this read even if it is no longer the
    /// latest afterwards (the one-tick pulse of the original). Releasing a
    /// control that is not the latest changes nothing.
    pub(crate) fn read(&mut self, raw: Controls) {
        let mut pulse = None;
        for direction in PRESS_ORDER {
            if raw.holds(direction) && !self.held.holds(direction) {
                pulse = Some(direction);
                self.latest = Some(direction);
            }
        }
        if let Some(latest) = self.latest
            && self.held.holds(latest)
            && !raw.holds(latest)
        {
            self.latest = None;
            for direction in PRESS_ORDER {
                if raw.holds(direction) {
                    pulse = Some(direction);
                    self.latest = Some(direction);
                }
            }
        }
        self.held = Controls { fire: false, ..raw };
        self.resolved = self.latest.or(pulse);
        self.fire_pending = raw.fire;
    }

    /// The direction resolved by the last read.
    #[allow(dead_code, reason = "read by the resolver's unit tests")]
    pub(crate) const fn resolved(&self) -> Option<Direction> {
        self.resolved
    }

    /// The movement sample of this tick: the resolved direction and the
    /// pending fire state.
    pub(crate) const fn sample(&self) -> Movement {
        Movement {
            direction: self.resolved,
            fire: self.fire_pending,
        }
    }

    /// Takes a (possibly replaced) movement sample: its fire state becomes
    /// the pending fire state, and its direction is returned for the
    /// digger to use.
    pub(crate) fn apply_sample(&mut self, sample: Movement) -> Option<Direction> {
        self.fire_pending = sample.fire;
        sample.direction
    }

    /// Moves the pending fire state to active; called once per tick,
    /// between one tick's reads and the next tick's fire decisions.
    pub(crate) fn advance_fire(&mut self) {
        self.fire_active = self.fire_pending;
    }

    /// The fire state that decides whether the slot's digger fires on this
    /// tick: the one read on the previous tick.
    pub(crate) const fn fire_active(&self) -> bool {
        self.fire_active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UP: Controls = Controls {
        up: true,
        ..Controls::NONE
    };
    const RIGHT: Controls = Controls {
        right: true,
        ..Controls::NONE
    };

    /// The direction controls named by `keys`, a subset of "UDLR".
    fn held(keys: &str) -> Controls {
        Controls {
            up: keys.contains('U'),
            down: keys.contains('D'),
            left: keys.contains('L'),
            right: keys.contains('R'),
            fire: false,
        }
    }

    fn read(slot: &mut SlotInput, raw: Controls) -> Option<Direction> {
        slot.read(raw);
        slot.resolved()
    }

    #[test]
    fn last_pressed_wins() {
        let mut slot = SlotInput::new();
        assert_eq!(read(&mut slot, RIGHT), Some(Direction::Right));
        assert_eq!(read(&mut slot, RIGHT), Some(Direction::Right));
        let both = held("UR");
        assert_eq!(read(&mut slot, both), Some(Direction::Up));
        assert_eq!(read(&mut slot, both), Some(Direction::Up));
    }

    #[test]
    fn same_tick_presses_rank_right_last() {
        let mut slot = SlotInput::new();
        assert_eq!(read(&mut slot, held("UDLR")), Some(Direction::Right));
        let mut slot = SlotInput::new();
        assert_eq!(read(&mut slot, held("UD")), Some(Direction::Down));
    }

    #[test]
    fn fallback_after_release_prefers_down_over_up() {
        let mut slot = SlotInput::new();
        read(&mut slot, held("U"));
        read(&mut slot, held("UD"));
        assert_eq!(read(&mut slot, held("UDL")), Some(Direction::Left));
        assert_eq!(read(&mut slot, held("UD")), Some(Direction::Down));
    }

    #[test]
    fn fallback_priority_is_right_left_down_up() {
        let mut slot = SlotInput::new();
        read(&mut slot, held("UDLR"));
        // Right won; press order no longer matters for the fallback.
        assert_eq!(read(&mut slot, held("UDL")), Some(Direction::Left));
        assert_eq!(read(&mut slot, held("UD")), Some(Direction::Down));
        assert_eq!(read(&mut slot, held("U")), Some(Direction::Up));
        assert_eq!(read(&mut slot, Controls::NONE), None);
    }

    #[test]
    fn short_tap_lasts_one_tick() {
        let mut slot = SlotInput::new();
        assert_eq!(read(&mut slot, UP), Some(Direction::Up));
        assert_eq!(read(&mut slot, Controls::NONE), None);
        assert_eq!(read(&mut slot, Controls::NONE), None);
    }

    #[test]
    fn releasing_non_winner_changes_nothing() {
        let mut slot = SlotInput::new();
        read(&mut slot, RIGHT);
        assert_eq!(read(&mut slot, held("UR")), Some(Direction::Up));
        assert_eq!(read(&mut slot, UP), Some(Direction::Up));
        assert_eq!(read(&mut slot, UP), Some(Direction::Up));
    }

    #[test]
    fn held_key_counts_as_new_press_after_reset() {
        let both = held("DL");
        let mut slot = SlotInput::new();
        read(&mut slot, held("L"));
        assert_eq!(read(&mut slot, both), Some(Direction::Down));
        // Without a reset the earlier press keeps winning.
        assert_eq!(read(&mut slot, both), Some(Direction::Down));
        slot.reset();
        // After the reset both are new presses on one read: left ranks last.
        assert_eq!(read(&mut slot, both), Some(Direction::Left));

        let mut slot = SlotInput::new();
        let left = held("L");
        read(&mut slot, left);
        slot.reset();
        assert_eq!(slot.resolved(), None);
        assert_eq!(read(&mut slot, left), Some(Direction::Left));
    }

    #[test]
    fn fire_read_on_tick_n_is_active_on_tick_n_plus_1() {
        let fire = Controls {
            fire: true,
            ..Controls::NONE
        };
        let mut slot = SlotInput::new();
        // Each tick: advance the latch, then read the slot.
        let tick = |slot: &mut SlotInput, raw: Controls| {
            slot.advance_fire();
            slot.read(raw);
            slot.fire_active()
        };
        assert!(!tick(&mut slot, Controls::NONE)); // tick 19
        assert!(!tick(&mut slot, fire)); // tick 20: read, not active yet
        assert!(slot.sample().fire);
        assert!(tick(&mut slot, Controls::NONE)); // tick 21: active
        assert!(!tick(&mut slot, Controls::NONE)); // tick 22

        // A replaced sample overwrites the pending state.
        tick(&mut slot, Controls::NONE);
        let direction = slot.apply_sample(Movement {
            direction: Some(Direction::Left),
            fire: true,
        });
        assert_eq!(direction, Some(Direction::Left));
        assert!(!slot.fire_active());
        assert!(tick(&mut slot, Controls::NONE));

        // Round start clears both states.
        tick(&mut slot, fire);
        slot.reset();
        assert!(!tick(&mut slot, Controls::NONE));
    }

    #[test]
    fn sample_holds_resolved_direction_and_pending_fire() {
        let mut slot = SlotInput::new();
        slot.read(Controls {
            fire: true,
            ..RIGHT
        });
        assert_eq!(
            slot.sample(),
            Movement {
                direction: Some(Direction::Right),
                fire: true
            }
        );
    }

    #[test]
    fn frame_input_slots_by_id() {
        let input = FrameInput {
            slots: [UP, RIGHT],
            ..FrameInput::NONE
        };
        assert_eq!(input.slot(SlotId::ALL[0]), UP);
        assert_eq!(input.slot(SlotId::ALL[1]), RIGHT);
        assert_eq!(FrameInput::default(), FrameInput::NONE);
    }
}
