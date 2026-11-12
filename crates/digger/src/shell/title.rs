//! The title scene: the title picture, the animation script and the mode
//! cycle (design D2).
//!
//! Task group 7 needs only the scene the program starts on and returns to:
//! a screen of its own (not a game's framebuffer) and the close request.
//! The picture, the animation, mode selection and starting a game follow
//! in task group 8.

use digger_core::video::Framebuffer;

use super::{Context, ShellInput, Transition};

/// The title screen (design D2).
#[derive(Debug, Clone)]
pub(crate) struct TitleScene {
    /// The shell-owned screen the title is drawn on.
    screen: Framebuffer,
}

impl TitleScene {
    /// The title screen as it opens.
    pub(crate) fn new() -> Self {
        Self {
            screen: Framebuffer::new(),
        }
    }

    /// Runs one frame of the title screen.
    #[expect(
        clippy::unused_self,
        clippy::needless_pass_by_ref_mut,
        reason = "the animation and the title actions of task group 8 use the scene and the context"
    )]
    pub(crate) fn frame(&mut self, input: &ShellInput, _cx: &mut Context<'_>) -> Transition {
        if input.close_requested {
            return Transition::Exit(0);
        }
        Transition::Stay
    }

    /// The picture to present.
    pub(crate) const fn picture(&self) -> &Framebuffer {
        &self.screen
    }
}
