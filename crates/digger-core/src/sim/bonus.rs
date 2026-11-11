//! The bonus (cherry): appearance, bonus-mode timers and signalling
//! (design D1; digger spec, "Bonus appearance", "Entering bonus mode",
//! "Bonus mode signalling"; research 02 §3.8).
//!
//! Bonus mode itself is the simulation's `bonus_mode` flag; this module
//! holds the bonus sprite's visibility and the two countdowns, and the
//! context methods that switch them.

use crate::assets::ImageId;
use crate::geom::Px;
use crate::sound_cmd::{SoundCommand, Tempo, Tune};
use crate::sprite::SpriteId;
use crate::video::Intensity;

use super::ids::DiggerId;
use super::level::Difficulty;
use super::score::{ScoreEvent, ScoreId};
use super::{Ctx, Sim};

/// Where the bonus appears: the top-right cell.
pub(crate) const BONUS_AT: Px = Px::new(292, 18);

/// The ticks of flashing and pulsing at the start of bonus mode, and the
/// remaining count below which they resume at its end.
const FLASH_TICKS: i16 = 20;

/// The length of bonus mode at difficulty `d`, in ticks of its countdown:
/// 250 − 20 × difficulty, 230..=50 (digger spec, "Entering bonus mode";
/// research 02 §7). Bonus mode lasts one tick more than this while a
/// digger is alive.
pub(crate) const fn bonus_duration(d: Difficulty) -> i16 {
    // The difficulty is at most 10, so the result is at least 50.
    250 - 20 * d.get() as i16
}

/// The bonus and the bonus-mode countdowns (hashed as `bonusvisible`,
/// `bonustimeleft` and `startbonustimeleft`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) struct Bonus {
    /// Whether the bonus is shown in its cell.
    pub(crate) visible: bool,
    /// Ticks of bonus mode left to count down (`bonustimeleft`).
    pub(crate) time_left: i16,
    /// Ticks of the opening flash left (`startbonustimeleft`). It keeps
    /// counting below 0 during the final flash, as in the original, where
    /// it is harmless.
    pub(crate) start_left: i16,
}

impl Sim {
    /// Clears the bonus and bonus mode as every round start does: hidden,
    /// both countdowns 0, bonus mode off (research 02 §3.2, `initdigger`).
    /// The picture's intensity is left as it is.
    pub(crate) fn reset_bonus(&mut self) {
        self.bonus = Bonus::default();
        self.bonus_mode = false;
    }
}

impl Ctx<'_> {
    /// Shows the bonus at (292, 18) (research 02 §3.8, `createbonus`). The
    /// collisions of its draw are not used.
    pub(crate) fn show_bonus(&mut self) {
        self.sim.bonus.visible = true;
        self.gfx.move_bonus(ImageId::BONUS, BONUS_AT);
    }

    /// Removes the bonus if it is shown, and restores normal intensity
    /// either way (research 02 §3.8, `erasebonus`).
    pub(crate) fn remove_bonus(&mut self) {
        if self.sim.bonus.visible {
            self.sim.bonus.visible = false;
            self.gfx.erase(SpriteId::BONUS);
        }
        self.gfx.fb_mut().set_intensity(Intensity::Normal);
    }

    /// The pickup of the bonus by `unit`'s move: 1000 points to the score
    /// it is credited to, then bonus mode starts (digger spec, "Entering
    /// bonus mode").
    pub(crate) fn collect_bonus(&mut self, unit: DiggerId) {
        let score = ScoreId::credited(self.sim.turn, unit);
        self.add_score(score, ScoreEvent::Bonus);
        self.start_bonus_mode();
    }

    /// Starts bonus mode: the bonus is removed, the picture goes to high
    /// intensity, the countdowns are set to the duration of the current
    /// level and to 20, and the eat multiplier of digger records 0 up to
    /// the number of diggers is reset to 1 (research 02 §3.8,
    /// `initbonusmode`).
    ///
    /// **Quirk Q24:** the records reset are counted from 0 whoever's turn
    /// it is, so in an alternating game the second player's multiplier is
    /// not reset here. It is reset at every round start and respawn
    /// anyway.
    pub(crate) fn start_bonus_mode(&mut self) {
        self.sim.bonus_mode = true;
        self.remove_bonus();
        self.gfx.fb_mut().set_intensity(Intensity::High);
        let difficulty = self.sim.current_level().difficulty();
        self.sim.bonus.time_left = bonus_duration(difficulty);
        self.sim.bonus.start_left = FLASH_TICKS;
        let diggers = usize::from(self.sim.cfg.mode.diggers());
        for digger in self.sim.diggers.iter_mut().take(diggers) {
            digger.eat_multiplier = 1;
        }
    }

    /// Ends bonus mode: the flag is cleared, normal intensity restored, the
    /// pulse cue stopped and the main music requested (research 02 §3.8,
    /// `endbonusmode`).
    fn end_bonus_mode(&mut self) {
        self.sim.bonus_mode = false;
        self.gfx.fb_mut().set_intensity(Intensity::Normal);
        self.sound.push(SoundCommand::BonusPulseStop);
        self.request_main_music();
    }

    /// The bonus-mode timers at the end of the diggers' update (digger
    /// spec, "Entering bonus mode", "Bonus mode signalling"; research 02
    /// §3.8).
    ///
    /// While a digger is alive the countdown runs from the pickup tick
    /// itself. During the first 20 ticks, and whenever fewer than 20 ticks
    /// remain, the intensity follows the parity of the remaining count
    /// (odd normal, even high) and the pulse cue sounds; the tick that ends
    /// the first 20 requests the bonus music, stops the pulse and holds
    /// the picture at high intensity. On the tick after the count has
    /// reached 0, or as soon as no digger is alive, bonus mode ends.
    pub(crate) fn update_bonus_mode(&mut self) {
        if !self.sim.bonus_mode {
            return;
        }
        if !self.sim.any_alive() {
            self.end_bonus_mode();
            return;
        }
        let bonus = &mut self.sim.bonus;
        if bonus.time_left == 0 {
            self.end_bonus_mode();
            return;
        }
        bonus.time_left -= 1;
        if bonus.start_left == 0 && bonus.time_left >= FLASH_TICKS {
            return;
        }
        bonus.start_left = bonus.start_left.wrapping_sub(1);
        let (time_left, start_left) = (bonus.time_left, bonus.start_left);
        let intensity = if time_left & 1 == 1 {
            Intensity::Normal
        } else {
            Intensity::High
        };
        self.gfx.fb_mut().set_intensity(intensity);
        self.sound.push(SoundCommand::BonusPulseStart);
        if start_left == 0 {
            self.request_bonus_music();
            self.sound.push(SoundCommand::BonusPulseStop);
            self.gfx.fb_mut().set_intensity(Intensity::High);
        }
    }

    /// Requests the bonus music at standard tempo; it is not a main tune,
    /// so a change of lives does not re-request anything while it plays.
    pub(crate) fn request_bonus_music(&mut self) {
        self.sim.main_tune = None;
        self.sound.push(SoundCommand::Music {
            tune: Tune::Bonus,
            tempo: Tempo::Standard,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::observer::NoopObserver;
    use crate::sim::testkit::Scenario;
    use crate::sim::{GameMode, LevelNumber, PlayerId};

    /// Starts bonus mode in a `mode` game on `turn`'s turn with both
    /// records' multipliers at 8; returns the multipliers after.
    fn multipliers_after_start(mode: GameMode, turn: PlayerId) -> [i16; 2] {
        let mut run = Scenario::new(mode).build();
        run.start();
        let game = run.game_mut();
        game.sim.turn = turn;
        for digger in &mut game.sim.diggers {
            digger.eat_multiplier = 8;
        }
        let mut obs = NoopObserver;
        let mut cx = Ctx {
            sim: &mut game.sim,
            gfx: &mut game.gfx,
            sound: &mut game.sound,
            obs: &mut obs,
        };
        cx.start_bonus_mode();
        game.sim.diggers.map(|digger| digger.eat_multiplier)
    }

    #[test]
    fn quirk_q24_bonus_mode_resets_multipliers_by_absolute_index() {
        let [first, second] = PlayerId::ALL;
        assert_eq!(multipliers_after_start(GameMode::TwoDiggers, first), [1, 1]);
        assert_eq!(multipliers_after_start(GameMode::TwoPlayers, first), [1, 8]);
        // On the second player's turn it is still record 0 that is reset.
        assert_eq!(
            multipliers_after_start(GameMode::TwoPlayers, second),
            [1, 8]
        );
    }

    #[test]
    fn bonus_duration_table_230_to_50() {
        let ticks: Vec<i16> = (1..=11)
            .map(|level| {
                let level = LevelNumber::new(level).unwrap_or_else(|| unreachable!());
                bonus_duration(level.difficulty())
            })
            .collect();
        assert_eq!(ticks, [230, 210, 190, 170, 150, 130, 110, 90, 70, 50, 50]);
    }
}
