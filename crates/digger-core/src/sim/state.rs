//! Read-only views of the simulation state for hashing, tests and the
//! tick trace (design D1, D5, D11).

use core::fmt;

use super::Sim;
use super::bags::Bag;
use super::config::GameMode;
use super::digger::Digger;
use super::field::{EmeraldMap, Field};
use super::gauntlet::GauntletClock;
use super::gfx::Gfx;
use super::ids::{BagId, DiggerId, MonsterSlot, PlayerId};
use super::level::LevelNumber;
use super::monster::{Monster, Monsters};
use super::penalty::Penalty;
use super::score::{ScoreId, Scores};

/// A read-only view of the game state at one moment, handed to the
/// [`Observer`](super::Observer) hooks and returned by the game.
///
/// Its getters expose the variables of the upstream state hash
/// (research 01 §8) with typed values; converting them to the original's
/// numeric encodings is the recording change's job (design D5). Each task
/// group adds the getters of the state it introduces.
#[derive(Debug, Clone, Copy)]
pub struct StateView<'a> {
    /// The simulation state.
    sim: &'a Sim,
    /// The picture and the sprite layer.
    gfx: &'a Gfx,
}

impl<'a> StateView<'a> {
    /// A view of `sim` drawn into `gfx`.
    pub(crate) const fn new(sim: &'a Sim, gfx: &'a Gfx) -> Self {
        Self { sim, gfx }
    }

    /// The mode of play.
    pub fn mode(self) -> GameMode {
        self.sim.cfg.mode
    }

    /// Ticks run since the game started (hashed as `ticks`).
    pub fn ticks(self) -> u32 {
        self.sim.ticks
    }

    /// The index of the round in progress, counted from 0 across the whole
    /// game; between two rounds, the index of the next one.
    pub fn round(self) -> u32 {
        self.sim.round
    }

    /// The player whose turn it is. The current player's emerald bit is the
    /// hash's `emmask`.
    pub fn current_player(self) -> PlayerId {
        self.sim.turn
    }

    /// `player`'s level, or `None` for the level 0 placeholder of the
    /// second player outside alternating games (quirk Q06).
    pub fn level(self, player: PlayerId) -> Option<LevelNumber> {
        player.of(&self.sim.players).level
    }

    /// Whether `player`'s level has been marked complete.
    pub fn level_done(self, player: PlayerId) -> bool {
        player.of(&self.sim.players).level_done
    }

    /// The random-number generator's state (hashed as `randv`).
    pub fn rng_state(self) -> i32 {
        self.sim.rng.state()
    }

    /// Random draws taken since the game started (design D7).
    #[cfg(any(test, feature = "testkit"))]
    pub fn rng_draws(self) -> u64 {
        self.sim.rng.draws()
    }

    /// The penalty counter of the current (or last) tick.
    pub fn penalty(self) -> Penalty {
        self.sim.penalty
    }

    /// The field in play (hashed as `field`).
    pub fn field(self) -> &'a Field {
        &self.sim.field
    }

    /// `player`'s saved field: the field that is put in play when that
    /// player's level is next drawn in full.
    pub fn saved_field(self, player: PlayerId) -> &'a Field {
        self.sim.saved_fields.saved(player)
    }

    /// Both players' emeralds (hashed as `emfield`).
    pub fn emeralds(self) -> &'a EmeraldMap {
        &self.sim.emeralds
    }

    /// The lives of digger record `digger` (hashed as the digger's
    /// `lives`). In an alternating game record `n` is player `n`'s.
    pub fn lives(self, digger: DiggerId) -> u32 {
        digger.of(&self.sim.diggers).lives
    }

    /// Digger record `digger`: its position, movement, emerald run, death
    /// and the other hashed digger variables. In an alternating game
    /// record `n` is player `n`'s.
    pub fn digger(self, digger: DiggerId) -> &'a Digger {
        digger.of(&self.sim.diggers)
    }

    /// Whether the diggers are shown and updated (hashed as `digvisible`).
    pub fn diggers_visible(self) -> bool {
        self.sim.diggers_visible
    }

    /// Both scores (their totals are hashed as `gettscore`).
    pub fn scores(self) -> &'a Scores {
        &self.sim.scores
    }

    /// The gauntlet clock in 1,193,181ths of a second (hashed as `cgtime`),
    /// or `None` outside the gauntlet modes.
    pub fn gauntlet_clock(self) -> Option<u32> {
        self.sim.gauntlet.map(GauntletClock::units)
    }

    /// Whether the gauntlet clock has run out (hashed as `timeout` in the
    /// gauntlet modes).
    pub fn timed_out(self) -> bool {
        self.sim.gauntlet.is_some_and(GauntletClock::timed_out)
    }

    /// The monster in `slot`, with its last values when the slot is free.
    pub fn monster(self, slot: MonsterSlot) -> &'a Monster {
        slot.of(&self.sim.monsters.slots)
    }

    /// The monster slots with the round's spawn schedule.
    pub fn monsters(self) -> &'a Monsters {
        &self.sim.monsters
    }

    /// The monsters remaining in the round: occupied slots plus those not
    /// yet spawned.
    pub fn monsters_left(self) -> u16 {
        self.sim.monsters.left()
    }

    /// Whether bonus mode is on (hashed as `bonusmode`).
    pub fn bonus_mode(self) -> bool {
        self.sim.bonus_mode
    }

    /// Bag `id`, with its last values when it no longer exists.
    pub fn bag(self, id: BagId) -> &'a Bag {
        id.of(&self.sim.bags)
    }

    /// Bag `id` as saved for `player` at the end of that player's last
    /// round, or as created for the player's level.
    pub fn saved_bag(self, player: PlayerId, id: BagId) -> &'a Bag {
        id.of(player.of(&self.sim.saved_bags))
    }

    /// The gold lifetime (hashed as `goldtime`).
    pub fn gold_time(self) -> u16 {
        self.sim.gold_time
    }

    /// The push counter shared by all bags (hashed as `pushcount`).
    pub fn push_count(self) -> u8 {
        self.sim.push_count
    }

    /// The number of bags still moving for the round's wind-down: those
    /// wobbling (the fall after a wobble included) and gold younger than 10
    /// ticks.
    pub fn moving_bags(self) -> usize {
        self.sim.moving_bags()
    }

    /// The picture, as it stands.
    pub fn framebuffer(self) -> &'a crate::video::Framebuffer {
        self.gfx.fb()
    }

    /// The sprite layer: where every sprite is, what it shows and whether
    /// it is enabled.
    pub fn sprites(self) -> &'a crate::sprite::SpriteLayer {
        self.gfx.sprites()
    }

    /// Writes the one-line tick trace of this state into `out`, without a
    /// line break (design D11).
    ///
    /// The format is fixed: space-separated `key=value` fields in this
    /// order — `t` ticks, `r` round, `p` current player (0 or 1), `rng` the
    /// generator state as eight hex digits, `pen` the penalty counter,
    /// `lv` both players' levels (0 for the Q06 placeholder), `li` both
    /// digger records' lives, `sc` both total scores, `em` the current
    /// player's emeralds left, `dg` both digger records' positions as
    /// `x,y,x,y`, `fb` both digger records' fireball positions as
    /// `x,y,x,y`, and `gt` the gauntlet clock in the gauntlet modes. Later
    /// task groups append the positions of the entities they add.
    ///
    /// # Errors
    ///
    /// Whatever `out` returns.
    pub fn trace_line(self, out: &mut impl fmt::Write) -> fmt::Result {
        let sim = self.sim;
        let [lv0, lv1] = sim.players.map(|p| p.level.map_or(0, LevelNumber::get));
        let [li0, li1] = sim.diggers.map(|d| d.lives);
        let [sc0, sc1] = ScoreId::ALL.map(|id| sim.scores.total(id));
        let [d0, d1] = sim.diggers.map(|d| d.position);
        let [f0, f1] = sim.diggers.map(|d| d.fireball.position);
        write!(
            out,
            "t={} r={} p={} rng={:08X} pen={} lv={lv0},{lv1} li={li0},{li1} sc={sc0},{sc1} em={} \
             dg={},{},{},{} fb={},{},{},{}",
            sim.ticks,
            sim.round,
            sim.turn.number(),
            sim.rng.state().cast_unsigned(),
            sim.penalty.value(),
            sim.emeralds.count(sim.turn),
            d0.x,
            d0.y,
            d1.x,
            d1.y,
            f0.x,
            f0.y,
            f1.x,
            f1.y,
        )?;
        if let Some(clock) = sim.gauntlet {
            write!(out, " gt={}", clock.units())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FrameInput, Game, GameConfig};
    use super::*;

    fn trace(game: &Game) -> String {
        let mut line = String::new();
        game.state()
            .trace_line(&mut line)
            .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
        line
    }

    #[test]
    fn trace_line_is_stable_for_new_game() {
        let game = Game::new(GameConfig::default(), 0);
        assert_eq!(
            trace(&game),
            "t=0 r=0 p=0 rng=00000000 pen=0 lv=1,0 li=3,0 sc=0,0 em=30 dg=0,0,0,0 fb=0,0,0,0"
        );

        let config = GameConfig {
            mode: GameMode::Gauntlet {
                time: super::super::GauntletSeconds::DEFAULT,
            },
            ..GameConfig::default()
        };
        let game = Game::new(config, 0);
        assert_eq!(
            trace(&game),
            "t=0 r=0 p=0 rng=00000000 pen=0 lv=1,0 li=3,0 sc=0,0 em=30 dg=0,0,0,0 fb=0,0,0,0 gt=143181720"
        );
    }

    #[test]
    fn trace_line_follows_the_first_tick() {
        let mut game = Game::new(GameConfig::default(), 0);
        while game.state().ticks() == 0 {
            game.step(&FrameInput::NONE);
        }
        assert_eq!(
            trace(&game),
            "t=1 r=0 p=0 rng=7B1DCDAF pen=1 lv=1,0 li=3,0 sc=0,0 em=30 dg=152,180,0,0 fb=0,0,0,0"
        );
    }
}
