//! Per-tick trace lines and the decoded state dump for divergence triage
//! (replay-verification spec, "Per-tick state trace for triage"; design D5
//! and D12 of `add-recording-and-conformance`).
//!
//! A trace line is one tick's summary of the state the hash covers:
//!
//! ```text
//! tick=256 hash=17B2FBDB playfield=0123ABCD/300 digger=…/263 monster=…/163 bag=…/165 game=…/12 rng=-5 score=0 score2=0
//! ```
//!
//! `hash` is the plain state hash (the value a `C` checkpoint of that tick
//! holds); each block is shown as its own hash and its byte length, the
//! length telling at a glance when only the variable-length monster block
//! changed shape. With the decoded dump requested, every hashed value
//! follows its tick line on a line of its own, indented by two spaces:
//! `  monster.x[2]=104`. The format is read back by [`TraceLine::parse`]
//! and [`TraceField::parse`], which `cargo xtask trace-diff` is built on.

use core::fmt::{self, Write as _};

use crate::state_hash::layout::Block;
use crate::state_hash::{BlockHashes, StateHashFields, decode, format_hash};
use crate::{Observer, StateView};

/// Which ticks a [`TraceSink`] writes, and how much of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceOptions {
    /// The first tick traced (tick counts start at 1 for the first tick).
    pub from: u32,
    /// The last tick traced, inclusive.
    pub to: u32,
    /// Whether every hashed value follows its tick line.
    pub fields: bool,
}

impl TraceOptions {
    /// Every tick from `from` to `to` inclusive, without the decoded dump.
    pub const fn ticks(from: u32, to: u32) -> Self {
        Self {
            from,
            to,
            fields: false,
        }
    }

    /// Whether tick `tick` is traced.
    pub const fn covers(&self, tick: u32) -> bool {
        self.from <= tick && tick <= self.to
    }
}

/// One tick of the trace: the full hash, the hash and length of each
/// block, the random-number state and both scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TraceLine {
    /// The tick count including the tick.
    pub tick: u32,
    /// The plain state hash after the tick.
    pub hash: u32,
    /// The hash and byte length of each block.
    pub blocks: BlockHashes,
    /// The random-number state.
    pub rng_state: i32,
    /// The total scores of player/digger 1 and 2.
    pub scores: [i32; 2],
}

impl TraceLine {
    /// The trace line of `fields` after tick `tick`.
    pub fn of(tick: u32, fields: &StateHashFields) -> Self {
        Self {
            tick,
            hash: fields.hash(),
            blocks: fields.block_hashes(None),
            rng_state: fields.rng_state,
            scores: fields.scores,
        }
    }

    /// Reads a line written by [`Display`](fmt::Display); `None` for
    /// anything else (a field line, a comment, a malformed line).
    pub fn parse(text: &str) -> Option<Self> {
        let mut words = text.split(' ');
        let tick = words.next()?.strip_prefix("tick=")?.parse().ok()?;
        let hash = hex(words.next()?.strip_prefix("hash=")?)?;
        let mut blocks = BlockHashes {
            hashes: [0; 5],
            lengths: [0; 5],
        };
        for block in Block::ALL {
            let (name, value) = words.next()?.split_once('=')?;
            if name != block.name() {
                return None;
            }
            let (block_hash, length) = value.split_once('/')?;
            *blocks.hashes.get_mut(block.index())? = hex(block_hash)?;
            *blocks.lengths.get_mut(block.index())? = length.parse().ok()?;
        }
        let rng_state = words.next()?.strip_prefix("rng=")?.parse().ok()?;
        let score = words.next()?.strip_prefix("score=")?.parse().ok()?;
        let score2 = words.next()?.strip_prefix("score2=")?.parse().ok()?;
        if words.next().is_some() {
            return None;
        }
        Some(Self {
            tick,
            hash,
            blocks,
            rng_state,
            scores: [score, score2],
        })
    }

    /// The first block, in stream order, whose hash or length differs
    /// between `self` and `other`.
    pub fn first_differing_block(&self, other: &Self) -> Option<Block> {
        Block::ALL.into_iter().find(|&block| {
            self.blocks.hash(block) != other.blocks.hash(block)
                || self.blocks.len(block) != other.blocks.len(block)
        })
    }

    /// The block's part of the line, `name=HASH/length`.
    pub fn block_text(&self, block: Block) -> String {
        format!(
            "{}={}/{}",
            block.name(),
            format_hash(self.blocks.hash(block)),
            self.blocks.len(block)
        )
    }
}

impl fmt::Display for TraceLine {
    /// The trace line, without a line end.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "tick={} hash={:08X}", self.tick, self.hash)?;
        for block in Block::ALL {
            write!(f, " {}", self.block_text(block))?;
        }
        let [score, score2] = self.scores;
        write!(f, " rng={} score={score} score2={score2}", self.rng_state)
    }
}

/// One line of the decoded dump: a hashed value's full name and its value
/// as text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TraceField {
    /// `block.name` or `block.name[index]`.
    pub name: String,
    /// The value as the dump prints it.
    pub value: String,
}

impl TraceField {
    /// The indentation that marks a field line.
    pub const INDENT: &'static str = "  ";

    /// Reads a field line (`  name=value`); `None` for anything else.
    pub fn parse(text: &str) -> Option<Self> {
        let (name, value) = text.strip_prefix(Self::INDENT)?.split_once('=')?;
        Some(Self {
            name: name.to_owned(),
            value: value.to_owned(),
        })
    }
}

/// An eight-digit hexadecimal hash.
fn hex(text: &str) -> Option<u32> {
    if text.len() != 8 {
        return None;
    }
    u32::from_str_radix(text, 16).ok()
}

/// Writes one trace line per tick in its range, as text drained by the
/// caller (see [`take_pending`](Self::take_pending)). It only observes:
/// [`ReplayObserver`](super::ReplayObserver) tells it the state after each
/// tick and ignores what it would return, so tracing never changes a run.
#[derive(Debug, Clone)]
pub struct TraceSink {
    options: TraceOptions,
    pending: String,
    lines: u32,
}

impl TraceSink {
    /// A sink tracing the ticks `options` names.
    pub const fn new(options: TraceOptions) -> Self {
        Self {
            options,
            pending: String::new(),
            lines: 0,
        }
    }

    /// What the sink traces.
    pub const fn options(&self) -> TraceOptions {
        self.options
    }

    /// The number of tick lines written so far.
    pub const fn lines(&self) -> u32 {
        self.lines
    }

    /// Removes and returns the text written since the last call, each line
    /// ending in LF.
    pub fn take_pending(&mut self) -> String {
        core::mem::take(&mut self.pending)
    }

    /// Writes the lines of tick `tick` for `fields`.
    fn write_tick(&mut self, tick: u32, fields: &StateHashFields) {
        let _ = writeln!(self.pending, "{}", TraceLine::of(tick, fields));
        if self.options.fields {
            for value in decode(fields, None) {
                let _ = writeln!(self.pending, "{}{value}", TraceField::INDENT);
            }
        }
        self.lines = self.lines.saturating_add(1);
    }
}

impl Observer for TraceSink {
    fn tick_end(&mut self, tick: u32, state: StateView<'_>) {
        if self.options.covers(tick) {
            self.write_tick(tick, &StateHashFields::from_view(state));
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    reason = "a failed expectation in a test is a test failure"
)]
mod tests {
    use super::*;

    fn line(tick: u32) -> TraceLine {
        TraceLine::of(tick, &StateHashFields::default())
    }

    #[test]
    fn trace_line_round_trips() {
        let mut original = line(700);
        original.rng_state = -12345;
        original.scores = [3100, 25];
        let text = original.to_string();
        assert!(text.starts_with("tick=700 hash="), "{text}");
        assert!(text.contains(" monster="), "{text}");
        assert!(text.ends_with(" rng=-12345 score=3100 score2=25"), "{text}");
        assert_eq!(TraceLine::parse(&text), Some(original));
        assert_eq!(TraceLine::parse("  digger.lives[0]=3"), None);
        assert_eq!(TraceLine::parse(&format!("{text} extra")), None);
    }

    #[test]
    fn default_state_block_lengths() {
        let line = line(1);
        let lengths: Vec<usize> = Block::ALL.into_iter().map(|b| line.blocks.len(b)).collect();
        // 903 bytes with no monster body: 300 + 263 + 163 + 165 + 12.
        assert_eq!(lengths, [300, 263, 163, 165, 12]);
    }

    #[test]
    fn first_differing_block_is_in_stream_order() {
        let a = line(700);
        let mut b = a;
        assert_eq!(a.first_differing_block(&b), None);
        b.blocks.hashes[3] ^= 1;
        assert_eq!(a.first_differing_block(&b), Some(Block::Bag));
        b.blocks.lengths[2] += 5;
        assert_eq!(a.first_differing_block(&b), Some(Block::Monster));
    }

    #[test]
    fn field_lines() {
        let field = TraceField::parse("  monster.x[2]=104").expect("a field line");
        assert_eq!(field.name, "monster.x[2]");
        assert_eq!(field.value, "104");
        assert_eq!(TraceField::parse("tick=1 hash=00000000"), None);
    }

    #[test]
    fn sink_traces_only_its_range_with_fields() {
        let fields = StateHashFields::default();
        let mut sink = TraceSink::new(TraceOptions {
            from: 2,
            to: 3,
            fields: true,
        });
        for tick in 1..=5 {
            if sink.options().covers(tick) {
                sink.write_tick(tick, &fields);
            }
        }
        assert_eq!(sink.lines(), 2);
        let text = sink.take_pending();
        let ticks: Vec<u32> = text
            .lines()
            .filter_map(TraceLine::parse)
            .map(|l| l.tick)
            .collect();
        assert_eq!(ticks, [2, 3]);
        let field_lines = text.lines().filter_map(TraceField::parse).count();
        assert_eq!(field_lines, 2 * decode(&fields, None).len());
        assert_eq!(sink.take_pending(), "");
    }
}
