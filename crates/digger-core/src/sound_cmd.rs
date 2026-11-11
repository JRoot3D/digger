//! Sound commands emitted by the simulation and their fixed-capacity queue
//! (design D8).
//!
//! The simulation never plays sound. It pushes one [`SoundCommand`] per
//! sound event into a queue, in the order the events happen within the
//! frame, and the caller drains the queue after each step. Nothing about
//! playback feeds back into the game (game-flow spec, "Sound commands per
//! frame"; research 04 §1.5, §5).

use crate::sim::index_id;
use crate::sprite::SpriteId;

index_id!(
    /// One of the two fireballs, 0 or 1: fireball `i` belongs to digger `i`
    /// (research 02 §3.6).
    FireballId,
    2: [0, 1]
);

impl FireballId {
    /// The fireball's sprite, slot 14 + index.
    pub const fn sprite(self) -> SpriteId {
        match SpriteId::fireball(self.number()) {
            Some(id) => id,
            None => unreachable!(),
        }
    }
}

index_id!(
    /// The position of an eaten emerald in a run of consecutive emeralds,
    /// 0..=7: it picks the note of the rising scale C5..C6 (research 04
    /// §2, research 02 §3.4).
    EmeraldStep,
    8: [0, 1, 2, 3, 4, 5, 6, 7]
);

/// A piece of music (research 04 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tune {
    /// The main music, played during a round.
    Main,
    /// The main music's replacement while the current player's first
    /// digger has exactly 1 life left.
    LastLife,
    /// The bonus-mode music.
    Bonus,
    /// The dirge played after a digger's death.
    Dirge,
}

/// The speed at which a tune is played (research 04 §3.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum Tempo {
    /// Notes at their written length.
    #[default]
    Standard,
    /// Notes at 0.7 of their written length; used only for the dirge in
    /// two-digger games.
    Quick,
}

/// One sound event produced by the simulation (design D8).
///
/// Commands are delivered in event order within the frame that caused
/// them. The set may grow, so matches need a wildcard arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SoundCommand {
    /// A bag starts falling.
    FallStart,
    /// No bag is falling any more.
    FallStop,
    /// A bag starts wobbling.
    WobbleStart,
    /// No bag is wobbling any more.
    WobbleStop,
    /// A bag breaks open into gold.
    Break,
    /// A digger collects gold.
    Gold,
    /// A fireball is launched (or drawn again); its sweep starts.
    Fire(FireballId),
    /// A fireball's flight sound stops.
    FireStop(FireballId),
    /// A fireball explodes.
    Explode(FireballId),
    /// A digger eats an emerald: the click.
    EmeraldClick,
    /// A digger eats an emerald: the note of its position in the run.
    EmeraldNote(EmeraldStep),
    /// A monster is eaten in bonus mode.
    EatMonster,
    /// A digger dies.
    DiggerDeath,
    /// A bonus life is awarded.
    ExtraLife,
    /// The siren before and at the end of bonus mode starts.
    BonusPulseStart,
    /// The bonus-mode siren stops.
    BonusPulseStop,
    /// A tune starts, replacing any tune playing.
    Music {
        /// The tune to play.
        tune: Tune,
        /// Its tempo.
        tempo: Tempo,
    },
    /// The music stops.
    MusicOff,
    /// The level-complete jingle plays.
    LevelDone,
    /// The game is paused: all sound freezes.
    Pause,
    /// The game resumes: frozen sound continues.
    Resume,
    /// Every sound and the music stop.
    StopAll,
}

/// The number of commands the queue holds before it drops the oldest.
pub(crate) const SOUND_QUEUE_CAP: usize = 64;

/// The commands of the frames since the last drain, oldest first, in a
/// fixed-capacity ring (design D8, D9).
///
/// A frame produces far fewer than [`SOUND_QUEUE_CAP`] commands. If the
/// caller never drains, the oldest commands are dropped (sound is not
/// simulation state) and counted, so a test can tell that it happened.
#[derive(Debug, Clone)]
pub(crate) struct SoundQueue {
    /// Ring storage; entries outside `start..start + len` are stale.
    items: [SoundCommand; SOUND_QUEUE_CAP],
    /// Index of the oldest command.
    start: usize,
    /// Number of queued commands.
    len: usize,
    /// Number of commands dropped because the queue was full.
    dropped: u64,
}

#[allow(dead_code, reason = "owned by the game from task group 7")]
impl SoundQueue {
    /// An empty queue.
    pub(crate) const fn new() -> Self {
        Self {
            items: [SoundCommand::StopAll; SOUND_QUEUE_CAP],
            start: 0,
            len: 0,
            dropped: 0,
        }
    }

    /// Appends `command`; when the queue is full the oldest command is
    /// dropped first.
    pub(crate) fn push(&mut self, command: SoundCommand) {
        if self.len == SOUND_QUEUE_CAP {
            self.start = (self.start + 1) % SOUND_QUEUE_CAP;
            self.len -= 1;
            self.dropped += 1;
        }
        let end = (self.start + self.len) % SOUND_QUEUE_CAP;
        if let Some(slot) = self.items.get_mut(end) {
            *slot = command;
            self.len += 1;
        }
    }

    /// The number of queued commands.
    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    /// Whether no command is queued.
    pub(crate) const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The number of commands dropped so far because the queue was full.
    pub(crate) const fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Removes and yields the queued commands, oldest first. Commands not
    /// consumed by the iterator are still removed when it is dropped.
    pub(crate) fn drain(&mut self) -> SoundDrain<'_> {
        SoundDrain { queue: self }
    }

    /// Removes the oldest command.
    fn pop(&mut self) -> Option<SoundCommand> {
        if self.len == 0 {
            return None;
        }
        let command = self.items.get(self.start).copied();
        self.start = (self.start + 1) % SOUND_QUEUE_CAP;
        self.len -= 1;
        command
    }
}

impl Default for SoundQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// The draining iterator of a [`SoundQueue`], oldest command first.
#[derive(Debug)]
pub(crate) struct SoundDrain<'a> {
    /// The queue being emptied.
    queue: &'a mut SoundQueue,
}

impl Iterator for SoundDrain<'_> {
    type Item = SoundCommand;

    fn next(&mut self) -> Option<SoundCommand> {
        self.queue.pop()
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.queue.len, Some(self.queue.len))
    }
}

impl ExactSizeIterator for SoundDrain<'_> {}

impl Drop for SoundDrain<'_> {
    fn drop(&mut self) {
        self.queue.start = 0;
        self.queue.len = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(n: u8) -> SoundCommand {
        SoundCommand::EmeraldNote(EmeraldStep::new(n % 8).unwrap_or_else(|| unreachable!()))
    }

    #[test]
    fn emerald_step_is_checked() {
        assert_eq!(
            EmeraldStep::ALL.map(EmeraldStep::number),
            [0, 1, 2, 3, 4, 5, 6, 7]
        );
        assert!(EmeraldStep::new(7).is_some());
        assert!(EmeraldStep::new(8).is_none());
        assert!(EmeraldStep::try_from(8).is_err());
    }

    #[test]
    fn sound_queue_preserves_order() {
        let mut queue = SoundQueue::new();
        let commands = [
            SoundCommand::EmeraldClick,
            note(0),
            SoundCommand::Break,
            SoundCommand::Fire(FireballId::ALL[1]),
            SoundCommand::Music {
                tune: Tune::Dirge,
                tempo: Tempo::Quick,
            },
        ];
        for command in commands {
            queue.push(command);
        }
        assert_eq!(queue.len(), 5);
        assert!(queue.drain().eq(commands));

        // Order also holds across the ring's wrap-around.
        for n in 0..60 {
            queue.push(note(n));
        }
        let _ = queue.drain().count();
        for n in 0..20 {
            queue.push(note(n));
        }
        assert!(queue.drain().eq((0..20).map(note)));
        assert_eq!(queue.dropped(), 0);
    }

    #[test]
    fn sound_queue_drops_oldest_when_full() {
        let mut queue = SoundQueue::new();
        let fire =
            |n: u8| SoundCommand::Fire(FireballId::new(n % 2).unwrap_or_else(|| unreachable!()));
        for n in 0..67 {
            queue.push(fire(n));
            queue.push(note(n));
        }
        // 2 × 67 pushed into 64 places: the 70 oldest are gone.
        assert_eq!(queue.len(), SOUND_QUEUE_CAP);
        assert_eq!(queue.dropped(), 70);
        let kept: [SoundCommand; SOUND_QUEUE_CAP] = core::array::from_fn(|i| {
            let push = 70 + i;
            let n = u8::try_from(push / 2).unwrap_or_else(|_| unreachable!());
            if push % 2 == 0 { fire(n) } else { note(n) }
        });
        assert!(queue.drain().eq(kept));
    }

    #[test]
    fn drain_empties_queue() {
        let mut queue = SoundQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.drain().next(), None);
        queue.push(SoundCommand::Pause);
        queue.push(SoundCommand::Resume);
        queue.push(SoundCommand::StopAll);
        assert_eq!(queue.drain().len(), 3);
        assert!(queue.is_empty());

        // A partly consumed drain still empties the queue.
        queue.push(SoundCommand::Gold);
        queue.push(SoundCommand::LevelDone);
        assert_eq!(queue.drain().next(), Some(SoundCommand::Gold));
        assert!(queue.is_empty());
        assert_eq!(queue.drain().next(), None);
        queue.push(SoundCommand::MusicOff);
        assert!(queue.drain().eq([SoundCommand::MusicOff]));
    }
}
