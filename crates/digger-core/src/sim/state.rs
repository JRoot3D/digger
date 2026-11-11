//! Read-only views of the simulation state for hashing, tests and the
//! tick trace (design D1, D5).

use core::marker::PhantomData;

/// A read-only view of the game state at one moment, handed to the
/// [`Observer`](super::Observer) hooks and returned by the game.
///
/// Its getters expose every variable of the upstream state hash
/// (research 01 §8) with typed values; converting them to the original's
/// numeric encodings is the recording change's job (design D5).
#[derive(Debug, Clone, Copy)]
pub struct StateView<'a> {
    /// The borrowed game state.
    // Task group 7 replaces this marker with the borrowed simulation state.
    game: PhantomData<&'a ()>,
}

impl StateView<'_> {
    /// A view of nothing yet; the game builds real views from task group 7.
    #[allow(dead_code, reason = "replaced by the game's view in task group 7")]
    pub(crate) const fn empty() -> Self {
        Self { game: PhantomData }
    }
}
