//! The field table of the state hash: every value of the stream, in
//! stream order, with its block, name, width, repetition and condition
//! (replay-verification spec, "State stream order" and the block layout
//! requirements; research 01 §8; design D5).
//!
//! The table is the single definition of the wire format. The hash walk
//! ([`StateHashFields::walk`](super::StateHashFields::walk)) emits its
//! values group by group from these arrays, the trace and the decoded dump
//! name their values from it, and the "State hash" section of
//! `docs/recording-formats.md` is generated from it.
//!
//! **Quirk P10:** the stream covers both digger slots, all six monster
//! slots and all seven bag slots whether or not they are in use, includes
//! the flags the original keeps but never changes (`fire_pressed`,
//! `level_done`), and holds the monster body part only while a slot has a
//! monster body. A reorder or a width change is a visible diff of the
//! golden table in `layout_golden.txt`.

use core::fmt::{self, Write as _};

/// The cells of the playfield and of the emerald map.
pub const CELLS: usize = 150;
/// The digger slots.
pub const DIGGER_SLOTS: usize = 2;
/// The monster slots.
pub const MONSTER_SLOTS: usize = 6;
/// The bag slots.
pub const BAG_SLOTS: usize = 7;

/// A part of the stream with its own module hash, as upstream's trace
/// hashes them separately (research 01 §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Block {
    /// The tunnel bits of the 150 cells.
    Playfield,
    /// Shared bonus and emerald state, then both digger slots.
    Digger,
    /// The spawn schedule, then the six monster slots.
    Monster,
    /// The push counter and gold lifetime, then the seven bags.
    Bag,
    /// The random-number state, both scores, the gauntlet clock and, in
    /// the round-end form, the tick count.
    Game,
}

impl Block {
    /// The blocks in stream order.
    pub const ALL: [Self; 5] = [
        Self::Playfield,
        Self::Digger,
        Self::Monster,
        Self::Bag,
        Self::Game,
    ];

    /// The block's name in the table and the trace.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Playfield => "playfield",
            Self::Digger => "digger",
            Self::Monster => "monster",
            Self::Bag => "bag",
            Self::Game => "game",
        }
    }

    /// The block's position in [`Block::ALL`].
    pub const fn index(self) -> usize {
        match self {
            Self::Playfield => 0,
            Self::Digger => 1,
            Self::Monster => 2,
            Self::Bag => 3,
            Self::Game => 4,
        }
    }
}

/// How a value is written: little-endian at a fixed width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Width {
    /// One byte, 0 to 255.
    Byte,
    /// One byte, 0 or 1.
    Bool,
    /// Two bytes, two's complement for negative values.
    Bits16,
    /// Four bytes, two's complement for negative values.
    Bits32,
}

impl Width {
    /// Bytes written.
    pub const fn bytes(self) -> usize {
        match self {
            Self::Byte | Self::Bool => 1,
            Self::Bits16 => 2,
            Self::Bits32 => 4,
        }
    }

    /// The width's name in the table.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Byte => "8-bit",
            Self::Bool => "bool",
            Self::Bits16 => "16-bit",
            Self::Bits32 => "32-bit",
        }
    }
}

/// How often an entry occurs.
///
/// [`Repeat::PerCell`] repeats the entry itself 150 times in a row. The
/// per-slot repeats apply to the run of consecutive entries with the same
/// repeat: that run is written once for slot 0, then again for slot 1, and
/// so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Repeat {
    /// Once.
    Once,
    /// Once per cell, row by row from the top-left cell.
    PerCell,
    /// Once per digger slot, slot 1 then slot 2.
    PerDigger,
    /// Once per monster slot.
    PerMonster,
    /// Once per bag slot.
    PerBag,
}

impl Repeat {
    /// Occurrences.
    pub const fn count(self) -> usize {
        match self {
            Self::Once => 1,
            Self::PerCell => CELLS,
            Self::PerDigger => DIGGER_SLOTS,
            Self::PerMonster => MONSTER_SLOTS,
            Self::PerBag => BAG_SLOTS,
        }
    }

    /// The repeat's name in the table.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Once => "once",
            Self::PerCell => "per cell",
            Self::PerDigger => "per digger",
            Self::PerMonster => "per monster",
            Self::PerBag => "per bag",
        }
    }
}

/// When an entry is present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Condition {
    /// Always.
    Always,
    /// Only for a monster slot that holds a monster body: from the tick a
    /// monster is spawned into the slot until the next round's monster
    /// reset.
    MonsterBody,
    /// Only in the gauntlet modes.
    Gauntlet,
    /// Only in the round-end form of the hash.
    RoundEnd,
}

impl Condition {
    /// The condition's name in the table.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::MonsterBody => "with monster body",
            Self::Gauntlet => "gauntlet only",
            Self::RoundEnd => "round end only",
        }
    }
}

/// One entry of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Entry {
    /// The block the value belongs to.
    pub block: Block,
    /// The value's name (the original's variable in the description).
    pub name: &'static str,
    /// How it is written.
    pub width: Width,
    /// How often it occurs.
    pub repeat: Repeat,
    /// When it is present.
    pub condition: Condition,
    /// What it is, with the original's variable name.
    pub description: &'static str,
}

/// An entry present always.
const fn e(
    block: Block,
    name: &'static str,
    width: Width,
    repeat: Repeat,
    description: &'static str,
) -> Entry {
    Entry {
        block,
        name,
        width,
        repeat,
        condition: Condition::Always,
        description,
    }
}

/// An entry present only under `condition`.
const fn when(
    block: Block,
    name: &'static str,
    width: Width,
    repeat: Repeat,
    condition: Condition,
    description: &'static str,
) -> Entry {
    Entry {
        block,
        name,
        width,
        repeat,
        condition,
        description,
    }
}

use Block::{Bag as B, Digger as D, Game as G, Monster as M, Playfield as F};
use Repeat::{Once, PerBag, PerCell, PerDigger, PerMonster};
use Width::{Bits16, Bits32, Bool, Byte};

/// Part 1: the playfield.
pub const PLAYFIELD: [Entry; 1] = [e(
    F,
    "cell",
    Bits16,
    PerCell,
    "tunnel bit set of the cell (`field`)",
)];

/// Part 2, shared values before the emerald map.
pub const DIGGER_SHARED: [Entry; 6] = [
    e(
        D,
        "bonus_start_time_left",
        Bits16,
        Once,
        "bonus-start countdown (`startbonustimeleft`)",
    ),
    e(
        D,
        "bonus_time_left",
        Bits16,
        Once,
        "bonus time left (`bonustimeleft`)",
    ),
    e(
        D,
        "emerald_mask",
        Bits16,
        Once,
        "emerald bit of the current player, 1 or 2 (`emmask`)",
    ),
    e(
        D,
        "bonus_visible",
        Bool,
        Once,
        "bonus (cherry) shown (`bonusvisible`)",
    ),
    e(D, "bonus_mode", Bool, Once, "bonus mode on (`bonusmode`)"),
    e(
        D,
        "diggers_visible",
        Bool,
        Once,
        "diggers shown (`digvisible`)",
    ),
];

/// Part 2, the emerald map.
pub const EMERALDS: [Entry; 1] = [e(
    D,
    "emeralds",
    Byte,
    PerCell,
    "emerald bits of the cell, bit 0 player 1, bit 1 player 2 (`emfield`)",
)];

/// Part 2, one digger slot.
pub const DIGGER_SLOT: [Entry; 29] = [
    e(D, "h", Bits16, PerDigger, "cell column (`h`)"),
    e(D, "v", Bits16, PerDigger, "cell row (`v`)"),
    e(D, "rx", Bits16, PerDigger, "x remainder (`rx`)"),
    e(D, "ry", Bits16, PerDigger, "y remainder (`ry`)"),
    e(
        D,
        "moved",
        Bits16,
        PerDigger,
        "direction moved this tick (`mdir`)",
    ),
    e(
        D,
        "bag_time",
        Bits16,
        PerDigger,
        "bag-push pause (`bagtime`)",
    ),
    e(
        D,
        "recharge_time",
        Bits16,
        PerDigger,
        "fireball recharge countdown (`rechargetime`)",
    ),
    e(
        D,
        "death_stage",
        Bits16,
        PerDigger,
        "death stage (`deathstage`)",
    ),
    e(
        D,
        "death_bag",
        Bits16,
        PerDigger,
        "index of the killing bag (`deathbag`)",
    ),
    e(
        D,
        "death_frame",
        Bits16,
        PerDigger,
        "death animation counter (`deathani`)",
    ),
    e(
        D,
        "death_timer",
        Bits16,
        PerDigger,
        "death timer (`deathtime`)",
    ),
    e(
        D,
        "emerald_window",
        Bits16,
        PerDigger,
        "emerald-streak window countdown (`emocttime`)",
    ),
    e(
        D,
        "emerald_run",
        Bits16,
        PerDigger,
        "emeralds in the streak (`emn`)",
    ),
    e(
        D,
        "eat_multiplier",
        Bits16,
        PerDigger,
        "monster-eating score multiplier (`msc`)",
    ),
    e(D, "lives", Bits16, PerDigger, "lives (`lives`)"),
    e(
        D,
        "invincibility",
        Bits16,
        PerDigger,
        "invincibility ticks (`ivt`)",
    ),
    e(
        D,
        "not_firing",
        Bool,
        PerDigger,
        "no fireball in play (`notfiring`)",
    ),
    e(
        D,
        "fire_pressed",
        Bool,
        PerDigger,
        "fire pressed, never set: always 0 (`firepressed`)",
    ),
    e(D, "dead", Bool, PerDigger, "death sequence over (`dead`)"),
    e(
        D,
        "level_done",
        Bool,
        PerDigger,
        "level done, never set: always 0 (`levdone`)",
    ),
    e(D, "invincible", Bool, PerDigger, "invincible (`invin`)"),
    e(D, "x", Bits16, PerDigger, "sprite x (`dob.x`)"),
    e(D, "y", Bits16, PerDigger, "sprite y (`dob.y`)"),
    e(
        D,
        "facing",
        Bits16,
        PerDigger,
        "facing direction (`dob.dir`)",
    ),
    e(
        D,
        "alive",
        Bool,
        PerDigger,
        "alive and controllable (`dob.alive`)",
    ),
    e(D, "fireball_x", Bits16, PerDigger, "fireball x (`bob.x`)"),
    e(D, "fireball_y", Bits16, PerDigger, "fireball y (`bob.y`)"),
    e(
        D,
        "fireball_direction",
        Bits16,
        PerDigger,
        "fireball direction (`bob.dir`)",
    ),
    e(
        D,
        "fireball_stage",
        Bits16,
        PerDigger,
        "fireball explosion stage (`bob.expsn`)",
    ),
];

/// Part 3, the spawn schedule.
pub const MONSTER_SCHEDULE: [Entry; 7] = [
    e(
        M,
        "next_monster",
        Bits16,
        Once,
        "monsters spawned this round (`nextmonster`)",
    ),
    e(
        M,
        "total_monsters",
        Bits16,
        Once,
        "monsters for the level (`totalmonsters`)",
    ),
    e(
        M,
        "max_on_screen",
        Bits16,
        Once,
        "most monsters at once (`maxmononscr`)",
    ),
    e(
        M,
        "next_spawn_time",
        Bits16,
        Once,
        "time to next spawn (`nextmontime`)",
    ),
    e(
        M,
        "spawn_gap",
        Bits16,
        Once,
        "gap between spawns (`mongaptime`)",
    ),
    e(
        M,
        "next_chase",
        Bits16,
        Once,
        "digger the next monster chases (`chase`)",
    ),
    e(
        M,
        "bonus_pending",
        Bool,
        Once,
        "bonus still to appear this round (`unbonusflag`)",
    ),
];

/// Part 3, one monster slot: the values present always.
pub const MONSTER_SLOT: [Entry; 13] = [
    e(M, "h", Bits16, PerMonster, "cell column (`h`)"),
    e(M, "v", Bits16, PerMonster, "cell row (`v`)"),
    e(M, "xr", Bits16, PerMonster, "x remainder (`xr`)"),
    e(M, "yr", Bits16, PerMonster, "y remainder (`yr`)"),
    e(
        M,
        "direction",
        Bits16,
        PerMonster,
        "movement direction (`dir`)",
    ),
    e(
        M,
        "delay",
        Bits16,
        PerMonster,
        "delay before next move (`t`)",
    ),
    e(
        M,
        "counter",
        Bits16,
        PerMonster,
        "nobbin crossing count / time as hobbin (`hnt`)",
    ),
    e(M, "death", Bits16, PerMonster, "death stage (`death`)"),
    e(
        M,
        "death_bag",
        Bits16,
        PerMonster,
        "squashing bag index (`bag`)",
    ),
    e(
        M,
        "death_delay",
        Bits16,
        PerMonster,
        "death delay (`dtime`)",
    ),
    e(M, "standstill", Bits16, PerMonster, "start delay (`stime`)"),
    e(
        M,
        "chase",
        Bits16,
        PerMonster,
        "digger record chased (`chase`)",
    ),
    e(M, "in_use", Bool, PerMonster, "slot in use (`flag`)"),
];

/// Part 3, one monster slot: the body values.
pub const MONSTER_BODY: [Entry; 5] = [
    when(
        M,
        "x",
        Bits16,
        PerMonster,
        Condition::MonsterBody,
        "sprite x (`pos.x`)",
    ),
    when(
        M,
        "y",
        Bits16,
        PerMonster,
        Condition::MonsterBody,
        "sprite y (`pos.y`)",
    ),
    when(
        M,
        "facing",
        Bits16,
        PerMonster,
        Condition::MonsterBody,
        "facing direction (`pos.dir`)",
    ),
    when(
        M,
        "alive",
        Bool,
        PerMonster,
        Condition::MonsterBody,
        "alive (`isalive`)",
    ),
    when(
        M,
        "nobbin",
        Bool,
        PerMonster,
        Condition::MonsterBody,
        "is a nobbin (`ISNOB`)",
    ),
];

/// Part 4, shared values.
pub const BAG_SHARED: [Entry; 2] = [
    e(B, "push_count", Bits16, Once, "push counter (`pushcount`)"),
    e(
        B,
        "gold_time",
        Bits16,
        Once,
        "gold lifetime for the level (`goldtime`)",
    ),
];

/// Part 4, one bag slot.
pub const BAG_SLOT: [Entry; 13] = [
    e(B, "x", Bits16, PerBag, "pixel x (`x`)"),
    e(B, "y", Bits16, PerBag, "pixel y (`y`)"),
    e(B, "h", Bits16, PerBag, "cell column (`h`)"),
    e(B, "v", Bits16, PerBag, "cell row (`v`)"),
    e(B, "xr", Bits16, PerBag, "x remainder (`xr`)"),
    e(B, "yr", Bits16, PerBag, "y remainder (`yr`)"),
    e(B, "direction", Bits16, PerBag, "direction (`dir`)"),
    e(B, "wobble", Bits16, PerBag, "wobble countdown (`wt`)"),
    e(B, "gold_age", Bits16, PerBag, "gold timer (`gt`)"),
    e(B, "cells_fallen", Bits16, PerBag, "cells fallen (`fallh`)"),
    e(B, "wobbling", Bool, PerBag, "wobbling (`wobbling`)"),
    e(B, "unfallen", Bool, PerBag, "not yet fallen (`unfallen`)"),
    e(B, "exists", Bool, PerBag, "exists (`exist`)"),
];

/// Parts 5 and 6: the random-number state and both total scores.
pub const GAME: [Entry; 3] = [
    e(
        G,
        "rng_state",
        Bits32,
        Once,
        "random-number state (`randv`)",
    ),
    e(
        G,
        "score_1",
        Bits32,
        Once,
        "total score of player/digger 1 (`gettscore(0)`)",
    ),
    e(
        G,
        "score_2",
        Bits32,
        Once,
        "total score of player/digger 2 (`gettscore(1)`)",
    ),
];

/// Part 7: the gauntlet clock.
pub const GAUNTLET: [Entry; 2] = [
    when(
        G,
        "gauntlet_clock",
        Bits32,
        Once,
        Condition::Gauntlet,
        "gauntlet clock, unsigned, wrapped on the time-out tick (`cgtime`)",
    ),
    when(
        G,
        "timed_out",
        Bool,
        Once,
        Condition::Gauntlet,
        "gauntlet time is out (`timeout`)",
    ),
];

/// Part 8: the tick count of the round-end form.
pub const ROUND_END: [Entry; 1] = [when(
    G,
    "tick",
    Bits32,
    Once,
    Condition::RoundEnd,
    "ticks since game start, unsigned (`ticks`)",
)];

/// The groups of the table in stream order.
pub const GROUPS: [&[Entry]; 12] = [
    &PLAYFIELD,
    &DIGGER_SHARED,
    &EMERALDS,
    &DIGGER_SLOT,
    &MONSTER_SCHEDULE,
    &MONSTER_SLOT,
    &MONSTER_BODY,
    &BAG_SHARED,
    &BAG_SLOT,
    &GAME,
    &GAUNTLET,
    &ROUND_END,
];

/// Every entry of the table in order.
pub fn entries() -> impl Iterator<Item = &'static Entry> {
    GROUPS.into_iter().flatten()
}

/// The circumstances that decide which conditional entries are present.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Shape {
    /// Monster slots holding a monster body, 0 to 6.
    pub bodies: usize,
    /// Whether the game is in a gauntlet mode.
    pub gauntlet: bool,
    /// Whether this is the round-end form.
    pub round_end: bool,
}

impl Shape {
    /// The shape of a one-player non-gauntlet game with no monster body.
    pub const PLAIN: Self = Self {
        bodies: 0,
        gauntlet: false,
        round_end: false,
    };

    /// Occurrences of `entry` in a stream of this shape.
    pub const fn occurrences(self, entry: &Entry) -> usize {
        match entry.condition {
            Condition::Always => entry.repeat.count(),
            Condition::MonsterBody => self.bodies,
            Condition::Gauntlet => self.gauntlet as usize,
            Condition::RoundEnd => self.round_end as usize,
        }
    }

    /// Bytes of `block` in a stream of this shape.
    pub fn block_len(self, block: Block) -> usize {
        entries()
            .filter(|entry| entry.block == block)
            .map(|entry| self.occurrences(entry) * entry.width.bytes())
            .sum()
    }

    /// Bytes of the whole stream of this shape.
    pub fn stream_len(self) -> usize {
        Block::ALL.into_iter().map(|b| self.block_len(b)).sum()
    }
}

/// Writes the table as plain text, one entry per line:
/// `block name width repeat condition`.
///
/// # Errors
///
/// Whatever `out` returns.
pub fn render(out: &mut impl fmt::Write) -> fmt::Result {
    for entry in entries() {
        writeln!(
            out,
            "{:<9} {:<22} {:<6} {:<11} {}",
            entry.block.name(),
            entry.name,
            entry.width.name(),
            entry.repeat.name(),
            entry.condition.name()
        )?;
    }
    Ok(())
}

/// Writes the table as a Markdown table for `docs/recording-formats.md`.
///
/// # Errors
///
/// Whatever `out` returns.
pub fn render_markdown(out: &mut impl fmt::Write) -> fmt::Result {
    writeln!(
        out,
        "| # | Block | Name | Width | Repeat | Present | Value |"
    )?;
    writeln!(out, "|---|---|---|---|---|---|---|")?;
    for (n, entry) in entries().enumerate() {
        writeln!(
            out,
            "| {} | {} | `{}` | {} | {} | {} | {} |",
            n + 1,
            entry.block.name(),
            entry.name,
            entry.width.name(),
            entry.repeat.name(),
            entry.condition.name(),
            entry.description
        )?;
    }
    writeln!(out)?;
    writeln!(out, "| Block | Bytes |")?;
    writeln!(out, "|---|---|")?;
    for block in Block::ALL {
        let least = Shape::PLAIN.block_len(block);
        let most = Shape {
            bodies: MONSTER_SLOTS,
            gauntlet: true,
            round_end: true,
        }
        .block_len(block);
        let mut bytes = String::new();
        if least == most {
            write!(bytes, "{least}")?;
        } else {
            write!(bytes, "{least} to {most}")?;
        }
        writeln!(out, "| {} | {bytes} |", block.name())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The checked-in rendering of the table.
    const GOLDEN: &str = include_str!("layout_golden.txt");

    #[test]
    fn layout_golden() {
        let mut text = String::new();
        render(&mut text).unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
        assert_eq!(text, GOLDEN, "the state hash layout changed");

        assert_eq!(Shape::PLAIN.block_len(Block::Playfield), 300);
        assert_eq!(Shape::PLAIN.block_len(Block::Digger), 263);
        assert_eq!(Shape::PLAIN.block_len(Block::Monster), 163);
        let all_bodies = Shape {
            bodies: MONSTER_SLOTS,
            ..Shape::PLAIN
        };
        assert_eq!(all_bodies.block_len(Block::Monster), 211);
        assert_eq!(Shape::PLAIN.block_len(Block::Bag), 165);
        assert_eq!(Shape::PLAIN.block_len(Block::Game), 12);

        assert_eq!(Shape::PLAIN.stream_len(), 903);
        let gauntlet = Shape {
            gauntlet: true,
            ..Shape::PLAIN
        };
        assert_eq!(gauntlet.stream_len(), 908);
        assert_eq!(all_bodies.stream_len(), 951);
        let round_end = Shape {
            round_end: true,
            ..Shape::PLAIN
        };
        assert_eq!(round_end.stream_len(), 907);
    }

    /// The document holding the generated table.
    const DOC_PATH: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/recording-formats.md"
    );
    /// The markers around the generated table.
    const BEGIN: &str = "<!-- state-hash-table:begin -->\n";
    /// See [`BEGIN`].
    const END: &str = "<!-- state-hash-table:end -->\n";

    #[test]
    #[expect(clippy::panic, reason = "a stale document is a test failure")]
    fn layout_doc_is_current() {
        let doc = std::fs::read_to_string(DOC_PATH)
            .unwrap_or_else(|error| panic!("reading {DOC_PATH}: {error}"));
        let (before, rest) = doc
            .split_once(BEGIN)
            .unwrap_or_else(|| panic!("{DOC_PATH} has no {BEGIN:?}"));
        let (current, after) = rest
            .split_once(END)
            .unwrap_or_else(|| panic!("{DOC_PATH} has no {END:?}"));
        let mut table = String::new();
        render_markdown(&mut table)
            .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
        if std::env::var_os("DIGGER_UPDATE_DOCS").is_some() {
            let updated = format!("{before}{BEGIN}{table}{END}{after}");
            std::fs::write(DOC_PATH, updated)
                .unwrap_or_else(|error| panic!("writing {DOC_PATH}: {error}"));
            return;
        }
        assert_eq!(
            current, table,
            "the State hash table in docs/recording-formats.md is stale; \
             rerun this test with DIGGER_UPDATE_DOCS=1"
        );
    }

    #[test]
    fn digger_and_monster_parts_match_the_spec_tables() {
        let bytes = |group: &[Entry]| -> usize { group.iter().map(|e| e.width.bytes()).sum() };
        // Shared part: 3 × 16-bit, 3 flags, 150 emerald bytes.
        assert_eq!(bytes(&DIGGER_SHARED) + CELLS, 159);
        assert_eq!(bytes(&DIGGER_SLOT), 52);
        assert_eq!(bytes(&MONSTER_SCHEDULE), 13);
        assert_eq!(bytes(&MONSTER_SLOT), 25);
        assert_eq!(bytes(&MONSTER_BODY), 8);
        assert_eq!(bytes(&BAG_SHARED), 4);
        assert_eq!(bytes(&BAG_SLOT), 23);
    }

    #[test]
    fn entries_are_grouped_by_block_in_stream_order() {
        let blocks: Vec<Block> = entries().map(|entry| entry.block).collect();
        assert!(blocks.is_sorted(), "a block's entries are contiguous");
        for group in GROUPS {
            let first = group.first().map(|entry| (entry.repeat, entry.condition));
            assert!(
                group
                    .iter()
                    .all(|entry| Some((entry.repeat, entry.condition)) == first),
                "one repeat and one condition per group"
            );
        }
    }
}
