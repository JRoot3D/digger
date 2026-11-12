//! The frame pacer over an injected `Clock`: absolute deadlines, the cap on
//! due frames and re-anchoring (design D4).
//!
//! The pacer is a plain state machine; it never sleeps and never reads the
//! time itself except through its [`Clock`]. One wake-up of the event loop
//! is one *batch*:
//!
//! 1. [`Pacer::begin`] decides how many frames the batch may run: the
//!    deadlines that have passed (at most [`MAX_DUE`]) when paced, or a time
//!    budget of [`UNPACED_BUDGET`] when unpaced.
//! 2. [`Pacer::next_frame`] is asked before each frame; it says whether
//!    another frame runs in this batch.
//! 3. [`Pacer::end`] says whether to present the picture and when to wake
//!    up next.
//!
//! Paced frame `k` after an anchor is due at `anchor + k × frame time`, so a
//! late wake-up delays one frame but never the schedule (`desktop-app`
//! spec, frame pacing). Falling more than [`MAX_DUE`] frame times behind
//! restarts the schedule from the current time (recovery after a stall).

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the pacer is driven by the platform event loop of a later task group"
    )
)]

use std::time::{Duration, Instant};

use crate::config::FrameTime;

/// The most frames one batch runs when paced; falling further behind than
/// this many frame times re-anchors the schedule.
pub(crate) const MAX_DUE: u32 = 3;

/// How long one unpaced batch runs frames before returning to the event
/// loop.
pub(crate) const UNPACED_BUDGET: Duration = Duration::from_millis(8);

/// The least time between two presented pictures when unpaced.
pub(crate) const UNPACED_REDRAW: Duration = Duration::from_millis(16);

/// A monotonic clock: the time since some fixed origin.
pub(crate) trait Clock {
    /// The time now, measured from the clock's origin.
    fn now(&self) -> Duration;
}

/// The real clock: the time since the clock was made.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SystemClock {
    origin: Instant,
}

impl SystemClock {
    /// A clock whose origin is now.
    pub(crate) fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }

    /// The instant `at` after the origin, for `ControlFlow::WaitUntil`.
    pub(crate) fn instant(&self, at: Duration) -> Instant {
        self.origin + at
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// What the event loop does after a batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wake {
    /// Whether the picture is presented now.
    pub(crate) redraw: bool,
    /// When the event loop wakes up next.
    pub(crate) control: Control,
}

/// When the event loop wakes up next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Control {
    /// At this time of the pacer's clock, or earlier for input.
    WaitUntil(Duration),
    /// At once, after handling pending events (unpaced mode).
    Poll,
}

/// The schedule: paced from an anchor, or unpaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Schedule {
    /// Frame `k` after the anchor is due at `anchor + frame_time × k`.
    Paced {
        /// The frame length, never zero.
        frame_time: Duration,
        /// Where frame 0 of the schedule is due.
        anchor: Duration,
        /// The frames run since the anchor: the next frame's number.
        frames_since_anchor: u64,
    },
    /// As fast as possible; the picture is presented every
    /// [`UNPACED_REDRAW`].
    Unpaced {
        /// When the last picture was presented, if one was.
        last_redraw: Option<Duration>,
    },
}

/// The current batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Batch {
    /// No batch is open, or it was cut short by a change of frame time.
    Closed,
    /// So many paced frames remain.
    Frames(u32),
    /// Unpaced frames run until this time; the first frame always runs.
    Until(Duration),
}

/// The frame pacer (design D4).
#[derive(Debug)]
pub(crate) struct Pacer<C: Clock> {
    clock: C,
    schedule: Schedule,
    batch: Batch,
    /// The frames run in the current batch.
    ran: u32,
}

impl<C: Clock> Pacer<C> {
    /// A pacer at `frame_time` whose first frame is due now.
    pub(crate) fn new(clock: C, frame_time: FrameTime) -> Self {
        let now = clock.now();
        Self {
            clock,
            schedule: Self::schedule_at(frame_time, now),
            batch: Batch::Closed,
            ran: 0,
        }
    }

    /// The clock the pacer reads.
    pub(crate) const fn clock(&self) -> &C {
        &self.clock
    }

    /// The current frame time.
    pub(crate) fn frame_time(&self) -> FrameTime {
        match self.schedule {
            Schedule::Paced { frame_time, .. } => FrameTime::from_duration(frame_time),
            Schedule::Unpaced { .. } => FrameTime::UNPACED,
        }
    }

    /// When the next paced frame is due; `None` when unpaced.
    pub(crate) fn next_deadline(&self) -> Option<Duration> {
        match self.schedule {
            Schedule::Paced {
                frame_time,
                anchor,
                frames_since_anchor,
            } => Some(deadline(anchor, frame_time, frames_since_anchor)),
            Schedule::Unpaced { .. } => None,
        }
    }

    /// Opens a batch at the current time.
    ///
    /// When paced, the batch holds the frames whose deadlines have passed,
    /// at most [`MAX_DUE`]; if the next frame is more than [`MAX_DUE`] frame
    /// times late, the schedule restarts now and the batch holds one frame.
    /// When unpaced, the batch runs frames for [`UNPACED_BUDGET`].
    pub(crate) fn begin(&mut self) {
        let now = self.clock.now();
        self.ran = 0;
        self.batch = match &mut self.schedule {
            Schedule::Paced {
                frame_time,
                anchor,
                frames_since_anchor,
            } => {
                let next = deadline(*anchor, *frame_time, *frames_since_anchor);
                match now.checked_sub(next) {
                    None => Batch::Frames(0),
                    Some(late) if late > *frame_time * MAX_DUE => {
                        *anchor = now;
                        *frames_since_anchor = 0;
                        Batch::Frames(1)
                    }
                    Some(late) => {
                        // `late` is at most MAX_DUE frame times, so at most
                        // MAX_DUE + 1 deadlines have passed.
                        let passed = 1 + late.as_nanos() / frame_time.as_nanos();
                        Batch::Frames(u32::try_from(passed).unwrap_or(MAX_DUE).min(MAX_DUE))
                    }
                }
            }
            Schedule::Unpaced { .. } => Batch::Until(now + UNPACED_BUDGET),
        };
    }

    /// Whether another frame runs in the open batch; if so, it is counted
    /// as run.
    pub(crate) fn next_frame(&mut self) -> bool {
        let runs = match self.batch {
            Batch::Closed => false,
            Batch::Frames(remaining) => {
                if remaining == 0 {
                    false
                } else {
                    self.batch = Batch::Frames(remaining - 1);
                    if let Schedule::Paced {
                        frames_since_anchor,
                        ..
                    } = &mut self.schedule
                    {
                        *frames_since_anchor += 1;
                    }
                    true
                }
            }
            Batch::Until(until) => self.ran == 0 || self.clock.now() < until,
        };
        if runs {
            self.ran += 1;
        }
        runs
    }

    /// Closes the batch: whether to present the picture and when to wake
    /// up next.
    ///
    /// Paced, the picture is presented when a frame ran and the loop waits
    /// for the next deadline. Unpaced, the picture is presented when
    /// [`UNPACED_REDRAW`] has passed since the last one and the loop polls.
    pub(crate) fn end(&mut self) -> Wake {
        let ran = self.ran;
        self.batch = Batch::Closed;
        self.ran = 0;
        let now = self.clock.now();
        match &mut self.schedule {
            Schedule::Paced {
                frame_time,
                anchor,
                frames_since_anchor,
            } => Wake {
                redraw: ran > 0,
                control: Control::WaitUntil(deadline(*anchor, *frame_time, *frames_since_anchor)),
            },
            Schedule::Unpaced { last_redraw } => {
                let redraw = ran > 0
                    && last_redraw.is_none_or(|last| now.saturating_sub(last) >= UNPACED_REDRAW);
                if redraw {
                    *last_redraw = Some(now);
                }
                Wake {
                    redraw,
                    control: Control::Poll,
                }
            }
        }
    }

    /// Changes the frame time from the next frame on. A paced schedule
    /// restarts at the next frame's deadline, so that frame keeps its time
    /// and the ones after it follow at the new frame time; changing to or
    /// from unpaced restarts the schedule now. The open batch ends.
    pub(crate) fn set_frame_time(&mut self, frame_time: FrameTime) {
        let now = self.clock.now();
        let restart = match (self.next_deadline(), frame_time.is_unpaced()) {
            (Some(next), false) => next,
            _ => now,
        };
        if frame_time != self.frame_time() {
            self.schedule = Self::schedule_at(frame_time, restart);
        }
        self.batch = Batch::Closed;
    }

    /// Restarts the schedule now: the next frame is due at once. Used when
    /// a pause ends, focus returns or a scene changes the frame length.
    pub(crate) fn reanchor(&mut self) {
        let now = self.clock.now();
        if let Schedule::Paced {
            anchor,
            frames_since_anchor,
            ..
        } = &mut self.schedule
        {
            *anchor = now;
            *frames_since_anchor = 0;
        }
    }

    /// A new schedule at `frame_time` whose frame 0 is due at `at`.
    fn schedule_at(frame_time: FrameTime, at: Duration) -> Schedule {
        if frame_time.is_unpaced() {
            Schedule::Unpaced { last_redraw: None }
        } else {
            Schedule::Paced {
                frame_time: frame_time.duration(),
                anchor: at,
                frames_since_anchor: 0,
            }
        }
    }
}

/// Where frame `k` after `anchor` is due.
fn deadline(anchor: Duration, frame_time: Duration, k: u64) -> Duration {
    let nanos = frame_time.as_nanos().saturating_mul(u128::from(k));
    anchor + Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

#[cfg(test)]
#[allow(
    clippy::panic,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    /// A clock the test sets by hand; clones share the time.
    #[derive(Debug, Clone, Default)]
    struct FakeClock(Rc<Cell<Duration>>);

    impl FakeClock {
        fn set(&self, at: Duration) {
            self.0.set(at);
        }

        fn advance(&self, by: Duration) {
            self.0.set(self.0.get() + by);
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            self.0.get()
        }
    }

    const DEFAULT: Duration = Duration::from_millis(80);

    fn pacer(frame_time: Duration) -> (FakeClock, Pacer<FakeClock>) {
        let clock = FakeClock::default();
        let pacer = Pacer::new(clock.clone(), FrameTime::from_duration(frame_time));
        (clock, pacer)
    }

    /// Runs one batch; returns the frames it ran and how it ended.
    fn batch(pacer: &mut Pacer<FakeClock>) -> (u32, Wake) {
        pacer.begin();
        let mut frames = 0;
        while pacer.next_frame() {
            frames += 1;
        }
        (frames, pacer.end())
    }

    fn wait_until(wake: Wake) -> Duration {
        match wake.control {
            Control::WaitUntil(at) => at,
            Control::Poll => panic!("a paced batch polls"),
        }
    }

    /// A small deterministic generator for the wake-up jitter.
    struct Lcg(u64);

    impl Lcg {
        fn next_below(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 33) % bound
        }
    }

    #[test]
    fn pacer_no_drift_7500_frames() {
        let (clock, mut pacer) = pacer(DEFAULT);
        let mut jitter = Lcg(0x5eed);
        let end = Duration::from_secs(600);
        let mut frames = 0_u64;
        let mut redraws = 0_u64;
        loop {
            let (ran, wake) = batch(&mut pacer);
            frames += u64::from(ran);
            redraws += u64::from(wake.redraw);
            // The operating system wakes the loop up to 15 ms late; now
            // and then input wakes it early.
            let next = if jitter.next_below(10) == 0 {
                clock.now() + Duration::from_millis(jitter.next_below(30))
            } else {
                wait_until(wake) + Duration::from_micros(jitter.next_below(15_000))
            };
            if next >= end {
                break;
            }
            clock.set(next);
        }
        assert!((7499..=7501).contains(&frames), "{frames} frames in 600 s");
        assert!(redraws <= frames);
    }

    #[test]
    fn pacer_runs_late_frames_in_one_batch() {
        let (clock, mut pacer) = pacer(DEFAULT);
        assert_eq!(batch(&mut pacer).0, 1);
        // Two and a half frame times late: frames 1, 2 and 3 are due.
        clock.set(Duration::from_millis(80 * 3 + 40));
        let (ran, wake) = batch(&mut pacer);
        assert_eq!(ran, 3);
        assert!(wake.redraw);
        assert_eq!(wait_until(wake), Duration::from_millis(320));
        // Exactly three frame times late, four deadlines have passed: three
        // run now, the fourth at the next wake-up, which is at once.
        clock.set(Duration::from_millis(320 + 80 * 3));
        assert_eq!(batch(&mut pacer).0, 3);
        assert_eq!(pacer.next_deadline(), Some(Duration::from_millis(560)));
        assert_eq!(batch(&mut pacer).0, 1);
        assert_eq!(pacer.next_deadline(), Some(Duration::from_millis(640)));
    }

    #[test]
    fn pacer_waits_for_the_deadline() {
        let (clock, mut pacer) = pacer(DEFAULT);
        batch(&mut pacer);
        clock.set(Duration::from_millis(79));
        let (ran, wake) = batch(&mut pacer);
        assert_eq!(ran, 0);
        assert!(!wake.redraw);
        assert_eq!(wait_until(wake), DEFAULT);
    }

    #[test]
    fn pacer_stall_reanchors() {
        let (clock, mut pacer) = pacer(DEFAULT);
        for k in 0..10 {
            clock.set(DEFAULT * k);
            assert_eq!(batch(&mut pacer).0, 1);
        }
        // Blocked for 2 seconds.
        let resume = DEFAULT * 9 + Duration::from_secs(2);
        clock.set(resume);
        let (ran, wake) = batch(&mut pacer);
        assert!(ran <= MAX_DUE, "{ran} frames in a burst");
        assert_eq!(wait_until(wake), resume + DEFAULT);
        // Later frames follow at the frame time.
        for k in 1..=5 {
            clock.set(resume + DEFAULT * k);
            let (ran, wake) = batch(&mut pacer);
            assert_eq!(ran, 1);
            assert_eq!(wait_until(wake), resume + DEFAULT * (k + 1));
        }
    }

    #[test]
    fn pacer_reanchor_after_pause() {
        let (clock, mut pacer) = pacer(DEFAULT);
        batch(&mut pacer);
        clock.set(Duration::from_millis(1234));
        assert_eq!(pacer.clock().now(), Duration::from_millis(1234));
        pacer.reanchor();
        assert_eq!(pacer.next_deadline(), Some(Duration::from_millis(1234)));
    }

    #[test]
    fn pacer_speed_change() {
        let (clock, mut pacer) = pacer(DEFAULT);
        for k in 0..5 {
            clock.set(DEFAULT * k);
            batch(&mut pacer);
        }
        // Speed up during frame 4: frame 5 keeps its deadline, the frames
        // after it follow at 70 ms.
        let faster = Duration::from_millis(70);
        clock.advance(Duration::from_millis(3));
        pacer.set_frame_time(FrameTime::from_duration(faster));
        assert_eq!(pacer.frame_time(), FrameTime::from_duration(faster));
        assert_eq!(pacer.next_deadline(), Some(DEFAULT * 5));
        for k in 0..4 {
            clock.set(DEFAULT * 5 + faster * k);
            let (ran, wake) = batch(&mut pacer);
            assert_eq!(ran, 1);
            assert_eq!(wait_until(wake), DEFAULT * 5 + faster * (k + 1));
        }
        // A change in the middle of a batch ends the batch.
        clock.set(DEFAULT * 5 + faster * 6);
        pacer.begin();
        assert!(pacer.next_frame());
        pacer.set_frame_time(FrameTime::from_duration(DEFAULT));
        assert!(!pacer.next_frame());
        let wake = pacer.end();
        assert!(wake.redraw);
        assert_eq!(wait_until(wake), DEFAULT * 5 + faster * 5);
        // The same frame time changes nothing.
        pacer.set_frame_time(FrameTime::from_duration(DEFAULT));
        assert_eq!(pacer.next_deadline(), Some(DEFAULT * 5 + faster * 5));
    }

    #[test]
    fn pacer_speed_option_25_frames_per_second() {
        let (clock, mut pacer) = pacer(Duration::from_millis(40));
        let mut frames = 0;
        while clock.now() < Duration::from_secs(1) {
            let (ran, wake) = batch(&mut pacer);
            frames += ran;
            clock.set(wait_until(wake));
        }
        assert_eq!(frames, 25);
    }

    #[test]
    fn pacer_unpaced_budget() {
        let (clock, mut pacer) = pacer(Duration::ZERO);
        assert_eq!(pacer.next_deadline(), None);
        // Each frame takes 1 ms: a batch runs 8 frames.
        let mut redraws = 0;
        let mut frames = 0;
        while clock.now() < Duration::from_secs(1) {
            pacer.begin();
            let mut ran = 0;
            while pacer.next_frame() {
                clock.advance(Duration::from_millis(1));
                ran += 1;
            }
            assert_eq!(ran, 8);
            frames += ran;
            let wake = pacer.end();
            assert_eq!(wake.control, Control::Poll);
            redraws += u32::from(wake.redraw);
            // Handling events takes a little time too.
            clock.advance(Duration::from_micros(100));
        }
        assert!(frames > 900, "{frames} frames in one second");
        // At least 30 pictures a second, but not one per frame.
        assert!((30..=63).contains(&redraws), "{redraws} pictures");
        // A frame slower than the budget still runs, one per batch.
        pacer.begin();
        assert!(pacer.next_frame());
        clock.advance(Duration::from_millis(20));
        assert!(!pacer.next_frame());
        assert!(pacer.end().redraw);
    }

    #[test]
    fn pacer_switches_between_paced_and_unpaced() {
        let (clock, mut pacer) = pacer(DEFAULT);
        batch(&mut pacer);
        clock.set(Duration::from_millis(30));
        pacer.set_frame_time(FrameTime::UNPACED);
        assert_eq!(pacer.frame_time(), FrameTime::UNPACED);
        pacer.begin();
        assert!(pacer.next_frame());
        clock.advance(UNPACED_BUDGET);
        assert!(!pacer.next_frame());
        assert_eq!(pacer.end().control, Control::Poll);
        clock.set(Duration::from_millis(500));
        pacer.set_frame_time(FrameTime::from_duration(DEFAULT));
        assert_eq!(pacer.next_deadline(), Some(Duration::from_millis(500)));
        assert_eq!(batch(&mut pacer).0, 1);
    }

    #[test]
    fn system_clock_converts_deadlines() {
        let clock = SystemClock::new();
        let at = Duration::from_millis(80);
        assert_eq!(clock.instant(at) - clock.instant(Duration::ZERO), at);
        assert!(clock.now() < Duration::from_secs(60));
    }
}
