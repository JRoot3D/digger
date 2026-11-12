//! gilrs polling into abstract pad events (design D7).
//!
//! The platform drains gilrs once per wake-up and turns each event into an
//! [`input::PadEvent`](crate::input::PadEvent); slot assignment, the stick's
//! dead zone and hysteresis, and what a pad's buttons mean are decided by the
//! input layer (task 5.6). gilrs's default filters are kept: they turn a
//! d-pad reported as axes into d-pad buttons and drop axis jitter.
//!
//! Pads connected before the program started are reported as connected
//! when polling starts, in gilrs's order, so the first of them plays for
//! player 1 (`input-controls` spec, gamepad assignment and hot-plug).

use digger_core::Direction;
use gilrs::{Axis, Button, EventType, Gilrs};

use crate::input::{PadButton, PadEvent, PadId, StickAxis};

/// The gamepads, while polling works.
#[derive(Debug)]
pub(super) struct GamepadPoller {
    gilrs: Gilrs,
    /// Connections found at start, reported by the first poll.
    initial: Vec<PadEvent>,
}

impl GamepadPoller {
    /// Starts polling; `Err` is the reason, for a notice, when gamepads
    /// cannot be read on this system.
    pub(super) fn new() -> Result<Self, String> {
        let gilrs = Gilrs::new().map_err(|error| error.to_string())?;
        let initial = gilrs
            .gamepads()
            .filter(|(_, pad)| pad.is_connected())
            .map(|(id, _)| PadEvent::Connected(PadId(id.into())))
            .collect();
        Ok(Self { gilrs, initial })
    }

    /// Appends the pad events that happened since the last poll, in order.
    pub(super) fn poll(&mut self, out: &mut Vec<PadEvent>) {
        out.append(&mut self.initial);
        while let Some(event) = self.gilrs.next_event() {
            if let Some(event) = translate(PadId(event.id.into()), event.event) {
                out.push(event);
            }
        }
    }
}

/// The pad event of one gilrs event, if it means anything to the game.
pub(super) fn translate(pad: PadId, event: EventType) -> Option<PadEvent> {
    match event {
        EventType::Connected => Some(PadEvent::Connected(pad)),
        EventType::Disconnected => Some(PadEvent::Disconnected(pad)),
        EventType::ButtonPressed(button, _) => button_event(pad, button, true),
        EventType::ButtonReleased(button, _) => button_event(pad, button, false),
        EventType::AxisChanged(axis, value, _) => axis_event(pad, axis, value),
        _ => None,
    }
}

/// A left-stick axis moving; other axes mean nothing to the game.
fn axis_event(pad: PadId, axis: Axis, value: f32) -> Option<PadEvent> {
    let axis = match axis {
        Axis::LeftStickX => StickAxis::X,
        Axis::LeftStickY => StickAxis::Y,
        _ => return None,
    };
    Some(PadEvent::Stick {
        pad,
        axis,
        value: f64::from(value),
    })
}

/// A button going down or up: a d-pad button or a bindable one.
fn button_event(pad: PadId, button: Button, pressed: bool) -> Option<PadEvent> {
    let direction = match button {
        Button::DPadUp => Some(Direction::Up),
        Button::DPadDown => Some(Direction::Down),
        Button::DPadLeft => Some(Direction::Left),
        Button::DPadRight => Some(Direction::Right),
        _ => None,
    };
    if let Some(direction) = direction {
        return Some(PadEvent::DPad {
            pad,
            direction,
            pressed,
        });
    }
    pad_button(button).map(|button| PadEvent::Button {
        pad,
        button,
        pressed,
    })
}

/// The bindable button of a gilrs button.
const fn pad_button(button: Button) -> Option<PadButton> {
    Some(match button {
        Button::South => PadButton::South,
        Button::East => PadButton::East,
        Button::North => PadButton::North,
        Button::West => PadButton::West,
        Button::C => PadButton::C,
        Button::Z => PadButton::Z,
        Button::LeftTrigger => PadButton::LeftTrigger,
        Button::LeftTrigger2 => PadButton::LeftTrigger2,
        Button::RightTrigger => PadButton::RightTrigger,
        Button::RightTrigger2 => PadButton::RightTrigger2,
        Button::Select => PadButton::Select,
        Button::Start => PadButton::Start,
        Button::Mode => PadButton::Mode,
        Button::LeftThumb => PadButton::LeftThumb,
        Button::RightThumb => PadButton::RightThumb,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bindable_button_has_a_gilrs_button() {
        let gilrs_buttons = [
            Button::South,
            Button::East,
            Button::North,
            Button::West,
            Button::C,
            Button::Z,
            Button::LeftTrigger,
            Button::LeftTrigger2,
            Button::RightTrigger,
            Button::RightTrigger2,
            Button::Select,
            Button::Start,
            Button::Mode,
            Button::LeftThumb,
            Button::RightThumb,
        ];
        let mapped: Vec<PadButton> = gilrs_buttons
            .iter()
            .filter_map(|&b| pad_button(b))
            .collect();
        assert_eq!(mapped, PadButton::ALL);
        assert_eq!(pad_button(Button::Unknown), None);
    }

    #[test]
    fn gilrs_events_translate() {
        let pad = PadId(3);
        assert_eq!(
            translate(pad, EventType::Connected),
            Some(PadEvent::Connected(pad))
        );
        assert_eq!(
            translate(pad, EventType::Disconnected),
            Some(PadEvent::Disconnected(pad))
        );
        assert_eq!(translate(pad, EventType::Dropped), None);
        assert_eq!(
            button_event(pad, Button::DPadLeft, true),
            Some(PadEvent::DPad {
                pad,
                direction: Direction::Left,
                pressed: true
            })
        );
        assert_eq!(
            button_event(pad, Button::DPadUp, false),
            Some(PadEvent::DPad {
                pad,
                direction: Direction::Up,
                pressed: false
            })
        );
        assert_eq!(
            button_event(pad, Button::Start, false),
            Some(PadEvent::Button {
                pad,
                button: PadButton::Start,
                pressed: false
            })
        );
        assert_eq!(button_event(pad, Button::Unknown, true), None);
        assert_eq!(
            axis_event(pad, Axis::LeftStickY, 0.75),
            Some(PadEvent::Stick {
                pad,
                axis: StickAxis::Y,
                value: 0.75
            })
        );
        assert_eq!(
            axis_event(pad, Axis::LeftStickX, -1.0),
            Some(PadEvent::Stick {
                pad,
                axis: StickAxis::X,
                value: -1.0
            })
        );
        assert_eq!(axis_event(pad, Axis::RightStickX, 1.0), None);
    }
}
