//! The end-of-game hand-over: `player_out`, initials entry, then back to the
//! core's own notice (design D2, D3; `desktop-app` spec, game-over
//! sequence; `high-scores` spec, eligible games, qualifying score,
//! insertion and safe saving; research 01 §6.3).
//!
//! On the frame whose step reports that a player is out, the play scene
//! asks [`GameOver::begin`] whether an entry is due. The diggers of that
//! player are considered in order, first Digger first: one in a one-player
//! or alternating game (the player who is out), both in a game with two
//! Diggers. Each one's six-digit score is tested against the table of the
//! game's mode when its turn comes, so the second Digger is compared with
//! the table that already holds the first one's entry, as upstream's loop
//! does. For each qualifying digger an [`InitialsScene`] runs; when it is
//! complete the entry is inserted and the score file saved, before the
//! next digger or the game continues. While an entry runs the game is not
//! stepped; afterwards the core's 50-frame `GAME OVER` or `TIME UP` notice
//! follows.
//!
//! Only live games played by a person get here: a playback, taken over or
//! not, reports no player result, and a quit ends the game without one.

use digger_core::{Game, PlayerResult, ScoreId};

use super::Context;
use super::initials::{EntryStep, InitialsScene};
use crate::config::GameMode;
use crate::input::{InputFrame, PadAction};
use crate::scores::{Entry, TableKey};

/// A digger whose score is still to be tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Candidate {
    /// The number shown as `PLAYER n`, 1 or 2.
    label: u8,
    /// The six-digit score shown on screen.
    score: u32,
}

/// The six-digit score shown for `id` (never negative; a negative value
/// would be held as 0).
fn visible(game: &Game, id: ScoreId) -> u32 {
    u32::try_from(game.state().scores().visible(id)).unwrap_or(0)
}

/// What a frame of the hand-over decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GameOverStep {
    /// An entry screen is shown.
    Entering,
    /// A digger's entry was completed on this frame and stored; another
    /// entry may follow (see [`GameOver::is_done`]).
    Entered(Entry),
}

/// The entries due at the end of one player's game.
#[derive(Debug, Clone)]
pub(crate) struct GameOver {
    /// The table of the game's mode.
    key: TableKey,
    /// The diggers still to be tested, in order.
    waiting: Vec<Candidate>,
    /// The digger entering initials and its screen.
    current: Option<(Candidate, Box<InitialsScene>)>,
    /// The scores on the top line of each screen.
    top_line: Vec<u32>,
    /// Whether the screen flashes.
    flashing: bool,
}

impl GameOver {
    /// The hand-over for the player reported `out` of `game`, played in
    /// `mode` and recorded in the table named by `key`; `None` when no
    /// digger qualifies.
    pub(crate) fn begin(
        game: &Game,
        out: PlayerResult,
        mode: GameMode,
        key: TableKey,
        cx: &Context<'_>,
    ) -> Option<Self> {
        let waiting: Vec<Candidate> = if mode.two_diggers() {
            ScoreId::ALL
                .into_iter()
                .zip(1..)
                .map(|(id, label)| Candidate {
                    label,
                    score: visible(game, id),
                })
                .collect()
        } else {
            let number = out.player.number();
            let id = ScoreId::new(number).unwrap_or(ScoreId::ALL[0]);
            vec![Candidate {
                label: number + 1,
                score: visible(game, id),
            }]
        };
        // The top line shows both scores whenever the game has two
        // (alternating or simultaneous), as the core's status line does.
        let shown = if mode.two_diggers() || mode == GameMode::TwoPlayers {
            2
        } else {
            1
        };
        let top_line = ScoreId::ALL
            .into_iter()
            .take(shown)
            .map(|id| visible(game, id))
            .collect();
        let mut over = Self {
            key,
            waiting,
            current: None,
            top_line,
            flashing: !cx.settings.reduce_flashing,
        };
        over.next(cx);
        over.current.is_some().then_some(over)
    }

    /// Starts the entry of the next qualifying digger, if any.
    fn next(&mut self, cx: &Context<'_>) {
        self.current = None;
        while !self.waiting.is_empty() {
            let candidate = self.waiting.remove(0);
            // Compat: strictly above tenth place, tested against the
            // table as it is now (research 01 §6.3).
            if cx.scores.scores().qualifies(&self.key, candidate.score) {
                let screen = InitialsScene::new(candidate.label, &self.top_line, self.flashing);
                self.current = Some((candidate, Box::new(screen)));
                return;
            }
        }
    }

    /// Runs one frame of entry. A completed entry is inserted into its
    /// table and the score file saved at once; a save that fails is
    /// reported as a notice and the entry stays in memory.
    pub(crate) fn frame(&mut self, input: &InputFrame, cx: &mut Context<'_>) -> GameOverStep {
        let Some((candidate, screen)) = self.current.as_mut() else {
            return GameOverStep::Entering;
        };
        let fire = cx.settings.buttons_of(PadAction::Fire);
        match screen.frame(input, fire) {
            EntryStep::Entering => GameOverStep::Entering,
            EntryStep::Complete(initials) => {
                let entry = Entry::new(initials, candidate.score);
                cx.scores.record_reporting(&self.key, entry);
                self.next(cx);
                GameOverStep::Entered(entry)
            }
        }
    }

    /// Whether every due entry has been made.
    pub(crate) const fn is_done(&self) -> bool {
        self.current.is_none()
    }

    /// The entry screen being shown.
    pub(crate) fn screen(&self) -> Option<&InitialsScene> {
        self.current.as_ref().map(|(_, screen)| screen.as_ref())
    }
}
