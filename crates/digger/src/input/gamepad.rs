//! Pad slot assignment and the stick-to-virtual-direction conversion, as pure
//! functions over abstract pad events (design D7).
//!
//! The platform layer turns its own gamepad events into [`PadEvent`]s; this
//! module decides which player a pad plays for and turns the left stick into
//! four virtual direction buttons, giving [`PadSignal`]s that
//! [`InputState`](super::InputState) applies like key presses.
//!
//! Slots: the first connected pad plays for player 1 and the second for
//! player 2; a pad connected while both slots are taken is ignored for as
//! long as it stays connected, and a newly connected pad takes the lowest
//! free slot (`input-controls` spec, gamepad assignment and hot-plug).
//!
//! Stick: a direction is pressed when the stick is deflected beyond the dead
//! zone in that direction and released when the deflection falls below
//! 0.8 × the dead zone, so a stick resting on the threshold does not chatter.
//! The axes are independent, so a diagonal presses two virtual buttons, in
//! the order in which they crossed the threshold; the core's resolver then
//! applies the last-pressed rule to them as to keys.

use digger_core::Direction;

use super::PadButton;

/// The fraction of the dead zone below which a pressed stick direction is
/// released.
const RELEASE_FRACTION: f64 = 0.8;

/// The platform's identifier of one connected pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct PadId(pub(crate) usize);

/// An axis of the left stick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum StickAxis {
    /// Horizontal: −1 is full left, +1 full right.
    X,
    /// Vertical: −1 is full down, +1 full up.
    Y,
}

/// What the platform reports about its pads, in the order it happened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PadEvent {
    /// A pad was connected.
    Connected(PadId),
    /// A pad was removed.
    Disconnected(PadId),
    /// A bindable button went down or up.
    Button {
        /// The pad.
        pad: PadId,
        /// The button.
        button: PadButton,
        /// Down (`true`) or up.
        pressed: bool,
    },
    /// A d-pad button went down or up.
    DPad {
        /// The pad.
        pad: PadId,
        /// The d-pad button.
        direction: Direction,
        /// Down (`true`) or up.
        pressed: bool,
    },
    /// A left-stick axis moved.
    Stick {
        /// The pad.
        pad: PadId,
        /// The axis.
        axis: StickAxis,
        /// The deflection, −1 to +1.
        value: f64,
    },
}

/// One input of a pad that can be held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PadInput {
    /// A bindable button.
    Button(PadButton),
    /// A d-pad button.
    DPad(Direction),
    /// A virtual stick direction.
    Stick(Direction),
}

/// What a pad event means for the input layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PadSignal {
    /// A pad now plays for this slot (0 or 1).
    Connected(u8),
    /// The pad of this slot was removed: its inputs count as released.
    Removed(u8),
    /// An input of the pad in `slot` went down or up.
    Input {
        /// The slot.
        slot: u8,
        /// The input.
        input: PadInput,
        /// Down (`true`) or up.
        pressed: bool,
    },
}

/// The pad playing for one slot.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SlotPad {
    /// The pad.
    id: PadId,
    /// The last deflection of each axis: `[x, y]`.
    stick: [f64; 2],
    /// Which virtual stick directions are pressed: right, up, left, down.
    pressed: [bool; 4],
}

impl SlotPad {
    /// A pad at rest.
    const fn new(id: PadId) -> Self {
        Self {
            id,
            stick: [0.0; 2],
            pressed: [false; 4],
        }
    }
}

/// The virtual-button index of `direction` in [`SlotPad::pressed`].
const fn stick_index(direction: Direction) -> usize {
    match direction {
        Direction::Right => 0,
        Direction::Up => 1,
        Direction::Left => 2,
        Direction::Down => 3,
    }
}

/// Pad slots and stick state (design D7).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Pads {
    /// The pad of each slot.
    slots: [Option<SlotPad>; 2],
    /// The dead zone, a fraction of full deflection.
    dead_zone: f64,
}

impl Pads {
    /// No pad connected; sticks use `dead_zone` (`gamepad.dead_zone`).
    pub(crate) const fn new(dead_zone: f64) -> Self {
        Self {
            slots: [None; 2],
            dead_zone,
        }
    }

    /// The slot `pad` plays for, if any.
    pub(crate) fn slot_of(&self, pad: PadId) -> Option<u8> {
        self.slots
            .iter()
            .position(|slot| slot.is_some_and(|slot| slot.id == pad))
            .and_then(|index| u8::try_from(index).ok())
    }

    /// Applies `event` and appends what it means to `out`.
    pub(crate) fn handle(&mut self, event: PadEvent, out: &mut Vec<PadSignal>) {
        match event {
            PadEvent::Connected(pad) => {
                if self.slot_of(pad).is_some() {
                    return;
                }
                if let Some((index, free)) = self
                    .slots
                    .iter_mut()
                    .enumerate()
                    .find(|(_, slot)| slot.is_none())
                {
                    *free = Some(SlotPad::new(pad));
                    if let Ok(slot) = u8::try_from(index) {
                        out.push(PadSignal::Connected(slot));
                    }
                }
            }
            PadEvent::Disconnected(pad) => {
                if let Some(slot) = self.slot_of(pad) {
                    if let Some(entry) = self.slots.get_mut(usize::from(slot)) {
                        *entry = None;
                    }
                    out.push(PadSignal::Removed(slot));
                }
            }
            PadEvent::Button {
                pad,
                button,
                pressed,
            } => {
                if let Some(slot) = self.slot_of(pad) {
                    out.push(PadSignal::Input {
                        slot,
                        input: PadInput::Button(button),
                        pressed,
                    });
                }
            }
            PadEvent::DPad {
                pad,
                direction,
                pressed,
            } => {
                if let Some(slot) = self.slot_of(pad) {
                    out.push(PadSignal::Input {
                        slot,
                        input: PadInput::DPad(direction),
                        pressed,
                    });
                }
            }
            PadEvent::Stick { pad, axis, value } => {
                let dead_zone = self.dead_zone;
                let Some(slot) = self.slot_of(pad) else {
                    return;
                };
                let Some(Some(entry)) = self.slots.get_mut(usize::from(slot)) else {
                    return;
                };
                stick_moved(entry, slot, axis, value, dead_zone, out);
            }
        }
    }
}

/// Updates one axis of `pad` to `value` and appends the virtual-button
/// changes: releases first, then presses.
fn stick_moved(
    pad: &mut SlotPad,
    slot: u8,
    axis: StickAxis,
    value: f64,
    dead_zone: f64,
    out: &mut Vec<PadSignal>,
) {
    let value = if value.is_finite() {
        value.clamp(-1.0, 1.0)
    } else {
        0.0
    };
    let (positive, negative, index) = match axis {
        StickAxis::X => (Direction::Right, Direction::Left, 0),
        StickAxis::Y => (Direction::Up, Direction::Down, 1),
    };
    if let Some(stored) = pad.stick.get_mut(index) {
        *stored = value;
    }
    let wanted = |deflection: f64, pressed: bool| {
        if pressed {
            deflection >= RELEASE_FRACTION * dead_zone
        } else {
            deflection > dead_zone
        }
    };
    let mut changes = [(positive, value), (negative, -value)].map(|(direction, deflection)| {
        let was = pad
            .pressed
            .get(stick_index(direction))
            .copied()
            .unwrap_or(false);
        (direction, was, wanted(deflection, was))
    });
    // Releases before presses, so a stick flicked from one side to the
    // other never holds both.
    changes.sort_by_key(|&(_, _, now)| now);
    for (direction, was, now) in changes {
        if was != now {
            if let Some(flag) = pad.pressed.get_mut(stick_index(direction)) {
                *flag = now;
            }
            out.push(PadSignal::Input {
                slot,
                input: PadInput::Stick(direction),
                pressed: now,
            });
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use digger_core::Direction;

    use super::{PadEvent, PadId, PadInput, PadSignal, Pads, StickAxis};
    use crate::config::Settings;
    use crate::input::{Bindings, InputState, PadButton, PadChange};

    const PAD_A: PadId = PadId(7);
    const PAD_B: PadId = PadId(3);
    const PAD_C: PadId = PadId(9);

    fn run(pads: &mut Pads, events: &[PadEvent]) -> Vec<PadSignal> {
        let mut out = Vec::new();
        for &event in events {
            pads.handle(event, &mut out);
        }
        out
    }

    fn stick(pad: PadId, axis: StickAxis, value: f64) -> PadEvent {
        PadEvent::Stick { pad, axis, value }
    }

    fn stick_signal(direction: Direction, pressed: bool) -> PadSignal {
        PadSignal::Input {
            slot: 0,
            input: PadInput::Stick(direction),
            pressed,
        }
    }

    fn connected(pad: PadId) -> Pads {
        let mut pads = Pads::new(0.5);
        assert_eq!(
            run(&mut pads, &[PadEvent::Connected(pad)]),
            [PadSignal::Connected(0)]
        );
        pads
    }

    #[test]
    fn stick_dead_zone() {
        let mut pads = connected(PAD_A);
        // Drift at rest gives nothing.
        assert_eq!(run(&mut pads, &[stick(PAD_A, StickAxis::X, 0.12)]), []);
        assert_eq!(run(&mut pads, &[stick(PAD_A, StickAxis::Y, -0.3)]), []);
        // Exactly on the dead zone is not beyond it.
        assert_eq!(run(&mut pads, &[stick(PAD_A, StickAxis::X, 0.5)]), []);
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, 0.51)]),
            [stick_signal(Direction::Right, true)]
        );
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::Y, -0.9)]),
            [stick_signal(Direction::Down, true)]
        );
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::Y, 0.0)]),
            [stick_signal(Direction::Down, false)]
        );
        // A different dead zone.
        let mut pads = Pads::new(0.2);
        let _ = run(&mut pads, &[PadEvent::Connected(PAD_A)]);
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, -0.25)]),
            [stick_signal(Direction::Left, true)]
        );
        // Values outside the range are clamped, NaN is rest.
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, f64::NAN)]),
            [stick_signal(Direction::Left, false)]
        );
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::Y, 7.0)]),
            [stick_signal(Direction::Up, true)]
        );
    }

    #[test]
    fn stick_hysteresis() {
        let mut pads = connected(PAD_A);
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, 0.6)]),
            [stick_signal(Direction::Right, true)]
        );
        // Wobbling around the threshold, down to 0.8 × 0.5 = 0.4: held.
        for value in [0.49, 0.52, 0.45, 0.41, 0.4, 0.55, 0.42] {
            assert_eq!(run(&mut pads, &[stick(PAD_A, StickAxis::X, value)]), []);
        }
        // Below 0.4: released, and not pressed again until beyond 0.5.
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, 0.39)]),
            [stick_signal(Direction::Right, false)]
        );
        for value in [0.45, 0.5, 0.42] {
            assert_eq!(run(&mut pads, &[stick(PAD_A, StickAxis::X, value)]), []);
        }
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, 0.501)]),
            [stick_signal(Direction::Right, true)]
        );
        // Flicked to the other side in one report: release, then press.
        assert_eq!(
            run(&mut pads, &[stick(PAD_A, StickAxis::X, -1.0)]),
            [
                stick_signal(Direction::Right, false),
                stick_signal(Direction::Left, true)
            ]
        );
    }

    #[test]
    fn stick_roll_up_then_right() {
        let mut pads = connected(PAD_A);
        let signals = run(
            &mut pads,
            &[
                stick(PAD_A, StickAxis::Y, 0.9),
                // Rolling towards the up-right diagonal.
                stick(PAD_A, StickAxis::X, 0.3),
                stick(PAD_A, StickAxis::Y, 0.8),
                stick(PAD_A, StickAxis::X, 0.6),
                stick(PAD_A, StickAxis::Y, 0.7),
            ],
        );
        assert_eq!(
            signals,
            [
                stick_signal(Direction::Up, true),
                stick_signal(Direction::Right, true)
            ]
        );
        // Through the input state: up first, then up and right held, right
        // pressed last; the core's resolver turns that into right from the
        // frame the right deflection crossed (input_resolution_last_pressed
        // proves the resolver side).
        let mut state = InputState::new(Bindings::from_settings(&Settings::default()));
        let mut pads = connected(PAD_A);
        state.pad_event(&mut pads, stick(PAD_A, StickAxis::Y, 0.9));
        let frame = state.take_frame();
        assert!(frame.slots[0].up && !frame.slots[0].right);
        state.pad_event(&mut pads, stick(PAD_A, StickAxis::X, 0.6));
        let frame = state.take_frame();
        assert!(frame.slots[0].up && frame.slots[0].right);
        assert!(!frame.slots[0].left && !frame.slots[0].down);
    }

    #[test]
    fn pad_slots_assign_and_free() {
        let mut pads = Pads::new(0.5);
        assert_eq!(
            run(
                &mut pads,
                &[
                    PadEvent::Connected(PAD_A),
                    PadEvent::Connected(PAD_B),
                    PadEvent::Connected(PAD_C),
                    // A second report of a known pad changes nothing.
                    PadEvent::Connected(PAD_A),
                ]
            ),
            [PadSignal::Connected(0), PadSignal::Connected(1)]
        );
        assert_eq!(pads.slot_of(PAD_A), Some(0));
        assert_eq!(pads.slot_of(PAD_B), Some(1));
        assert_eq!(pads.slot_of(PAD_C), None, "a third pad is ignored");
        // The ignored pad's inputs do nothing.
        assert_eq!(
            run(
                &mut pads,
                &[PadEvent::Button {
                    pad: PAD_C,
                    button: PadButton::South,
                    pressed: true
                }]
            ),
            []
        );
        assert_eq!(
            run(
                &mut pads,
                &[PadEvent::Button {
                    pad: PAD_B,
                    button: PadButton::South,
                    pressed: true
                }]
            ),
            [PadSignal::Input {
                slot: 1,
                input: PadInput::Button(PadButton::South),
                pressed: true
            }]
        );
        // Player 1's pad goes; the next new pad takes the lowest free slot.
        assert_eq!(
            run(&mut pads, &[PadEvent::Disconnected(PAD_A)]),
            [PadSignal::Removed(0)]
        );
        assert_eq!(pads.slot_of(PAD_A), None);
        assert_eq!(run(&mut pads, &[PadEvent::Disconnected(PAD_C)]), []);
        assert_eq!(
            run(&mut pads, &[PadEvent::Connected(PAD_C)]),
            [PadSignal::Connected(0)]
        );
        assert_eq!(pads.slot_of(PAD_C), Some(0));
        assert_eq!(pads.slot_of(PAD_B), Some(1));
        // A reconnected pad starts at rest.
        assert_eq!(
            run(&mut pads, &[stick(PAD_C, StickAxis::X, 0.45)]),
            [],
            "below the dead zone for a fresh pad"
        );
    }

    #[test]
    fn pad_removed_releases_controls() {
        let mut state = InputState::new(Bindings::from_settings(&Settings::default()));
        let mut pads = Pads::new(0.5);
        state.pad_event(&mut pads, PadEvent::Connected(PAD_A));
        let frame = state.take_frame();
        assert_eq!(frame.pad_changes, [PadChange::Connected(0)]);
        state.pad_event(
            &mut pads,
            PadEvent::DPad {
                pad: PAD_A,
                direction: Direction::Right,
                pressed: true,
            },
        );
        state.pad_event(
            &mut pads,
            PadEvent::Button {
                pad: PAD_A,
                button: PadButton::South,
                pressed: true,
            },
        );
        let frame = state.take_frame();
        assert!(frame.slots[0].right && frame.slots[0].fire);
        // Unplugged with right and fire held.
        state.pad_event(&mut pads, PadEvent::Disconnected(PAD_A));
        let frame = state.take_frame();
        assert_eq!(frame.pad_changes, [PadChange::Removed(0)]);
        assert!(!frame.slots[0].right && !frame.slots[0].fire);
        // Nothing comes back by itself, and a late release is harmless.
        state.pad_event(
            &mut pads,
            PadEvent::DPad {
                pad: PAD_A,
                direction: Direction::Right,
                pressed: false,
            },
        );
        let frame = state.take_frame();
        assert_eq!(frame.slots, [digger_core::Controls::NONE; 2]);
        assert_eq!(frame.pad_changes, []);
    }
}
