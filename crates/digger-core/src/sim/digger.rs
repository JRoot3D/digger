//! The digger: movement, digging, emeralds, death, respawn and lives
//! (design D1).
//!
//! The game keeps two digger records, indexed by [`DiggerId`]: in a
//! two-digger game one per digger, in an alternating game one per player
//! (the record of the player whose turn it is plays), otherwise only the
//! first is in play (research 01 §2.1, §3.1).
//!
//! [`DiggerId`]: super::DiggerId

/// Lives every digger in play starts a game with (game-flow spec, "Game
/// start").
pub(crate) const START_LIVES: u32 = 3;

/// One digger record.
///
/// Task group 8 adds the position, movement and the other hash-visible
/// fields of design D5; this shell holds the lives the round flow needs.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Digger {
    /// Lives left, the one being played included.
    pub(crate) lives: u32,
}
