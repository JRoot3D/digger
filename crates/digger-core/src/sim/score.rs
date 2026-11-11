//! Scores, the scoring call, bonus thresholds and the one-million rollover
//! (design D1).
//!
//! Each of the two scores keeps a visible score, a carried total and the
//! threshold of its next bonus. Every point the game awards goes through
//! one scoring call, [`Scores::add`], which reduces the award to 16 bits,
//! rolls the score over at one million, redraws it, checks the bonus
//! threshold once and charges the penalty counter three times (scoring
//! spec; research 01 §6.1, §6.2). What a bonus does to lives, the gauntlet
//! clock, sound and the lives display is returned as a [`BonusEffect`] for
//! the caller to apply, so this module needs no access to the diggers.

use crate::geom::Px;
use crate::video::{Framebuffer, TextColor};

use super::config::GameConfig;
use super::ids::{DiggerId, PlayerId, index_id};
use super::penalty::{Penalty, PenaltySite};

index_id!(
    /// One of the two scores, 0 or 1. Score 1 belongs to the second player
    /// of an alternating game or to the second digger of a simultaneous
    /// game (research 01 §6.2).
    ScoreId,
    2: [0, 1]
);

impl ScoreId {
    /// The score credited for something `digger` did while it is `turn`'s
    /// turn: score `turn + digger`. At most one of the two is non-zero in
    /// any game mode, because alternating players and simultaneous diggers
    /// cannot be combined (research 01 §6.1, design D2).
    pub fn credited(turn: PlayerId, digger: DiggerId) -> Self {
        debug_assert!(
            turn.number() == 0 || digger.number() == 0,
            "alternating players and two diggers are never combined"
        );
        Self::new(turn.number().saturating_add(digger.number())).unwrap_or(Self::ALL[1])
    }
}

/// A scored game event and its value (scoring spec, "Score event values";
/// research 01 §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScoreEvent {
    /// A zero-point call: at round start, after a pause and at game end
    /// (scoring spec, "Zero-point scoring calls").
    Refresh,
    /// An emerald eaten by a digger: 25.
    Emerald,
    /// The extra award for the eighth emerald of a run: 250.
    EighthEmerald,
    /// Gold collected: 500.
    Gold,
    /// A monster killed by a fireball: 250.
    FireballKill,
    /// A monster killed by a bag in a one-digger game: 250.
    BagKill,
    /// The bonus (cherry) taken: 1000.
    Bonus,
    /// A monster eaten in bonus mode: 200 × the eater's multiplier.
    EatMonster {
        /// The eater's multiplier before it doubles.
        multiplier: i16,
    },
}

impl ScoreEvent {
    /// The award before its reduction to 16 bits.
    pub const fn points(self) -> i32 {
        match self {
            Self::Refresh => 0,
            Self::Emerald => 25,
            Self::Gold => 500,
            Self::EighthEmerald | Self::FireballKill | Self::BagKill => 250,
            Self::Bonus => 1000,
            Self::EatMonster { multiplier } => multiplier as i32 * 200,
        }
    }
}

/// What a scoring call's bonus asks of the caller (scoring spec, "Life
/// cap", "Bonus time in gauntlet mode").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BonusEffect {
    /// No bonus was given.
    #[default]
    None,
    /// Give the score's digger one life, emit the extra-life cue, redraw the
    /// lives display and re-request the main tune.
    ExtraLife,
    /// Add 15 seconds (17,897,715 clock units, with wrap-around) to the
    /// gauntlet clock and redraw the lives display, which shows the clock.
    GauntletTime,
}

/// Lives below which a bonus life is given, unless lives are unlimited
/// (quirk Q10).
const LIFE_CAP: u32 = 5;
/// A visible score above this rolls over into the carried total (quirk
/// Q11).
const ROLLOVER_ABOVE: i32 = 999_999;
/// The second score's x while it is below [`SECOND_SHIFT_AT`].
const SECOND_X: i32 = 236;
/// The second score's x from [`SECOND_SHIFT_AT`] on.
const SECOND_X_SHIFTED: i32 = 248;
/// The second score is drawn from [`SECOND_X_SHIFTED`] from this value on.
const SECOND_SHIFT_AT: i32 = 100_000;
/// Digit positions of a score on the status line.
const SCORE_DIGITS: usize = 6;

/// Reduces an award to the signed 16-bit value that is added.
///
/// **Quirk Q12:** the original's scoring call takes a 16-bit signed award,
/// so an eat at multiplier 256 (51,200 points) adds −14,336 (research 01
/// §6.2, §9).
pub(crate) const fn award_i16(points: i32) -> i16 {
    // Truncation is the quirk.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "quirk Q12: the award is truncated to 16 bits"
    )]
    let award = points as i16;
    award
}

/// One player's or digger's score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct Score {
    /// The score shown on the status line.
    visible: i32,
    /// What rolled over at one million (quirk Q11).
    carried: i32,
    /// The score at which the next bonus is due (before quirk Q09's extra
    /// point for the second score).
    next_bonus: i32,
}

impl Score {
    const fn zero(step: u16) -> Self {
        Self {
            visible: 0,
            carried: 0,
            next_bonus: step as i32,
        }
    }
}

/// Both scores and the rules of their bonuses (scoring spec; research 01
/// §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Scores {
    by_id: [Score; ScoreId::COUNT],
    /// The bonus step: 20,000, or what custom level data sets.
    step: u16,
    /// Whether the life cap is lifted.
    unlimited_lives: bool,
    /// Whether a bonus adds gauntlet time instead of a life.
    gauntlet: bool,
}

impl Scores {
    /// Zeroed scores under `config`'s bonus rules.
    pub fn new(config: &GameConfig) -> Self {
        let step = config.levels.bonus_step();
        Self {
            by_id: [Score::zero(step); ScoreId::COUNT],
            step,
            unlimited_lives: config.unlimited_lives,
            gauntlet: config.mode.gauntlet().is_some(),
        }
    }

    /// Zeroes both scores and their carried totals and sets both
    /// thresholds to the bonus step, as at the start of every game.
    pub fn start_game(&mut self) {
        self.by_id = [Score::zero(self.step); ScoreId::COUNT];
    }

    /// The score shown on the status line.
    pub fn visible(&self, id: ScoreId) -> i32 {
        id.of(&self.by_id).visible
    }

    /// The total score: carried total plus visible score (scoring spec,
    /// "Reported results").
    pub fn total(&self, id: ScoreId) -> i32 {
        let score = id.of(&self.by_id);
        // Beyond 2^31 points (two thousand rollovers) the original's
        // 32-bit sum wraps; it is kept defined here.
        score.carried.wrapping_add(score.visible)
    }

    /// The carried total of rolled-over scores.
    pub fn carried(&self, id: ScoreId) -> i32 {
        id.of(&self.by_id).carried
    }

    /// The threshold of the next bonus, as stored (without quirk Q09's
    /// extra point).
    pub fn next_bonus(&self, id: ScoreId) -> i32 {
        id.of(&self.by_id).next_bonus
    }

    /// The scoring call for an event.
    #[allow(
        dead_code,
        reason = "called by the scoring systems of later task groups"
    )]
    pub(crate) fn add_event(
        &mut self,
        id: ScoreId,
        event: ScoreEvent,
        lives: u32,
        penalty: &mut Penalty,
        fb: &mut Framebuffer,
    ) -> BonusEffect {
        self.add(id, event.points(), lives, penalty, fb)
    }

    /// The scoring call: adds `points` to score `id`, whose digger has
    /// `lives`, redraws the score in text colour 1, checks the bonus
    /// threshold once and charges the penalty counter three times
    /// (research 01 §6.2).
    #[allow(
        dead_code,
        reason = "called by the scoring systems of later task groups"
    )]
    pub(crate) fn add(
        &mut self,
        id: ScoreId,
        points: i32,
        lives: u32,
        penalty: &mut Penalty,
        fb: &mut Framebuffer,
    ) -> BonusEffect {
        let (unlimited, gauntlet, step) = (self.unlimited_lives, self.gauntlet, self.step);
        let score = id.of_mut(&mut self.by_id);
        // Quirk Q12: the award is a signed 16-bit value.
        score.visible += i32::from(award_i16(points));
        // Quirk Q11: past one million the whole score moves to the carried
        // total; the threshold stays where it is, so no bonus follows.
        if score.visible > ROLLOVER_ABOVE {
            score.carried = score.carried.wrapping_add(score.visible);
            score.visible = 0;
        }
        draw_score(fb, id, score.visible, TextColor::One);

        // Quirk Q09: the second score needs one point more than the
        // threshold. Quirk Q10: one step per call, `if` and not `while`;
        // the threshold advances whether or not the cap allowed a bonus.
        let mut effect = BonusEffect::None;
        if score.visible >= score.next_bonus + i32::from(id.number()) {
            if lives < LIFE_CAP || unlimited {
                effect = if gauntlet {
                    BonusEffect::GauntletTime
                } else {
                    BonusEffect::ExtraLife
                };
            }
            score.next_bonus += i32::from(step);
        }

        // Quirk Q01: three charges per call, even of 0 points.
        for _ in 0..3 {
            penalty.charge(PenaltySite::ScoreCall);
        }
        effect
    }
}

/// The two scoring calls for a monster killed by a bag in a two-digger
/// game, in the order they are made, given each digger's lives before the
/// kill.
///
/// Both diggers with lives get 125 each. If digger 0 has no lives, it gets
/// 0 and digger 1 gets 250 if it has lives, otherwise 0; otherwise, if
/// digger 1 has no lives, digger 0 gets 250 and digger 1 gets 0. Both calls
/// are always made, including calls of 0 points, so they redraw, check the
/// thresholds and charge the penalty counter (scoring spec, "Bag kills with
/// two diggers"; research 01 §6.1). The caller makes the calls with
/// [`Scores::add`] in order and applies each call's [`BonusEffect`] before
/// the next, as the original did.
#[allow(dead_code, reason = "called by the monster squash (task 12.8)")]
pub(crate) const fn bag_kill_calls(lives: [u32; 2]) -> [(ScoreId, i32); 2] {
    let [first, second] = ScoreId::ALL;
    let shares = match lives {
        [0, 0] => [0, 0],
        [0, _] => [0, 250],
        [_, 0] => [250, 0],
        _ => [125, 125],
    };
    [(first, shares[0]), (second, shares[1])]
}

/// Where score `id` is drawn while its visible value is `visible`: the
/// first score from x = 0, the second from x = 236 below 100,000 and from
/// x = 248 otherwise (research 01 §6.2, research 03 §8.5).
fn score_x(id: ScoreId, visible: i32) -> i32 {
    match id.number() {
        0 => 0,
        _ if visible < SECOND_SHIFT_AT => SECOND_X,
        _ => SECOND_X_SHIFTED,
    }
}

/// Draws score `id` with visible value `visible` on the status line in
/// `color`: six digit positions of 12 pixels, the five rightmost always
/// drawn with leading zeros, the leftmost only when non-zero and otherwise
/// left as it was (scoring spec, "Score display"; quirk R6).
#[allow(
    dead_code,
    reason = "also called by the full score redraw (task group 7)"
)]
pub(crate) fn draw_score(fb: &mut Framebuffer, id: ScoreId, visible: i32, color: TextColor) {
    // A visible score is never negative (the largest negative award is
    // always preceded by larger positive ones); the magnitude keeps the
    // drawing defined regardless.
    fb.draw_number(
        visible.unsigned_abs(),
        SCORE_DIGITS,
        Px::new(score_x(id, visible), 0),
        color,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::config::{GameMode, GauntletSeconds};

    fn id(n: u8) -> ScoreId {
        ScoreId::new(n).unwrap_or_else(|| unreachable!())
    }

    /// Scores, a penalty counter and a picture for scoring calls.
    struct Bench {
        scores: Scores,
        penalty: Penalty,
        fb: Framebuffer,
    }

    impl Bench {
        fn with(config: &GameConfig) -> Self {
            Self {
                scores: Scores::new(config),
                penalty: Penalty::default(),
                fb: Framebuffer::new(),
            }
        }

        fn new() -> Self {
            Self::with(&GameConfig::default())
        }

        fn add(&mut self, n: u8, points: i32, lives: u32) -> BonusEffect {
            self.scores
                .add(id(n), points, lives, &mut self.penalty, &mut self.fb)
        }

        fn event(&mut self, n: u8, event: ScoreEvent) -> BonusEffect {
            self.scores
                .add_event(id(n), event, 3, &mut self.penalty, &mut self.fb)
        }

        /// Sets score `n`'s visible value directly.
        fn set(&mut self, n: u8, visible: i32) {
            id(n).of_mut(&mut self.scores.by_id).visible = visible;
        }
    }

    #[test]
    fn event_values_accumulate_25_525_775_1775() {
        let mut bench = Bench::new();
        let mut seen = Vec::new();
        for event in [
            ScoreEvent::Emerald,
            ScoreEvent::Gold,
            ScoreEvent::FireballKill,
            ScoreEvent::Bonus,
        ] {
            assert_eq!(bench.event(0, event), BonusEffect::None);
            seen.push(bench.scores.visible(id(0)));
        }
        assert_eq!(seen, [25, 525, 775, 1775]);
        assert_eq!(
            bench.scores.visible(id(1)),
            0,
            "the other score is unchanged"
        );
        // The remaining values of the event table.
        assert_eq!(ScoreEvent::Refresh.points(), 0);
        assert_eq!(ScoreEvent::EighthEmerald.points(), 250);
        assert_eq!(ScoreEvent::BagKill.points(), 250);
        let eats: Vec<i32> = [1, 2, 4, 8, 16, 32]
            .into_iter()
            .map(|multiplier| ScoreEvent::EatMonster { multiplier }.points())
            .collect();
        assert_eq!(eats, [200, 400, 800, 1600, 3200, 6400]);
    }

    #[test]
    fn scores_start_at_zero_and_reset_at_game_start() {
        let mut bench = Bench::new();
        for n in 0..2 {
            assert_eq!(bench.scores.visible(id(n)), 0);
            assert_eq!(bench.scores.total(id(n)), 0);
            assert_eq!(bench.scores.next_bonus(id(n)), 20_000);
        }
        bench.add(0, 30_000, 3);
        bench.set(1, 999_990);
        bench.add(1, 25, 3);
        bench.scores.start_game();
        assert_eq!(bench.scores, Bench::new().scores);
    }

    #[test]
    fn quirk_q12_award_51200_adds_minus_14336() {
        assert_eq!(award_i16(51_200), -14_336);
        assert_eq!(award_i16(25_600), 25_600);
        assert_eq!(award_i16(-1), -1);
        let mut bench = Bench::new();
        bench.set(0, 60_000);
        let multiplier = 256;
        bench.event(0, ScoreEvent::EatMonster { multiplier });
        assert_eq!(bench.scores.visible(id(0)), 60_000 - 14_336);
    }

    #[test]
    fn score_call_charges_three_even_for_zero() {
        let mut bench = Bench::new();
        bench.event(0, ScoreEvent::Refresh);
        assert_eq!(bench.penalty.value(), 3);
        assert_eq!(bench.penalty.count_of(PenaltySite::ScoreCall), 3);
        bench.event(1, ScoreEvent::Emerald);
        assert_eq!(bench.penalty.value(), 6);
        assert_eq!(bench.penalty.count_of(PenaltySite::ScoreCall), 6);
    }

    #[test]
    fn first_bonus_life_at_20000() {
        let mut bench = Bench::new();
        bench.set(0, 19_990);
        assert_eq!(bench.add(0, 25, 3), BonusEffect::ExtraLife);
        assert_eq!(bench.scores.visible(id(0)), 20_015);
        assert_eq!(bench.scores.next_bonus(id(0)), 40_000);
        // Exactly on the threshold counts for the first score.
        let mut bench = Bench::new();
        bench.set(0, 19_975);
        assert_eq!(bench.add(0, 25, 3), BonusEffect::ExtraLife);
        // Just below it does not.
        let mut bench = Bench::new();
        bench.set(0, 19_974);
        assert_eq!(bench.add(0, 25, 3), BonusEffect::None);
        assert_eq!(bench.scores.next_bonus(id(0)), 20_000);
    }

    #[test]
    fn custom_bonus_step_sets_thresholds() {
        let mut config = GameConfig::default();
        config.levels = crate::sim::LevelSet::from_plans(*config.levels.layouts(), 5_000, true);
        let mut bench = Bench::with(&config);
        assert_eq!(bench.scores.next_bonus(id(0)), 5_000);
        assert_eq!(bench.add(0, 5_000, 3), BonusEffect::ExtraLife);
        assert_eq!(bench.scores.next_bonus(id(0)), 10_000);
    }

    #[test]
    fn one_threshold_step_per_call() {
        let mut bench = Bench::new();
        bench.set(0, 19_000);
        assert_eq!(bench.add(0, 22_000, 3), BonusEffect::ExtraLife);
        assert_eq!(bench.scores.visible(id(0)), 41_000);
        assert_eq!(bench.scores.next_bonus(id(0)), 40_000);
        // The second bonus comes with the next call, even of 0 points.
        assert_eq!(bench.event(0, ScoreEvent::Refresh), BonusEffect::ExtraLife);
        assert_eq!(bench.scores.next_bonus(id(0)), 60_000);
        assert_eq!(bench.event(0, ScoreEvent::Refresh), BonusEffect::None);
    }

    #[test]
    fn quirk_q09_second_score_needs_20001() {
        let mut bench = Bench::new();
        bench.set(1, 19_975);
        assert_eq!(bench.add(1, 25, 3), BonusEffect::None);
        assert_eq!(bench.scores.visible(id(1)), 20_000);
        assert_eq!(bench.scores.next_bonus(id(1)), 20_000);
        // Still nothing at 20000 on a later call.
        assert_eq!(bench.add(1, 0, 3), BonusEffect::None);
        // At 20001 or more the next call gives it.
        assert_eq!(bench.add(1, 25, 3), BonusEffect::ExtraLife);
        assert_eq!(bench.scores.next_bonus(id(1)), 40_000);
        // The first score is not late.
        let mut bench = Bench::new();
        bench.set(0, 19_975);
        assert_eq!(bench.add(0, 25, 3), BonusEffect::ExtraLife);
    }

    #[test]
    fn quirk_q10_cap_at_5_threshold_advances() {
        let mut bench = Bench::new();
        bench.set(0, 19_990);
        assert_eq!(bench.add(0, 25, 5), BonusEffect::None);
        assert_eq!(bench.scores.next_bonus(id(0)), 40_000);
        // Four lives still get one.
        bench.set(0, 39_990);
        assert_eq!(bench.add(0, 25, 4), BonusEffect::ExtraLife);
        assert_eq!(bench.scores.next_bonus(id(0)), 60_000);
    }

    #[test]
    fn unlimited_lives_passes_cap() {
        let config = GameConfig {
            unlimited_lives: true,
            ..GameConfig::default()
        };
        let mut bench = Bench::with(&config);
        bench.set(0, 19_990);
        assert_eq!(bench.add(0, 25, 5), BonusEffect::ExtraLife);
        bench.set(0, 39_990);
        assert_eq!(bench.add(0, 25, 99), BonusEffect::ExtraLife);
    }

    #[test]
    fn gauntlet_bonus_adds_time_under_the_same_cap() {
        let config = GameConfig {
            mode: GameMode::Gauntlet {
                time: GauntletSeconds::default(),
            },
            ..GameConfig::default()
        };
        let mut bench = Bench::with(&config);
        bench.set(0, 19_990);
        assert_eq!(bench.add(0, 25, 3), BonusEffect::GauntletTime);
        bench.set(0, 39_990);
        assert_eq!(bench.add(0, 25, 5), BonusEffect::None);
        assert_eq!(bench.scores.next_bonus(id(0)), 60_000);
    }

    #[test]
    fn quirk_q11_rollover_moves_score_to_carried_total() {
        let mut bench = Bench::new();
        bench.set(0, 999_900);
        let threshold = bench.scores.next_bonus(id(0));
        bench.add(0, 250, 3);
        assert_eq!(bench.scores.visible(id(0)), 0);
        assert_eq!(bench.scores.carried(id(0)), 1_000_150);
        assert_eq!(bench.scores.total(id(0)), 1_000_150);
        // The threshold is not reset, so no bonus follows.
        assert_eq!(bench.scores.next_bonus(id(0)), threshold);
        assert_eq!(bench.add(0, 25, 3), BonusEffect::None);
        assert_eq!(bench.scores.visible(id(0)), 25);
        assert_eq!(bench.scores.total(id(0)), 1_000_175);
        // 999,999 itself does not roll over.
        let mut bench = Bench::new();
        bench.set(1, 999_974);
        bench.add(1, 25, 3);
        assert_eq!(bench.scores.visible(id(1)), 999_999);
        assert_eq!(bench.scores.carried(id(1)), 0);
    }

    /// The status-line pixels from logical x = `x`, `w` logical pixels
    /// wide.
    fn status_block(fb: &Framebuffer, x: usize, w: usize) -> Vec<u8> {
        fb.pixels()
            .chunks(Framebuffer::WIDTH)
            .take(24)
            .flat_map(|row| row.get(2 * x..2 * (x + w)).unwrap_or_default().to_vec())
            .collect()
    }

    fn text_at(text: &str, x: i32) -> Framebuffer {
        let mut fb = Framebuffer::new();
        fb.draw_text(text, Px::new(x, 0), TextColor::One);
        fb
    }

    #[test]
    fn score_25_draws_00025_from_x12() {
        let mut bench = Bench::new();
        bench.event(0, ScoreEvent::Emerald);
        assert_eq!(
            bench.fb,
            text_at("00025", 12),
            "x = 12..71, nothing at 0..11"
        );
        assert!(status_block(&bench.fb, 0, 12).iter().all(|&b| b == 0));
        // A zero-point call draws 00000 the same way.
        let mut bench = Bench::new();
        bench.event(0, ScoreEvent::Refresh);
        assert_eq!(bench.fb, text_at("00000", 12));
    }

    #[test]
    fn six_digit_score_from_x0() {
        let mut bench = Bench::new();
        bench.set(0, 123_425);
        bench.event(0, ScoreEvent::Emerald);
        assert_eq!(bench.fb, text_at("123450", 0));
        // The leftmost position is left as it was when its digit is 0 (R6):
        // after the rollover the stale 1 stays visible.
        bench.set(0, 999_900);
        bench.add(0, 250, 3);
        assert_eq!(bench.scores.visible(id(0)), 0);
        assert_eq!(bench.fb, text_at("100000", 0));
        // Colour 3, as the full score redraw uses.
        let mut fb = Framebuffer::new();
        draw_score(&mut fb, id(0), 42, TextColor::Three);
        let mut expected = Framebuffer::new();
        expected.draw_text("00042", Px::new(12, 0), TextColor::Three);
        assert_eq!(fb, expected);
    }

    #[test]
    fn second_score_shifts_at_100000() {
        assert_eq!(score_x(id(1), 99_999), 236);
        assert_eq!(score_x(id(1), 100_000), 248);
        assert_eq!(score_x(id(0), 100_000), 0);

        // Below 100,000: six positions from x = 236, the leftmost (236..247)
        // blank, so the digits occupy 248..307.
        let mut bench = Bench::new();
        bench.set(1, 99_950);
        bench.event(1, ScoreEvent::Emerald);
        assert_eq!(bench.fb, text_at("99975", 248));

        // From 100,000: six positions from x = 248, up to 319.
        let mut bench = Bench::new();
        bench.set(1, 99_975);
        bench.event(1, ScoreEvent::Emerald);
        assert_eq!(bench.scores.visible(id(1)), 100_000);
        assert_eq!(bench.fb, text_at("100000", 248));

        let mut bench = Bench::new();
        bench.set(1, 123_425);
        bench.event(1, ScoreEvent::Emerald);
        assert_eq!(bench.fb, text_at("123450", 248));
        assert!(status_block(&bench.fb, 0, 248).iter().all(|&b| b == 0));
    }

    #[test]
    fn bag_kill_split_both_alive() {
        assert_eq!(bag_kill_calls([3, 3]), [(id(0), 125), (id(1), 125)]);
        assert_eq!(bag_kill_calls([1, 7]), [(id(0), 125), (id(1), 125)]);
        let mut bench = Bench::new();
        for (score, points) in bag_kill_calls([3, 3]) {
            bench.add(score.number(), points, 3);
        }
        assert_eq!(bench.scores.visible(id(0)), 125);
        assert_eq!(bench.scores.visible(id(1)), 125);
    }

    #[test]
    fn bag_kill_all_to_digger_1_when_2_out() {
        // Digger 2 (index 1) out of lives: all to digger 1 (index 0).
        assert_eq!(bag_kill_calls([3, 0]), [(id(0), 250), (id(1), 0)]);
        // Digger 1 out: all to digger 2 while it has lives.
        assert_eq!(bag_kill_calls([0, 2]), [(id(0), 0), (id(1), 250)]);
    }

    #[test]
    fn bag_kill_makes_two_calls_with_zero_points() {
        assert_eq!(bag_kill_calls([0, 0]), [(id(0), 0), (id(1), 0)]);
        let mut bench = Bench::new();
        for (score, points) in bag_kill_calls([3, 0]) {
            bench.add(score.number(), points, 3);
        }
        assert_eq!(bench.penalty.count_of(PenaltySite::ScoreCall), 6);
        assert_eq!(bench.scores.visible(id(0)), 250);
        assert_eq!(bench.scores.visible(id(1)), 0);
        // The 0-point call still redrew the second score.
        let mut expected = text_at("00250", 12);
        expected.draw_text("00000", Px::new(248, 0), TextColor::One);
        assert_eq!(bench.fb, expected);
    }

    #[test]
    fn credited_score_is_turn_plus_digger() {
        let p = |n| PlayerId::new(n).unwrap_or_else(|| unreachable!());
        let d = |n| DiggerId::new(n).unwrap_or_else(|| unreachable!());
        assert_eq!(ScoreId::credited(p(0), d(0)), id(0));
        assert_eq!(ScoreId::credited(p(1), d(0)), id(1));
        assert_eq!(ScoreId::credited(p(0), d(1)), id(1));
    }
}
