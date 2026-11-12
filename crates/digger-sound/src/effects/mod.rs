//! The sound effects and the order in which they are heard (design D3).
//!
//! Each effect is a small state machine kept in an `Option` slot of
//! [`Effects`]: `None` is "inactive", starting an effect constructs a fresh
//! state and stopping it drops the state. Once per tick every active
//! effect advances and returns a [`Step`]; the offers are folded in the
//! original's updater order, so the last effect in that order that offers
//! a divisor is heard (kept quirk SQ3).

mod bonus;
mod breaking;
mod death;
mod eat;
mod emerald;
mod explode;
mod extra_life;
mod fall;
mod fire;
mod gold;
mod wobble;

pub use emerald::{EMERALD_DIVISORS, emerald_divisor};

use bonus::Bonus;
use breaking::Break;
use death::Death;
use eat::EatMonster;
use emerald::{Click, EmeraldScale};
use explode::Explode;
use extra_life::ExtraLife;
use fall::Fall;
use fire::FireSlot;
use gold::Gold;
use wobble::Wobble;

use crate::command::{FireballId, SoundCommand};
use crate::noise::NoiseRng;
use crate::units::Divisor;

/// What one effect did on one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Step {
    /// The divisor the effect offers on this tick, if any.
    pub offer: Option<Divisor>,
    /// Whether the effect has finished. An effect may offer a divisor on
    /// the tick on which it finishes (kept quirk SQ5).
    pub done: bool,
}

impl Step {
    /// A tick on which the effect offers `divisor` and keeps running.
    pub const fn offer(divisor: Divisor) -> Self {
        Self {
            offer: Some(divisor),
            done: false,
        }
    }

    /// A tick on which the effect offers nothing and keeps running.
    pub const fn quiet() -> Self {
        Self {
            offer: None,
            done: false,
        }
    }
}

/// One effect's state machine.
trait Effect {
    /// Advances the effect by one tick.
    fn tick(&mut self) -> Step;
}

/// Advances the effect in `slot`, if one is active, ends it when it reports
/// done and returns its offer.
fn advance<E: Effect>(slot: &mut Option<E>) -> Option<Divisor> {
    finish(slot, Effect::tick)
}

/// Advances the effect in `slot` with `tick`, ends it when the step reports
/// done and returns its offer.
fn finish<E>(slot: &mut Option<E>, tick: impl FnOnce(&mut E) -> Step) -> Option<Divisor> {
    let step = tick(slot.as_mut()?);
    if step.done {
        *slot = None;
    }
    step.offer
}

/// Folds the offers of one tick: a later offer replaces an earlier one, an
/// absent offer replaces nothing.
fn fold(offers: impl IntoIterator<Item = Option<Divisor>>) -> Option<Divisor> {
    offers
        .into_iter()
        .fold(None, |heard, offer| offer.or(heard))
}

/// The element of a per-fireball pair that belongs to `id`.
fn of_fireball<T>(pair: &mut [T; 2], id: FireballId) -> &mut T {
    let [zero, one] = pair;
    if id.index() == 0 { zero } else { one }
}

/// The original's round-robin choice between the two fireballs.
///
/// Starting at the fireball whose turn it is, the first one that sounds on
/// this tick is heard, and the turn moves past every fireball looked at.
/// With both sounding on the same ticks the one heard alternates; the turn
/// is kept across ticks on which neither sounds, and across a stop-all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Turn {
    /// The fireball looked at first next time, 0 or 1.
    next: u8,
}

impl Turn {
    /// The value of the pair that is heard on this tick, chosen by turn.
    fn pick<T: Copy>(&mut self, pair: [Option<T>; 2]) -> Option<T> {
        let [zero, one] = pair;
        if zero.is_none() && one.is_none() {
            return None;
        }
        loop {
            let looked_at = if self.next == 0 { zero } else { one };
            self.next = (self.next + 1) % 2;
            if looked_at.is_some() {
                return looked_at;
            }
        }
    }
}

/// What the effects produced on one tick.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct EffectsTick {
    /// The divisor that is heard, if any effect offers one.
    pub offer: Option<Divisor>,
    /// Whether any tune is to be stopped: true on the death effect's first
    /// tick (design D3). Effects never touch the music themselves.
    pub stop_music: bool,
}

/// All effect slots.
///
/// Every slot starts empty; a start command fills it with a fresh state
/// and a stop command, or the effect's own end, empties it. The wobble and
/// the bonus siren are the exceptions: their start command keeps a running
/// state, because the simulation repeats it for as long as the cue lasts
/// and only the stop command resets it (as in the original, whose start
/// only raises a flag). A fire-start keeps the fireball's cadence (kept
/// quirk SQ4).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    emerald: Option<EmeraldScale>,
    wobble: Option<Wobble>,
    death: Option<Death>,
    breaking: Option<Break>,
    gold: Option<Gold>,
    click: Option<Click>,
    explode: [Option<Explode>; 2],
    fire: [FireSlot; 2],
    eat: Option<EatMonster>,
    fall: Option<Fall>,
    extra_life: Option<ExtraLife>,
    bonus: Option<Bonus>,
    fire_turn: Turn,
    explode_turn: Turn,
    noise: NoiseRng,
}

impl Effects {
    /// All slots empty, the noise generator in its initial state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Applies an effect command, or stop-all; any other command is
    /// ignored here.
    pub fn apply(&mut self, command: SoundCommand) {
        match command {
            SoundCommand::StopAll => self.stop_all(),
            SoundCommand::FallStart => self.fall = Some(Fall::new()),
            SoundCommand::FallStop => self.fall = None,
            SoundCommand::WobbleStart => {
                self.wobble.get_or_insert_with(Wobble::new);
            }
            SoundCommand::WobbleStop => self.wobble = None,
            SoundCommand::Break => self.breaking = Some(Break::new()),
            SoundCommand::Fire(id) => of_fireball(&mut self.fire, id).start(),
            SoundCommand::FireStop(id) => of_fireball(&mut self.fire, id).stop(),
            SoundCommand::Explode(id) => {
                of_fireball(&mut self.fire, id).stop();
                *of_fireball(&mut self.explode, id) = Some(Explode::new());
            }
            SoundCommand::BonusPulseStart => {
                self.bonus.get_or_insert_with(Bonus::new);
            }
            SoundCommand::BonusPulseStop => self.bonus = None,
            SoundCommand::EmeraldClick => self.click = Some(Click),
            SoundCommand::EmeraldNote(step) => self.emerald = Some(EmeraldScale::new(step)),
            SoundCommand::Gold => self.gold = Some(Gold::new()),
            SoundCommand::EatMonster => self.eat = Some(EatMonster::new()),
            SoundCommand::DiggerDeath => self.death = Some(Death::new()),
            SoundCommand::ExtraLife => self.extra_life = Some(ExtraLife::new()),
            _ => {}
        }
    }

    /// Ends the death effect (starting the dirge does this).
    pub fn stop_death(&mut self) {
        self.death = None;
    }

    /// Ends every effect (the stop-all command).
    ///
    /// Only the effect slots are cleared. The noise generator and the two
    /// round-robin turns are kept, as in the original, whose stop-all only
    /// switches each effect off.
    pub fn stop_all(&mut self) {
        *self = Self {
            fire_turn: self.fire_turn,
            explode_turn: self.explode_turn,
            noise: self.noise,
            ..Self::default()
        };
    }

    /// Whether no effect is active.
    pub fn is_idle(&self) -> bool {
        let Self {
            emerald,
            wobble,
            death,
            breaking,
            gold,
            click,
            explode,
            fire,
            eat,
            fall,
            extra_life,
            bonus,
            fire_turn: _,
            explode_turn: _,
            noise: _,
        } = self;
        emerald.is_none()
            && wobble.is_none()
            && death.is_none()
            && breaking.is_none()
            && gold.is_none()
            && click.is_none()
            && explode.iter().all(Option::is_none)
            && fire.iter().all(|slot| !slot.is_active())
            && eat.is_none()
            && fall.is_none()
            && extra_life.is_none()
            && bonus.is_none()
    }

    /// The noise generator's current state.
    pub fn noise(&self) -> NoiseRng {
        self.noise
    }

    /// Advances every active effect by one tick and returns the divisor
    /// that is heard, if any effect offers one.
    ///
    /// The slots are advanced in the original's updater order: emerald
    /// scale, wobble, death, break, gold, emerald click, explosion,
    /// fireball flight, eat monster, fall, extra life, bonus siren. The
    /// last offer wins, which gives the spec's priority from highest to
    /// lowest: bonus siren, extra life, bag fall, eat monster, fireball
    /// flight, explosion, emerald click, gold, bag break, death, wobble,
    /// emerald scale (kept quirk SQ3). An effect that offers nothing on a
    /// tick does not block a lower one, and every active effect advances
    /// whether or not it is heard (research 04 §1.3).
    pub fn tick(&mut self) -> EffectsTick {
        let mut stop_music = false;
        let offer = fold([
            advance(&mut self.emerald),
            advance(&mut self.wobble),
            finish(&mut self.death, |death| {
                let step = death.tick();
                stop_music = step.stop_music;
                step.step
            }),
            advance(&mut self.breaking),
            advance(&mut self.gold),
            advance(&mut self.click),
            self.tick_explosions(),
            self.tick_fire(),
            advance(&mut self.eat),
            advance(&mut self.fall),
            advance(&mut self.extra_life),
            advance(&mut self.bonus),
        ]);
        EffectsTick { offer, stop_music }
    }

    /// Advances both explosions; the one heard is chosen by turn.
    fn tick_explosions(&mut self) -> Option<Divisor> {
        let [zero, one] = &mut self.explode;
        let offers = [advance(zero), advance(one)];
        self.explode_turn.pick(offers)
    }

    /// Advances both flights; the one heard is chosen by turn and sounds
    /// its base plus a noise value from 0 to base / 8 - 1.
    ///
    /// The noise is drawn only on ticks on which some flight sounds.
    fn tick_fire(&mut self) -> Option<Divisor> {
        let [zero, one] = &mut self.fire;
        let bases = [zero.tick(), one.tick()];
        let base = self.fire_turn.pick(bases)?;
        let noise = self.noise.draw(u32::from(base >> 3));
        // The noise is below base / 8 <= 4096, so it fits and the sum stays
        // below 65536.
        let jitter = u16::try_from(noise).unwrap_or(0);
        Some(Divisor(base + jitter))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::EmeraldStep;

    /// An effect that offers on odd ticks and is done after `len` ticks.
    struct Blink {
        divisor: Divisor,
        tick: u32,
        len: u32,
    }

    impl Effect for Blink {
        fn tick(&mut self) -> Step {
            self.tick += 1;
            Step {
                offer: (self.tick % 2 == 1).then_some(self.divisor),
                done: self.tick == self.len,
            }
        }
    }

    #[test]
    fn last_offer_wins_and_absent_offers_do_not_block() {
        let (a, b) = (Some(Divisor(100)), Some(Divisor(200)));
        assert_eq!(fold([]), None);
        assert_eq!(fold([a, b]), b);
        assert_eq!(fold([b, a]), a);
        assert_eq!(fold([a, None]), a);
        assert_eq!(fold([None, b, None]), b);
    }

    #[test]
    fn done_effect_is_dropped_after_its_last_offer() {
        let mut slot = Some(Blink {
            divisor: Divisor(1000),
            tick: 0,
            len: 3,
        });
        assert_eq!(advance(&mut slot), Some(Divisor(1000)));
        assert_eq!(advance(&mut slot), None);
        assert!(slot.is_some());
        // The third tick both offers and finishes.
        assert_eq!(advance(&mut slot), Some(Divisor(1000)));
        assert!(slot.is_none());
        assert_eq!(advance(&mut slot), None);
    }

    #[test]
    fn empty_effects_offer_nothing() {
        let mut effects = Effects::new();
        assert!(effects.is_idle());
        for _ in 0..100 {
            assert_eq!(effects.tick(), EffectsTick::default());
        }
    }

    #[test]
    fn stop_all_empties_every_slot_but_keeps_the_noise() {
        let mut effects = Effects::new();
        effects.apply(SoundCommand::Gold);
        effects.apply(SoundCommand::Fire(FireballId::ALL[0]));
        for _ in 0..4 {
            effects.tick();
        }
        assert!(!effects.is_idle());
        let noise = effects.noise();
        assert_ne!(noise, NoiseRng::new());
        effects.stop_all();
        assert!(effects.is_idle());
        assert_eq!(effects.noise(), noise);
        assert_eq!(effects.tick(), EffectsTick::default());
    }

    #[test]
    fn turn_alternates_and_skips_silent_fireballs() {
        let mut turn = Turn::default();
        assert_eq!(turn.pick([Some(0), Some(1)]), Some(0));
        assert_eq!(turn.pick([Some(0), Some(1)]), Some(1));
        assert_eq!(turn.pick([Some(0), Some(1)]), Some(0));
        // Only fireball 0 sounds while it is fireball 1's turn: the turn
        // moves past 1 and then past 0.
        assert_eq!(turn.pick([Some(0), None]), Some(0));
        assert_eq!(turn.next, 1);
        assert_eq!(turn.pick::<u8>([None, None]), None);
        assert_eq!(turn.next, 1);
        assert_eq!(turn.pick([None, Some(1)]), Some(1));
        assert_eq!(turn.next, 0);
    }

    #[test]
    fn emerald_step_range() {
        assert_eq!(EmeraldStep::new(8), None);
        let divisors: Vec<u16> = EmeraldStep::ALL
            .iter()
            .map(|&step| emerald_divisor(step).get())
            .collect();
        assert_eq!(divisors, [2280, 2032, 1810, 1708, 1522, 1356, 1208, 1140]);
    }

    #[test]
    fn death_stops_music_on_its_first_tick_only() {
        let mut effects = Effects::new();
        effects.apply(SoundCommand::DiggerDeath);
        assert!(effects.tick().stop_music);
        assert!((0..60).all(|_| !effects.tick().stop_music));
    }
}
