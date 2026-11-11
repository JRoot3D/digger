//! The game simulation: one owning game value advanced one frame at a time
//! (design D1–D4).
//!
//! Every system lives in its own module below; the public names are
//! re-exported from the crate root.
//!
//! # No allocation
//!
//! Nothing in the simulation allocates on the heap once the game value
//! exists (design D9). The heap types, methods and macros listed in the
//! workspace's `clippy.toml` are denied here; the only local exceptions are
//! the game's constructor and the test kit. Unit tests are exempt: the
//! deny applies to the library build, which `cargo clippy --all-targets`
//! checks alongside the test build.

#![cfg_attr(
    not(test),
    deny(
        clippy::disallowed_types,
        clippy::disallowed_methods,
        clippy::disallowed_macros
    )
)]

mod bags;
mod bonus;
mod config;
mod digger;
mod draw;
mod field;
mod fireball;
mod gauntlet;
mod geom;
mod gfx;
mod ids;
mod input;
mod level;
mod monster;
mod observer;
mod penalty;
mod rng;
mod score;
mod state;
mod status;
#[cfg(any(test, feature = "testkit"))]
pub mod testkit;
mod timing;

pub use config::{ConfigError, GameConfig, GameMode, GauntletSeconds, StartLevel};
pub use field::{Cell, EmeraldMap, Field};
pub use geom::{CellPos, Direction, Located, locate, reverse_of, within_limits};
pub(crate) use ids::index_id;
pub use ids::{BagId, DiggerId, IdOutOfRange, MonsterSlot, PlayerId, SlotId};
pub use input::{Controls, FrameInput, Movement};
pub use level::{CUSTOM_LEVELS_LEN, Difficulty, LevelError, LevelLayout, LevelNumber, LevelSet};
pub use observer::{NoopObserver, Observer};
pub use penalty::{Penalty, PenaltySite};
pub use rng::{GameRng, Modulus, round_seed};
pub use score::{BonusEffect, ScoreEvent, ScoreId, Scores};
pub use state::StateView;
pub use timing::FrameLength;

use crate::sound_cmd::{SoundCommand, SoundQueue, Tempo, Tune};
use crate::video::{Framebuffer, LevelPlan, TextColor};

pub use bags::Bag;
use digger::{AnimFrame, START_LIVES};
pub use digger::{DeathStage, Digger};
use draw::{DrawProgress, LevelDraw};
use field::PlayerFields;
use fireball::FlameFrame;
pub use fireball::{Fireball, FireballStage};
use gauntlet::GauntletClock;
use gfx::Gfx;
use input::SlotInput;
pub use monster::Monster;
use status::SecondSide;

/// What kind of frame a step was (game-flow spec, "Frames and ticks").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameKind {
    /// The simulation was updated: a main-phase or wind-down tick. The tick
    /// count rose by exactly 1.
    Tick,
    /// A waiting frame: level draw, player announcement, level-complete
    /// pause, a notice, or any frame of a finished game. No tick elapsed.
    Wait,
    /// A frame of the pause. No tick elapsed and nothing changed.
    Paused,
}

/// Why a game finished (game-flow spec, "Finished-game summary").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FinishReason {
    /// Every player has run out of lives.
    AllPlayersOut,
    /// The gauntlet clock ran out.
    TimeUp,
    /// A quit request ended the game.
    Quit,
}

/// One score as reported at the end of a game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScoreSummary {
    /// The score shown on the status line.
    pub visible: i32,
    /// The total score: the visible score plus everything that rolled over
    /// at one million (quirk Q11).
    pub total: i32,
}

/// The result of a finished game (game-flow spec, "Finished-game
/// summary").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GameSummary {
    /// Why the game finished.
    pub reason: FinishReason,
    /// Both scores, indexed by [`ScoreId`]. The second is 0 in modes with
    /// one player and one digger.
    pub scores: [ScoreSummary; ScoreId::COUNT],
    /// The level of the player whose turn it was.
    pub level: LevelNumber,
    /// The number of ticks the game ran.
    pub ticks: u32,
}

/// A player whose game ended on this frame, for high-score entry (game-flow
/// spec, "Game over").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PlayerResult {
    /// The player whose game is over.
    pub player: PlayerId,
    /// The player's final visible score.
    pub score: i32,
}

/// What one step reported (design D2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StepOutcome {
    /// What kind of frame it was.
    pub frame: FrameKind,
    /// Set on the frame a player's game ends.
    pub player_out: Option<PlayerResult>,
    /// Set from the frame the game finishes on, and on every later step.
    pub finished: Option<GameSummary>,
}

impl StepOutcome {
    /// A frame of the given kind that ended nothing.
    const fn of(frame: FrameKind) -> Self {
        Self {
            frame,
            player_out: None,
            finished: None,
        }
    }

    /// A waiting frame of a finished game.
    const fn finished(summary: GameSummary) -> Self {
        Self {
            frame: FrameKind::Wait,
            player_out: None,
            finished: Some(summary),
        }
    }
}

/// Where the game stands between two frames (design D3).
///
/// Every waiting loop of the original is a variant; round start and round
/// end are transitions between variants and take no frame of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// The game is set up but no frame has run. The first step asks the
    /// observer for the participating players' layouts and then runs the
    /// first frame of the level draw.
    Start,
    /// The progressive full draw of the playfield: waiting frames.
    LevelDraw(LevelDraw),
    /// The main phase: one tick per frame.
    Playing,
    /// The game has finished; further steps change nothing.
    Finished(GameSummary),
}

impl Phase {
    /// Whether a round has begun and not yet ended, so that ending the game
    /// now passes through the round end.
    const fn in_round(self) -> bool {
        matches!(self, Self::Playing)
    }
}

/// The per-player state that is not part of the field: the level number
/// and whether the level is marked complete (research 01 §5.2, `gamedat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
struct PlayerState {
    /// The player's level; `None` is the level 0 placeholder of the second
    /// player in a game without alternating players (quirk Q06).
    level: Option<LevelNumber>,
    /// Whether the level has been marked complete.
    level_done: bool,
}

/// The simulation state proper: everything a tick reads or writes apart
/// from the picture, the sprite layer and the sound queue (design D4, D5).
#[derive(Debug, Clone)]
pub(crate) struct Sim {
    /// The configuration the game was started with.
    cfg: GameConfig,
    /// The base seed the round seeds are derived from.
    base_seed: u64,
    /// The random-number generator, reseeded at every round start.
    rng: GameRng,
    /// The penalty counter of the current tick.
    penalty: Penalty,
    /// Ticks run since the game started.
    ticks: u32,
    /// The index of the round in progress, or of the next round between
    /// rounds; it rises at every round end.
    round: u32,
    /// The player whose turn it is.
    turn: PlayerId,
    /// Each player's level.
    players: [PlayerState; PlayerId::COUNT],
    /// The field in play.
    field: Field,
    /// Each player's saved field.
    saved_fields: PlayerFields,
    /// Both players' emeralds.
    emeralds: EmeraldMap,
    /// The two digger records.
    diggers: [Digger; DiggerId::COUNT],
    /// Whether every digger's death sequence has finished, so the main
    /// phase ends after this tick (the original's `alldead`); cleared when
    /// the round's outcome has been decided.
    all_dead: bool,
    /// Whether the diggers are shown and updated (hashed as `digvisible`);
    /// false from the end of the main phase to the next round start.
    diggers_visible: bool,
    /// The animation counter of each digger sprite, by unit (not hashed).
    digger_frames: [AnimFrame; DiggerId::COUNT],
    /// The animation counter of each fireball sprite, by unit (not hashed).
    fireball_frames: [FlameFrame; DiggerId::COUNT],
    /// The six monster slots.
    monsters: [Monster; MonsterSlot::COUNT],
    /// The seven bags.
    bags: [Bag; BagId::COUNT],
    /// Both scores.
    scores: Scores,
    /// The gauntlet clock, in the gauntlet modes.
    gauntlet: Option<GauntletClock>,
    /// The direction resolver and fire latch of each control slot.
    inputs: [SlotInput; SlotId::COUNT],
    /// The tune of the last music request if it was for the main music,
    /// so a change of lives can switch between the main and the last-life
    /// tune (research 04 §3.1, `musicrefresh`).
    main_tune: Option<Tune>,
}

impl Sim {
    /// The state of a game just started under `config`: tick 0, round 0,
    /// zero scores, 3 lives for every digger record in play, every player's
    /// level set up, and player 1's turn (game-flow spec, "Game start";
    /// research 01 §3.1).
    fn new(config: GameConfig, base_seed: u64) -> Self {
        let mode = config.mode;
        let start = config.start_level.level();
        // Quirk Q06: outside alternating games the second player's level is
        // 0, which sets up an all-earth field without emeralds or bags. It
        // is reset at every game start (compat-quirks U09).
        let second = mode.alternating().then_some(start);
        let mut diggers = [Digger::default(); DiggerId::COUNT];
        // One record per digger, and one per player in alternating games.
        let in_play = usize::from(mode.diggers() + mode.players() - 1);
        for digger in diggers.iter_mut().take(in_play) {
            digger.lives = START_LIVES;
        }
        let scores = Scores::new(&config);
        let mut sim = Self {
            base_seed,
            rng: GameRng::new(0),
            penalty: Penalty::default(),
            ticks: 0,
            round: 0,
            turn: PlayerId::ALL[0],
            players: [
                PlayerState {
                    level: Some(start),
                    level_done: false,
                },
                PlayerState {
                    level: second,
                    level_done: false,
                },
            ],
            field: Field::EARTH,
            saved_fields: PlayerFields::default(),
            emeralds: EmeraldMap::EMPTY,
            diggers,
            all_dead: false,
            diggers_visible: false,
            digger_frames: [AnimFrame::START; DiggerId::COUNT],
            fireball_frames: [FlameFrame::START; DiggerId::COUNT],
            monsters: [Monster::default(); MonsterSlot::COUNT],
            bags: [Bag::default(); BagId::COUNT],
            scores,
            gauntlet: mode.gauntlet().map(GauntletClock::start),
            inputs: [SlotInput::new(); SlotId::COUNT],
            main_tune: None,
            cfg: config,
        };
        // Both player positions are always set up, the first then the
        // second, as in the original; the field in play is drawn from the
        // first player's save before play begins.
        for player in PlayerId::ALL {
            sim.set_up_level(player, None);
        }
        sim
    }

    /// Sets up `player`'s current level: field, emeralds and (from task
    /// group 11) bags, from `replacement` if given and otherwise from the
    /// level set as played (playfield spec, "Initial earth and tunnel
    /// state"; research 01 §3.1, `initlevel`).
    fn set_up_level(&mut self, player: PlayerId, replacement: Option<LevelLayout>) {
        let two_diggers = self.cfg.mode.two_diggers();
        let state = player.of_mut(&mut self.players);
        state.level_done = false;
        let layout = match (replacement, state.level) {
            // A supplied layout is used verbatim, without quirk Q07.
            (Some(layout), _) => layout,
            (None, Some(level)) => self.cfg.levels.layout_as_played(level.plan(), two_diggers),
            (None, None) => LevelLayout::EARTH,
        };
        self.field = Field::from_layout(&layout);
        self.saved_fields.save(player, &self.field);
        self.emeralds.place(player, &layout);
    }

    /// The digger record that score `id` belongs to and whose lives its
    /// bonus checks: record `id` (research 01 §6.2, `getlives(n)`).
    fn record_of(id: ScoreId) -> DiggerId {
        match DiggerId::new(id.number()) {
            Some(digger) => digger,
            None => unreachable!("there are as many digger records as scores"),
        }
    }

    /// The first digger record of the player whose turn it is.
    fn turn_record(&self) -> DiggerId {
        match DiggerId::new(self.turn.number()) {
            Some(digger) => digger,
            None => unreachable!("there are as many digger records as players"),
        }
    }

    /// The current player's level. The player whose turn it is always
    /// plays a level of at least 1.
    fn current_level(&self) -> LevelNumber {
        self.turn
            .of(&self.players)
            .level
            .unwrap_or(LevelNumber::FIRST)
    }

    /// The summary of the game as it stands, finished for `reason`.
    fn summary(&self, reason: FinishReason) -> GameSummary {
        GameSummary {
            reason,
            scores: ScoreId::ALL.map(|id| ScoreSummary {
                visible: self.scores.visible(id),
                total: self.scores.total(id),
            }),
            level: self.current_level(),
            ticks: self.ticks,
        }
    }
}

/// Everything a system may touch during one step, split into disjoint
/// borrows of the game once per step (design D4).
///
/// Every system is an `impl Ctx<'_>` block in its own module, so systems
/// call one another with ordinary re-borrows of `self`.
pub(crate) struct Ctx<'a> {
    /// The simulation state.
    sim: &'a mut Sim,
    /// The picture and the sprite layer.
    gfx: &'a mut Gfx,
    /// The sound commands of the frames since the last drain.
    sound: &'a mut SoundQueue,
    /// The observer of this step.
    obs: &'a mut dyn Observer,
}

impl Ctx<'_> {
    /// Advances `phase` by one frame and returns the phase that follows and
    /// the kind of the frame.
    fn advance(&mut self, phase: Phase, input: &FrameInput) -> (Phase, FrameKind) {
        match phase {
            Phase::Start => {
                self.start_game();
                let draw = self.begin_level_draw();
                self.level_draw_frame(draw)
            }
            Phase::LevelDraw(draw) => self.level_draw_frame(draw),
            Phase::Playing => {
                self.tick_main(input);
                if self.sim.all_dead {
                    return (self.round_over(), FrameKind::Tick);
                }
                (Phase::Playing, FrameKind::Tick)
            }
            Phase::Finished(_) => (phase, FrameKind::Wait),
        }
    }

    /// The game start's observer calls: once for every participating
    /// player's level, in player order. A supplied layout replaces the one
    /// set up when the game was created (design D8).
    fn start_game(&mut self) {
        for player in PlayerId::ALL {
            let Some(level) = player.of(&self.sim.players).level else {
                // The Q06 placeholder is never offered to the observer.
                continue;
            };
            let stored = *self.sim.cfg.levels.layout(level.plan());
            if let Some(layout) = self.obs.level_plan(player, level, &stored) {
                self.sim.set_up_level(player, Some(layout));
            }
        }
    }

    /// Starts the full draw of the current player's field: the saved field
    /// is put in play and the draw begins (design D3; research 01 §3.2,
    /// `drawscreen`). The sprites are reset to their initial images: the
    /// diggers' here, the bags' and monsters' by task groups 11 and 12.
    fn begin_level_draw(&mut self) -> LevelDraw {
        let turn = self.sim.turn;
        self.sim.field = self.sim.saved_fields.restore(turn);
        let plan: LevelPlan = self.sim.current_level().plan();
        self.reset_digger_sprites();
        LevelDraw::begin(self.gfx, &mut self.sim.field, plan)
    }

    /// One frame of the full draw; on the frame that draws the last item
    /// the round starts, so the next frame is the round's first tick.
    fn level_draw_frame(&mut self, mut draw: LevelDraw) -> (Phase, FrameKind) {
        let progress = draw.frame(
            self.gfx,
            &self.sim.field,
            &self.sim.emeralds,
            self.sim.turn,
            // The bags are drawn here from task group 11 on.
            |_| {},
        );
        match progress {
            DrawProgress::InProgress => (Phase::LevelDraw(draw), FrameKind::Wait),
            DrawProgress::Done => {
                self.begin_round(true);
                (Phase::Playing, FrameKind::Wait)
            }
        }
    }

    /// The round-start transition (game-flow spec, "Round start", "Per-round
    /// seeding"; research 01 §3.2).
    ///
    /// The generator is seeded with the round's seed as the observer
    /// returns it; after a full draw the diggers are placed and both scores
    /// drawn, otherwise the sprites are reset and the diggers placed (the
    /// monsters are reset here from task group 12 on); then
    /// the `PLAYER n` label is erased, every digger's score is refreshed by
    /// a zero-point scoring call, the lives are drawn, the main music is
    /// requested and the control state is cleared.
    fn begin_round(&mut self, after_full_draw: bool) {
        let round = self.sim.round;
        let derived = round_seed(self.sim.base_seed, round);
        let seed = self.obs.round_seed(round, derived);
        self.sim.rng.reseed(seed);

        if after_full_draw {
            self.init_diggers();
            self.draw_scores();
        } else {
            self.reset_digger_sprites();
            self.init_diggers();
        }
        status::erase_player_label(self.gfx.fb_mut());
        // The original refreshes scores 0..diggers, so in an alternating
        // game it is always the first score, whoever's turn it is
        // (research 01 §6.2, `initscores`).
        let diggers = usize::from(self.sim.cfg.mode.diggers());
        for id in ScoreId::ALL.into_iter().take(diggers) {
            self.add_score(id, ScoreEvent::Refresh);
        }
        self.draw_lives();
        self.request_main_music();
        for input in &mut self.sim.inputs {
            input.reset();
        }
    }

    /// One main-phase tick: the penalty counter is reset, the fire states
    /// read on the previous tick are latched, the diggers are updated and
    /// the tick is counted. Task 14.1 assembles the full update order.
    fn tick_main(&mut self, input: &FrameInput) {
        self.sim.penalty.reset();
        for slot in &mut self.sim.inputs {
            slot.advance_fire();
        }
        self.update_diggers(input);
        self.end_tick();
    }

    /// Counts the tick and tells the observer, after everything of the tick
    /// has happened.
    fn end_tick(&mut self) {
        // The original's tick counter is an unsigned 32-bit value.
        self.sim.ticks = self.sim.ticks.wrapping_add(1);
        self.obs
            .tick_end(self.sim.ticks, StateView::new(self.sim, self.gfx));
    }

    /// The round-end transition (game-flow spec, "Round end"): all sound is
    /// stopped, every fireball removed and the field saved for the player,
    /// then the observer sees the state and the round counter rises. Task
    /// 14.3 adds the removal of the bonus and monsters and the bag tidy in
    /// their places.
    fn end_round(&mut self) {
        self.sound.push(SoundCommand::StopAll);
        for unit in self.sim.units() {
            let record = self.sim.record_of_unit(unit);
            self.remove_fireball(unit, record);
        }
        self.sim.saved_fields.save(self.sim.turn, &self.sim.field);
        self.obs.round_end(StateView::new(self.sim, self.gfx));
        self.sim.round = self.sim.round.wrapping_add(1);
    }

    /// Ends a round whose diggers are all dead and decides what follows
    /// (game-flow spec, "Round end", "Death outcome and passing the turn";
    /// research 01 §3.2).
    ///
    /// Outside gauntlet mode every digger in play with lives left loses
    /// one, and the lives are redrawn. While the current player has lives
    /// the next round starts on the same field on the next frame; otherwise
    /// the game is over.
    ///
    /// Task group 14 completes this: the wind-down runs before the round
    /// end (14.2), a field without emeralds advances the level (14.4), the
    /// turn passes in an alternating game (14.5), and game over shows its
    /// notice and reports the player (14.6).
    fn round_over(&mut self) -> Phase {
        self.end_round();
        let gauntlet = self.sim.cfg.mode.gauntlet().is_some();
        for unit in self.sim.units() {
            let digger = self.sim.record_of_unit(unit).of_mut(&mut self.sim.diggers);
            if digger.lives > 0 && !gauntlet {
                digger.lives -= 1;
            }
        }
        self.draw_lives();
        self.sim.all_dead = false;
        let lives_left = self
            .sim
            .units()
            .any(|unit| self.sim.record_of_unit(unit).of(&self.sim.diggers).lives > 0);
        if !lives_left && !gauntlet {
            let summary = self.sim.summary(FinishReason::AllPlayersOut);
            self.obs.game_end(StateView::new(self.sim, self.gfx));
            return Phase::Finished(summary);
        }
        self.begin_round(false);
        Phase::Playing
    }

    /// Ends the game for `reason`: through the round end when a round is in
    /// progress, otherwise by stopping all sound; then the observer sees the
    /// final state (game-flow spec, "Quit"; compat-quirks U11).
    fn finish(&mut self, reason: FinishReason, in_round: bool) -> GameSummary {
        if in_round {
            self.end_round();
        } else {
            self.sound.push(SoundCommand::StopAll);
        }
        let summary = self.sim.summary(reason);
        self.obs.game_end(StateView::new(self.sim, self.gfx));
        summary
    }

    /// The scoring call for `event` on score `id`, with its bonus applied
    /// (scoring spec; research 01 §6.2, `addscore`).
    fn add_score(&mut self, id: ScoreId, event: ScoreEvent) {
        let lives = Sim::record_of(id).of(&self.sim.diggers).lives;
        let effect =
            self.sim
                .scores
                .add_event(id, event, lives, &mut self.sim.penalty, self.gfx.fb_mut());
        self.apply_bonus(id, effect);
    }

    /// Applies what a scoring call's bonus asks for: a life for score
    /// `id`'s digger record with the extra-life cue and a music refresh, or
    /// gauntlet time; the lives display is redrawn either way (research 01
    /// §6.2, `addlife`).
    fn apply_bonus(&mut self, id: ScoreId, effect: BonusEffect) {
        match effect {
            BonusEffect::None => return,
            BonusEffect::ExtraLife => {
                let digger = Sim::record_of(id).of_mut(&mut self.sim.diggers);
                digger.lives = digger.lives.saturating_add(1);
                self.sound.push(SoundCommand::ExtraLife);
                self.refresh_main_music();
            }
            BonusEffect::GauntletTime => {
                if let Some(clock) = &mut self.sim.gauntlet {
                    clock.add_bonus();
                }
            }
        }
        self.draw_lives();
    }

    /// Draws both scores in text colour 3, the second only when the mode has
    /// two scores (research 01 §6.2, `drawscores`).
    fn draw_scores(&mut self) {
        let mode = self.sim.cfg.mode;
        let count = if mode.alternating() || mode.two_diggers() {
            2
        } else {
            1
        };
        for id in ScoreId::ALL.into_iter().take(count) {
            score::draw_score(
                self.gfx.fb_mut(),
                id,
                self.sim.scores.visible(id),
                TextColor::Three,
            );
        }
    }

    /// Draws the lives display, or the gauntlet clock in its place
    /// (playfield spec, "Lives display"; research 01 §6.5).
    fn draw_lives(&mut self) {
        let fb: &mut Framebuffer = self.gfx.fb_mut();
        if let Some(clock) = self.sim.gauntlet {
            status::draw_gauntlet_clock(fb, clock.units());
            return;
        }
        let mode = self.sim.cfg.mode;
        let [first, second] = self.sim.diggers.map(|digger| digger.lives);
        let second = if mode.alternating() {
            Some(SecondSide::Player2 { lives: second })
        } else if mode.two_diggers() {
            Some(SecondSide::Digger2 { lives: second })
        } else {
            None
        };
        status::draw_lives(fb, first, second);
    }

    /// The main tune for the current lives: the last-life tune while the
    /// current player's first digger has exactly 1 life (game-flow spec,
    /// "Music requests").
    fn main_tune(&self) -> Tune {
        if self.sim.turn_record().of(&self.sim.diggers).lives == 1 {
            Tune::LastLife
        } else {
            Tune::Main
        }
    }

    /// Requests the main music at standard tempo, marked for the last life
    /// where that applies.
    fn request_main_music(&mut self) {
        let tune = self.main_tune();
        self.sim.main_tune = Some(tune);
        self.sound.push(SoundCommand::Music {
            tune,
            tempo: Tempo::Standard,
        });
    }

    /// Requests the main music again if it is the music last requested and
    /// the change of lives selects the other main tune (research 04 §3.1,
    /// `musicrefresh`).
    fn refresh_main_music(&mut self) {
        if self
            .sim
            .main_tune
            .is_some_and(|playing| playing != self.main_tune())
        {
            self.request_main_music();
        }
    }
}

/// A game of Digger: one owning value advanced one frame per
/// [`step`](Self::step) (design D2, D3).
///
/// See the crate documentation for the stepping loop.
#[derive(Debug, Clone)]
pub struct Game {
    /// Where the game stands.
    phase: Phase,
    /// The simulation state.
    sim: Sim,
    /// The picture and the sprite layer.
    gfx: Gfx,
    /// Sound commands not yet drained.
    sound: SoundQueue,
}

impl Game {
    /// A new game under `config`, with round seeds derived from
    /// `base_seed`. No frame has run yet: the first step starts the level
    /// draw.
    ///
    /// The game is boxed because it owns the picture; this is the only
    /// allocation the simulation makes (design D2, D9).
    #[allow(
        clippy::disallowed_types,
        clippy::disallowed_methods,
        reason = "the game value is allocated once, here (design D9)"
    )]
    pub fn new(config: GameConfig, base_seed: u64) -> Box<Self> {
        Box::new(Self {
            phase: Phase::Start,
            sim: Sim::new(config, base_seed),
            gfx: Gfx::new(),
            sound: SoundQueue::new(),
        })
    }

    /// Advances the game by exactly one frame with this frame's `input`
    /// and no observer.
    pub fn step(&mut self, input: &FrameInput) -> StepOutcome {
        self.step_observed(input, &mut NoopObserver)
    }

    /// Advances the game by exactly one frame with this frame's `input`,
    /// letting `obs` observe and replace round seeds, layouts, controls and
    /// movement samples at the points described on [`Observer`].
    ///
    /// A quit request ends the game on this frame without running a tick.
    /// Once the game has finished, every step changes nothing and reports
    /// the same summary.
    pub fn step_observed(&mut self, input: &FrameInput, obs: &mut dyn Observer) -> StepOutcome {
        if let Phase::Finished(summary) = self.phase {
            return StepOutcome::finished(summary);
        }
        let mut cx = Ctx {
            sim: &mut self.sim,
            gfx: &mut self.gfx,
            sound: &mut self.sound,
            obs,
        };
        if input.quit {
            let summary = cx.finish(FinishReason::Quit, self.phase.in_round());
            self.phase = Phase::Finished(summary);
            return StepOutcome::finished(summary);
        }
        let (phase, frame) = cx.advance(self.phase, input);
        self.phase = phase;
        let mut outcome = StepOutcome::of(frame);
        if let Phase::Finished(summary) = phase {
            outcome.finished = Some(summary);
        }
        outcome
    }

    /// The picture: 640×400 palette indices with the current palette.
    pub fn framebuffer(&self) -> &Framebuffer {
        self.gfx.fb()
    }

    /// Removes and yields the sound commands produced since the last drain,
    /// in the order their events happened.
    ///
    /// The queue holds 64 commands; a caller that never drains loses the
    /// oldest ones, which [`dropped_sound_commands`](Self::dropped_sound_commands)
    /// counts. Sound never affects the game.
    pub fn drain_sound(&mut self) -> impl Iterator<Item = SoundCommand> + '_ {
        self.sound.drain()
    }

    /// The number of sound commands dropped so far because the queue was
    /// full.
    pub fn dropped_sound_commands(&self) -> u64 {
        self.sound.dropped()
    }

    /// A read-only view of the simulation state.
    pub fn state(&self) -> StateView<'_> {
        StateView::new(&self.sim, &self.gfx)
    }

    /// The summary of the game once it has finished.
    pub fn summary(&self) -> Option<GameSummary> {
        match self.phase {
            Phase::Finished(summary) => Some(summary),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one_player() -> Box<Game> {
        Game::new(GameConfig::default(), 0)
    }

    /// Steps `game` until the first tick frame and returns the number of
    /// waiting frames before it.
    fn frames_to_first_tick(game: &mut Game) -> u32 {
        let mut waits = 0;
        while game.step(&FrameInput::NONE).frame == FrameKind::Wait {
            waits += 1;
            assert!(waits < 1000, "no tick within 1000 frames");
        }
        waits
    }

    #[test]
    fn new_game_starts_at_tick_0_round_0() {
        let config = GameConfig {
            start_level: StartLevel::new(5).unwrap_or_default(),
            ..GameConfig::default()
        };
        let game = Game::new(config, 0);
        let state = game.state();
        assert_eq!(state.ticks(), 0);
        assert_eq!(state.round(), 0);
        assert_eq!(state.current_player(), PlayerId::ALL[0]);
        assert_eq!(state.level(PlayerId::ALL[0]), LevelNumber::new(5));
        for id in ScoreId::ALL {
            assert_eq!(state.scores().visible(id), 0);
            assert_eq!(state.scores().total(id), 0);
        }
        assert_eq!(state.lives(DiggerId::ALL[0]), 3);
        assert_eq!(state.lives(DiggerId::ALL[1]), 0);
        assert_eq!(game.summary(), None);

        // Two players and two diggers both have a second record in play.
        for mode in [GameMode::TwoPlayers, GameMode::TwoDiggers] {
            let config = GameConfig {
                mode,
                ..GameConfig::default()
            };
            let game = Game::new(config, 0);
            assert_eq!(game.state().lives(DiggerId::ALL[1]), 3);
        }
    }

    #[test]
    fn quirk_q06_second_player_position_is_level_0() {
        let second = PlayerId::ALL[1];
        let fresh = Game::new(GameConfig::default(), 0);
        let check = |game: &Game| {
            let state = game.state();
            assert_eq!(state.level(second), None);
            assert_eq!(state.saved_field(second), &Field::EARTH);
            assert_eq!(state.emeralds().count(second), 0);
        };
        check(&fresh);

        // An alternating game sets up the second player at the start level.
        let config = GameConfig {
            mode: GameMode::TwoPlayers,
            start_level: StartLevel::new(3).unwrap_or_default(),
            ..GameConfig::default()
        };
        let mut two = Game::new(config, 7);
        assert_eq!(two.state().level(second), LevelNumber::new(3));
        assert_ne!(two.state().saved_field(second), &Field::EARTH);
        assert!(two.state().emeralds().count(second) > 0);
        for _ in 0..50 {
            two.step(&FrameInput::NONE);
        }

        // A one-player game after it holds level 0 state exactly as fresh.
        let after = Game::new(GameConfig::default(), 0);
        check(&after);
        assert_eq!(
            after.state().saved_field(second),
            fresh.state().saved_field(second)
        );
        assert_eq!(after.state().emeralds(), fresh.state().emeralds());
    }

    #[test]
    fn level_draw_frames_are_wait_frames() {
        // Level 1 has 33 opened cells and 30 emeralds: 21 frames.
        let mut game = one_player();
        for frame in 0..21 {
            let outcome = game.step(&FrameInput::NONE);
            assert_eq!(outcome.frame, FrameKind::Wait, "frame {frame}");
            assert_eq!(game.state().ticks(), 0);
        }
        let outcome = game.step(&FrameInput::NONE);
        assert_eq!(outcome.frame, FrameKind::Tick);
        assert_eq!(game.state().ticks(), 1);
        assert_eq!(game.step(&FrameInput::NONE).frame, FrameKind::Tick);
        assert_eq!(game.state().ticks(), 2);
    }

    #[test]
    fn input_during_level_draw_is_ignored() {
        let held = FrameInput {
            slots: [
                Controls {
                    right: true,
                    fire: true,
                    ..Controls::NONE
                },
                Controls::NONE,
            ],
            ..FrameInput::NONE
        };
        let mut plain = one_player();
        let mut pressed = one_player();
        assert_eq!(frames_to_first_tick(&mut plain), 21);
        for _ in 0..21 {
            pressed.step(&held);
        }
        assert_eq!(pressed.step(&FrameInput::NONE).frame, FrameKind::Tick);
        assert_eq!(plain.sim.inputs, pressed.sim.inputs);
    }

    #[test]
    fn round_0_seeded_from_base() {
        let mut game = one_player();
        frames_to_first_tick(&mut game);
        assert_eq!(game.state().rng_state().cast_unsigned(), 0x7B1D_CDAF);
        assert_eq!(game.state().round(), 0);

        let mut game = Game::new(GameConfig::default(), 1);
        frames_to_first_tick(&mut game);
        assert_eq!(game.state().rng_state().cast_unsigned(), 0x8902_5CC1);
    }

    /// An observer that replaces every round seed and records the calls.
    #[derive(Default)]
    struct SeedSwap {
        seen: Vec<(u32, i32)>,
    }

    impl Observer for SeedSwap {
        fn round_seed(&mut self, round: u32, derived: i32) -> i32 {
            self.seen.push((round, derived));
            1234
        }
    }

    #[test]
    fn observer_can_replace_round_seed() {
        let mut game = one_player();
        let mut obs = SeedSwap::default();
        while game.step_observed(&FrameInput::NONE, &mut obs).frame == FrameKind::Wait {}
        assert_eq!(obs.seen, [(0, round_seed(0, 0))]);
        assert_eq!(game.state().rng_state(), 1234);
    }

    #[test]
    fn round_start_emits_main_music() {
        let mut game = one_player();
        for _ in 0..20 {
            game.step(&FrameInput::NONE);
            assert_eq!(game.drain_sound().count(), 0);
        }
        // The last draw frame starts the round.
        assert_eq!(game.step(&FrameInput::NONE).frame, FrameKind::Wait);
        let sounds: Vec<_> = game.drain_sound().collect();
        assert_eq!(
            sounds,
            [SoundCommand::Music {
                tune: Tune::Main,
                tempo: Tempo::Standard,
            }]
        );
    }

    #[test]
    fn last_life_marks_main_music() {
        let mut game = one_player();
        game.sim.diggers[0].lives = 1;
        frames_to_first_tick(&mut game);
        let sounds: Vec<_> = game.drain_sound().collect();
        assert_eq!(
            sounds,
            [SoundCommand::Music {
                tune: Tune::LastLife,
                tempo: Tempo::Standard,
            }]
        );
    }

    #[test]
    fn round_start_draws_scores_and_lives() {
        let mut game = one_player();
        frames_to_first_tick(&mut game);
        // A score of 0 shows as 00000 from x = 12, and two spare-life
        // icons at x = 80 and 100.
        let mut expected = Framebuffer::new();
        expected.draw_number(0, 6, crate::geom::Px::new(0, 0), TextColor::One);
        status::draw_lives(&mut expected, 3, None);
        let rows = 14 * Framebuffer::WIDTH * 2;
        assert_eq!(
            game.framebuffer().pixels().get(..rows),
            expected.pixels().get(..rows)
        );
    }
}
