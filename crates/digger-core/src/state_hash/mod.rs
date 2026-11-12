//! The state hash: an exact byte stream over the simulation state, digested
//! with MD5 and cut to the first four digest bytes, as the checkpoints of
//! upstream's eDRF recordings hold it (replay-verification spec; design D5
//! of `add-recording-and-conformance`).
//!
//! The hash is a file format in its own right. It is defined by one ordered
//! field table ([`layout`]), independent of how the simulation lays out its
//! structs. The state is first turned into plain data, a
//! [`StateHashFields`] value whose every enum is already converted to its
//! wire code ([`codes`]); [`StateHashFields::from_view`] builds it from the
//! read-only [`StateView`](crate::StateView). A single walk
//! ([`StateHashFields::walk`]) then visits every value in table order, and
//! the visitors here turn the walk into the hash ([`StateHashFields::hash`],
//! [`StateHashFields::hash_at_round_end`]), per-block hashes and lengths
//! ([`BlockHashes`]) or a decoded list of values ([`decode`]).

pub mod codes;
pub mod layout;
mod view;

use core::fmt;

use md5::{Digest as _, Md5};

use layout::{BAG_SLOTS, Block, CELLS, DIGGER_SLOTS, Entry, MONSTER_SLOTS, Width};

/// One value of the stream, at its width.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Value {
    /// An 8-bit value.
    Byte(u8),
    /// A yes/no value, written as 0 or 1.
    Bool(bool),
    /// A signed 16-bit value.
    I16(i16),
    /// A 16-bit bit set (the playfield cells), written like an `i16`.
    U16(u16),
    /// A signed 32-bit value.
    I32(i32),
    /// An unsigned 32-bit value.
    U32(u32),
}

impl Value {
    /// The width the value is written at.
    pub const fn width(self) -> Width {
        match self {
            Self::Byte(_) => Width::Byte,
            Self::Bool(_) => Width::Bool,
            Self::I16(_) | Self::U16(_) => Width::Bits16,
            Self::I32(_) | Self::U32(_) => Width::Bits32,
        }
    }

    /// Passes the value's little-endian bytes to `out`.
    pub fn write_le(self, out: &mut impl FnMut(&[u8])) {
        match self {
            Self::Byte(v) => out(&[v]),
            Self::Bool(v) => out(&[u8::from(v)]),
            Self::I16(v) => out(&v.to_le_bytes()),
            Self::U16(v) => out(&v.to_le_bytes()),
            Self::I32(v) => out(&v.to_le_bytes()),
            Self::U32(v) => out(&v.to_le_bytes()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Byte(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{}", u8::from(v)),
            Self::I16(v) => write!(f, "{v}"),
            Self::U16(v) => write!(f, "0x{v:04X}"),
            Self::I32(v) => write!(f, "{v}"),
            Self::U32(v) => write!(f, "{v}"),
        }
    }
}

/// Receives the values of the stream in table order.
pub trait Visitor {
    /// One value: its table entry, its slot or cell index for a repeated
    /// entry (`None` for a value that occurs once), and the value.
    fn visit(&mut self, entry: &'static Entry, index: Option<usize>, value: Value);
}

/// The shared values of the digger block (part 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct DiggerSharedFields {
    /// Bonus-start countdown (`startbonustimeleft`).
    pub bonus_start_time_left: i16,
    /// Bonus time left (`bonustimeleft`).
    pub bonus_time_left: i16,
    /// The current player's emerald bit, 1 or 2 (`emmask`).
    pub emerald_mask: i16,
    /// Whether the bonus is shown (`bonusvisible`).
    pub bonus_visible: bool,
    /// Whether bonus mode is on (`bonusmode`).
    pub bonus_mode: bool,
    /// Whether the diggers are shown (`digvisible`).
    pub diggers_visible: bool,
}

impl DiggerSharedFields {
    /// The values in table order ([`layout::DIGGER_SHARED`]).
    pub const fn values(&self) -> [Value; 6] {
        [
            Value::I16(self.bonus_start_time_left),
            Value::I16(self.bonus_time_left),
            Value::I16(self.emerald_mask),
            Value::Bool(self.bonus_visible),
            Value::Bool(self.bonus_mode),
            Value::Bool(self.diggers_visible),
        ]
    }
}

/// One digger slot of the digger block (part 2). Directions and stages
/// are wire codes ([`codes`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent flags of the state hash, at the original's granularity (design D5)"
)]
pub struct DiggerHashFields {
    /// Cell column (`h`).
    pub h: i16,
    /// Cell row (`v`).
    pub v: i16,
    /// X remainder (`rx`).
    pub rx: i16,
    /// Y remainder (`ry`).
    pub ry: i16,
    /// Direction moved this tick (`mdir`).
    pub moved: i16,
    /// Bag-push pause (`bagtime`).
    pub bag_time: i16,
    /// Fireball recharge countdown (`rechargetime`).
    pub recharge_time: i16,
    /// Death stage (`deathstage`).
    pub death_stage: i16,
    /// Index of the killing bag (`deathbag`).
    pub death_bag: i16,
    /// Death animation counter (`deathani`).
    pub death_frame: i16,
    /// Death timer (`deathtime`).
    pub death_timer: i16,
    /// Emerald-streak window countdown (`emocttime`).
    pub emerald_window: i16,
    /// Emeralds in the streak (`emn`).
    pub emerald_run: i16,
    /// Monster-eating score multiplier (`msc`).
    pub eat_multiplier: i16,
    /// Lives (`lives`).
    pub lives: i16,
    /// Invincibility ticks (`ivt`).
    pub invincibility: i16,
    /// No fireball in play (`notfiring`).
    pub not_firing: bool,
    /// **Quirk P10:** the original's `firepressed`, kept but never set, so
    /// always false; still hashed.
    pub fire_pressed: bool,
    /// The death sequence is over (`dead`).
    pub dead: bool,
    /// **Quirk P10:** the original's per-digger `levdone`, kept but never
    /// set, so always false; still hashed.
    pub level_done: bool,
    /// Invincible (`invin`).
    pub invincible: bool,
    /// Sprite x (`dob.x`).
    pub x: i16,
    /// Sprite y (`dob.y`).
    pub y: i16,
    /// Facing direction (`dob.dir`).
    pub facing: i16,
    /// Alive and controllable (`dob.alive`).
    pub alive: bool,
    /// Fireball x (`bob.x`).
    pub fireball_x: i16,
    /// Fireball y (`bob.y`).
    pub fireball_y: i16,
    /// Fireball direction (`bob.dir`).
    pub fireball_direction: i16,
    /// Fireball explosion stage (`bob.expsn`).
    pub fireball_stage: i16,
}

impl DiggerHashFields {
    /// The values in table order ([`layout::DIGGER_SLOT`]).
    pub const fn values(&self) -> [Value; 29] {
        [
            Value::I16(self.h),
            Value::I16(self.v),
            Value::I16(self.rx),
            Value::I16(self.ry),
            Value::I16(self.moved),
            Value::I16(self.bag_time),
            Value::I16(self.recharge_time),
            Value::I16(self.death_stage),
            Value::I16(self.death_bag),
            Value::I16(self.death_frame),
            Value::I16(self.death_timer),
            Value::I16(self.emerald_window),
            Value::I16(self.emerald_run),
            Value::I16(self.eat_multiplier),
            Value::I16(self.lives),
            Value::I16(self.invincibility),
            Value::Bool(self.not_firing),
            Value::Bool(self.fire_pressed),
            Value::Bool(self.dead),
            Value::Bool(self.level_done),
            Value::Bool(self.invincible),
            Value::I16(self.x),
            Value::I16(self.y),
            Value::I16(self.facing),
            Value::Bool(self.alive),
            Value::I16(self.fireball_x),
            Value::I16(self.fireball_y),
            Value::I16(self.fireball_direction),
            Value::I16(self.fireball_stage),
        ]
    }
}

/// The digger block (part 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DiggerBlockFields {
    /// The shared values.
    pub shared: DiggerSharedFields,
    /// The emerald map, one byte per cell row by row (`emfield`).
    pub emeralds: [u8; CELLS],
    /// Both digger slots, whether or not the game uses the second.
    pub slots: [DiggerHashFields; DIGGER_SLOTS],
}

impl Default for DiggerBlockFields {
    fn default() -> Self {
        Self {
            shared: DiggerSharedFields::default(),
            emeralds: [0; CELLS],
            slots: [DiggerHashFields::default(); DIGGER_SLOTS],
        }
    }
}

/// The spawn schedule of the monster block (part 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MonsterScheduleFields {
    /// Monsters spawned this round (`nextmonster`).
    pub next_monster: i16,
    /// Monsters for the level (`totalmonsters`).
    pub total_monsters: i16,
    /// Most monsters on screen at once (`maxmononscr`).
    pub max_on_screen: i16,
    /// Time to next spawn (`nextmontime`).
    pub next_spawn_time: i16,
    /// Gap between spawns (`mongaptime`).
    pub spawn_gap: i16,
    /// The digger the next monster chases (`chase`).
    pub next_chase: i16,
    /// The bonus is still to appear this round (`unbonusflag`).
    pub bonus_pending: bool,
}

impl MonsterScheduleFields {
    /// The values in table order ([`layout::MONSTER_SCHEDULE`]).
    pub const fn values(&self) -> [Value; 7] {
        [
            Value::I16(self.next_monster),
            Value::I16(self.total_monsters),
            Value::I16(self.max_on_screen),
            Value::I16(self.next_spawn_time),
            Value::I16(self.spawn_gap),
            Value::I16(self.next_chase),
            Value::Bool(self.bonus_pending),
        ]
    }
}

/// The monster body of a slot: present from the tick a monster is spawned
/// into the slot until the next round's monster reset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MonsterBodyFields {
    /// Sprite x (`pos.x`).
    pub x: i16,
    /// Sprite y (`pos.y`).
    pub y: i16,
    /// Facing direction (`pos.dir`).
    pub facing: i16,
    /// Alive (`isalive`).
    pub alive: bool,
    /// A nobbin rather than a hobbin (`ISNOB`).
    pub nobbin: bool,
}

impl MonsterBodyFields {
    /// The values in table order ([`layout::MONSTER_BODY`]).
    pub const fn values(&self) -> [Value; 5] {
        [
            Value::I16(self.x),
            Value::I16(self.y),
            Value::I16(self.facing),
            Value::Bool(self.alive),
            Value::Bool(self.nobbin),
        ]
    }
}

/// One monster slot of the monster block (part 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MonsterHashFields {
    /// Cell column (`h`).
    pub h: i16,
    /// Cell row (`v`).
    pub v: i16,
    /// X remainder (`xr`).
    pub xr: i16,
    /// Y remainder (`yr`).
    pub yr: i16,
    /// Movement direction (`dir`).
    pub direction: i16,
    /// Delay before the next move (`t`).
    pub delay: i16,
    /// Nobbin crossing count / time as hobbin (`hnt`).
    pub counter: i16,
    /// Death stage (`death`).
    pub death: i16,
    /// Index of the squashing bag (`bag`).
    pub death_bag: i16,
    /// Death delay (`dtime`).
    pub death_delay: i16,
    /// Start delay (`stime`).
    pub standstill: i16,
    /// The digger record chased (`chase`).
    pub chase: i16,
    /// The slot is in use (`flag`).
    pub in_use: bool,
    /// The monster body, while the slot holds one (`mop != NULL`).
    pub body: Option<MonsterBodyFields>,
}

impl MonsterHashFields {
    /// The values always present, in table order
    /// ([`layout::MONSTER_SLOT`]).
    pub const fn values(&self) -> [Value; 13] {
        [
            Value::I16(self.h),
            Value::I16(self.v),
            Value::I16(self.xr),
            Value::I16(self.yr),
            Value::I16(self.direction),
            Value::I16(self.delay),
            Value::I16(self.counter),
            Value::I16(self.death),
            Value::I16(self.death_bag),
            Value::I16(self.death_delay),
            Value::I16(self.standstill),
            Value::I16(self.chase),
            Value::Bool(self.in_use),
        ]
    }
}

/// The monster block (part 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MonsterBlockFields {
    /// The spawn schedule.
    pub schedule: MonsterScheduleFields,
    /// The six slots.
    pub slots: [MonsterHashFields; MONSTER_SLOTS],
}

/// The shared values of the bag block (part 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BagSharedFields {
    /// Push counter (`pushcount`).
    pub push_count: i16,
    /// Gold lifetime for the level (`goldtime`).
    pub gold_time: i16,
}

impl BagSharedFields {
    /// The values in table order ([`layout::BAG_SHARED`]).
    pub const fn values(&self) -> [Value; 2] {
        [Value::I16(self.push_count), Value::I16(self.gold_time)]
    }
}

/// One bag slot of the bag block (part 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BagHashFields {
    /// Pixel x (`x`).
    pub x: i16,
    /// Pixel y (`y`).
    pub y: i16,
    /// Cell column (`h`).
    pub h: i16,
    /// Cell row (`v`).
    pub v: i16,
    /// X remainder (`xr`).
    pub xr: i16,
    /// Y remainder (`yr`).
    pub yr: i16,
    /// Direction (`dir`).
    pub direction: i16,
    /// Wobble countdown (`wt`).
    pub wobble: i16,
    /// Gold timer (`gt`).
    pub gold_age: i16,
    /// Cells fallen (`fallh`).
    pub cells_fallen: i16,
    /// Wobbling (`wobbling`).
    pub wobbling: bool,
    /// Not yet fallen (`unfallen`).
    pub unfallen: bool,
    /// Exists (`exist`).
    pub exists: bool,
}

impl BagHashFields {
    /// The values in table order ([`layout::BAG_SLOT`]).
    pub const fn values(&self) -> [Value; 13] {
        [
            Value::I16(self.x),
            Value::I16(self.y),
            Value::I16(self.h),
            Value::I16(self.v),
            Value::I16(self.xr),
            Value::I16(self.yr),
            Value::I16(self.direction),
            Value::I16(self.wobble),
            Value::I16(self.gold_age),
            Value::I16(self.cells_fallen),
            Value::Bool(self.wobbling),
            Value::Bool(self.unfallen),
            Value::Bool(self.exists),
        ]
    }
}

/// The bag block (part 4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct BagBlockFields {
    /// The shared values.
    pub shared: BagSharedFields,
    /// The seven bag slots, whether or not each bag exists.
    pub slots: [BagHashFields; BAG_SLOTS],
}

/// The gauntlet values (part 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GauntletHashFields {
    /// The clock, as its unsigned 32-bit value (`cgtime`).
    pub clock: u32,
    /// The time is out (`timeout`).
    pub timed_out: bool,
}

/// Every value the state hash covers, as plain data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StateHashFields {
    /// The tunnel bits of each cell, row by row (`field`).
    pub playfield: [u16; CELLS],
    /// The digger block.
    pub digger: DiggerBlockFields,
    /// The monster block.
    pub monster: MonsterBlockFields,
    /// The bag block.
    pub bag: BagBlockFields,
    /// The random-number state (`randv`).
    pub rng_state: i32,
    /// The total scores of player/digger 1 and 2 (`gettscore`).
    pub scores: [i32; 2],
    /// The gauntlet values, in the gauntlet modes only.
    pub gauntlet: Option<GauntletHashFields>,
}

impl Default for StateHashFields {
    fn default() -> Self {
        Self {
            playfield: [0; CELLS],
            digger: DiggerBlockFields::default(),
            monster: MonsterBlockFields::default(),
            bag: BagBlockFields::default(),
            rng_state: 0,
            scores: [0; 2],
            gauntlet: None,
        }
    }
}

/// Visits `values` against the entries of `group`, with `index`.
fn emit<const N: usize>(
    group: &'static [Entry; N],
    index: Option<usize>,
    values: [Value; N],
    visitor: &mut impl Visitor,
) {
    for (entry, value) in group.iter().zip(values) {
        visitor.visit(entry, index, value);
    }
}

/// Visits a per-cell entry with one value per cell.
fn emit_cells(
    group: &'static [Entry; 1],
    values: impl Iterator<Item = Value>,
    visitor: &mut impl Visitor,
) {
    let [entry] = group;
    for (cell, value) in values.enumerate() {
        visitor.visit(entry, Some(cell), value);
    }
}

impl StateHashFields {
    /// Visits every value of the stream in table order. With `tick`, the
    /// round-end form: the tick count follows everything else.
    pub fn walk(&self, tick: Option<u32>, visitor: &mut impl Visitor) {
        emit_cells(
            &layout::PLAYFIELD,
            self.playfield.iter().map(|&bits| Value::U16(bits)),
            visitor,
        );

        emit(
            &layout::DIGGER_SHARED,
            None,
            self.digger.shared.values(),
            visitor,
        );
        emit_cells(
            &layout::EMERALDS,
            self.digger.emeralds.iter().map(|&bits| Value::Byte(bits)),
            visitor,
        );
        for (n, slot) in self.digger.slots.iter().enumerate() {
            emit(&layout::DIGGER_SLOT, Some(n), slot.values(), visitor);
        }

        emit(
            &layout::MONSTER_SCHEDULE,
            None,
            self.monster.schedule.values(),
            visitor,
        );
        for (n, slot) in self.monster.slots.iter().enumerate() {
            emit(&layout::MONSTER_SLOT, Some(n), slot.values(), visitor);
            if let Some(body) = &slot.body {
                emit(&layout::MONSTER_BODY, Some(n), body.values(), visitor);
            }
        }

        emit(&layout::BAG_SHARED, None, self.bag.shared.values(), visitor);
        for (n, slot) in self.bag.slots.iter().enumerate() {
            emit(&layout::BAG_SLOT, Some(n), slot.values(), visitor);
        }

        let [score_1, score_2] = self.scores;
        emit(
            &layout::GAME,
            None,
            [
                Value::I32(self.rng_state),
                Value::I32(score_1),
                Value::I32(score_2),
            ],
            visitor,
        );
        if let Some(gauntlet) = &self.gauntlet {
            emit(
                &layout::GAUNTLET,
                None,
                [Value::U32(gauntlet.clock), Value::Bool(gauntlet.timed_out)],
                visitor,
            );
        }
        if let Some(tick) = tick {
            emit(&layout::ROUND_END, None, [Value::U32(tick)], visitor);
        }
    }

    /// The state hash: the first four bytes of the MD5 digest of the
    /// stream, first byte most significant.
    pub fn hash(&self) -> u32 {
        let mut sink = Md5Sink::default();
        self.walk(None, &mut sink);
        sink.finish()
    }

    /// The round-end form of the hash: the stream followed by `tick`.
    pub fn hash_at_round_end(&self, tick: u32) -> u32 {
        let mut sink = Md5Sink::default();
        self.walk(Some(tick), &mut sink);
        sink.finish()
    }

    /// The stream's bytes (with `tick`, of the round-end form).
    pub fn stream(&self, tick: Option<u32>) -> Vec<u8> {
        let mut sink = ByteSink::default();
        self.walk(tick, &mut sink);
        sink.0
    }

    /// The hash and byte length of each block (with `tick`, of the
    /// round-end form).
    pub fn block_hashes(&self, tick: Option<u32>) -> BlockHashes {
        let mut sink = BlockSink::default();
        self.walk(tick, &mut sink);
        sink.finish()
    }
}

/// The first four digest bytes as one number, first byte most significant.
fn digest_prefix(hasher: Md5) -> u32 {
    let digest = hasher.finalize();
    let mut prefix = [0; 4];
    for (out, byte) in prefix.iter_mut().zip(digest.iter()) {
        *out = *byte;
    }
    u32::from_be_bytes(prefix)
}

/// Feeds the stream to MD5.
#[derive(Default)]
struct Md5Sink(Md5);

impl fmt::Debug for Md5Sink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Md5Sink")
    }
}

impl Md5Sink {
    /// The hash of everything visited.
    fn finish(self) -> u32 {
        digest_prefix(self.0)
    }
}

impl Visitor for Md5Sink {
    fn visit(&mut self, _: &'static Entry, _: Option<usize>, value: Value) {
        value.write_le(&mut |bytes| self.0.update(bytes));
    }
}

/// Collects the stream's bytes.
#[derive(Debug, Default)]
struct ByteSink(Vec<u8>);

impl Visitor for ByteSink {
    fn visit(&mut self, _: &'static Entry, _: Option<usize>, value: Value) {
        value.write_le(&mut |bytes| self.0.extend_from_slice(bytes));
    }
}

/// The module hash and the byte length of each block, as the trace lines
/// show them (design D5, D12).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockHashes {
    /// The hash of each block's own stream, in [`Block::ALL`] order.
    pub hashes: [u32; 5],
    /// The byte length of each block's stream, in [`Block::ALL`] order.
    pub lengths: [usize; 5],
}

impl BlockHashes {
    /// The hash of `block`'s stream.
    pub fn hash(&self, block: Block) -> u32 {
        self.hashes.get(block.index()).copied().unwrap_or_default()
    }

    /// The byte length of `block`'s stream.
    pub fn len(&self, block: Block) -> usize {
        self.lengths.get(block.index()).copied().unwrap_or_default()
    }
}

/// Feeds each block to its own MD5 and counts its bytes.
#[derive(Default)]
struct BlockSink {
    /// One hasher per block.
    hashers: [Md5; 5],
    /// Bytes per block.
    lengths: [usize; 5],
}

impl fmt::Debug for BlockSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockSink")
            .field("lengths", &self.lengths)
            .finish_non_exhaustive()
    }
}

impl BlockSink {
    /// The hashes and lengths of everything visited.
    fn finish(self) -> BlockHashes {
        BlockHashes {
            hashes: self.hashers.map(digest_prefix),
            lengths: self.lengths,
        }
    }
}

impl Visitor for BlockSink {
    fn visit(&mut self, entry: &'static Entry, _: Option<usize>, value: Value) {
        let index = entry.block.index();
        if let (Some(hasher), Some(length)) =
            (self.hashers.get_mut(index), self.lengths.get_mut(index))
        {
            value.write_le(&mut |bytes| hasher.update(bytes));
            *length += value.width().bytes();
        }
    }
}

/// One value of the decoded dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DecodedValue {
    /// The table entry.
    pub entry: &'static Entry,
    /// The slot or cell index of a repeated entry.
    pub index: Option<usize>,
    /// The value.
    pub value: Value,
}

impl DecodedValue {
    /// The value's full name: `block.name` or `block.name[index]`.
    pub fn name(&self) -> String {
        match self.index {
            Some(index) => format!("{}.{}[{index}]", self.entry.block.name(), self.entry.name),
            None => format!("{}.{}", self.entry.block.name(), self.entry.name),
        }
    }
}

impl fmt::Display for DecodedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}={}", self.name(), self.value)
    }
}

/// Collects the decoded values.
#[derive(Debug, Default)]
struct Collector(Vec<DecodedValue>);

impl Visitor for Collector {
    fn visit(&mut self, entry: &'static Entry, index: Option<usize>, value: Value) {
        self.0.push(DecodedValue {
            entry,
            index,
            value,
        });
    }
}

/// Every value of the stream in order, with its name (with `tick`, of the
/// round-end form).
pub fn decode(fields: &StateHashFields, tick: Option<u32>) -> Vec<DecodedValue> {
    let mut collector = Collector::default();
    fields.walk(tick, &mut collector);
    collector.0
}

/// The state hash of `view` (see [`StateHashFields::hash`]).
pub fn hash(view: crate::StateView<'_>) -> u32 {
    StateHashFields::from_view(view).hash()
}

/// The round-end hash of `view` at `tick` (see
/// [`StateHashFields::hash_at_round_end`]).
pub fn hash_at_round_end(view: crate::StateView<'_>, tick: u32) -> u32 {
    StateHashFields::from_view(view).hash_at_round_end(tick)
}

/// The hash as text: exactly 8 upper-case hexadecimal digits.
pub fn format_hash(hash: u32) -> String {
    format!("{hash:08X}")
}

#[cfg(test)]
mod tests;
