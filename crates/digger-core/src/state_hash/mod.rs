//! The state hash: an exact byte stream over the simulation state, digested
//! with MD5 and cut to the first four digest bytes, as the checkpoints of
//! upstream's eDRF recordings hold it (replay-verification spec; design D5
//! of `add-recording-and-conformance`).
//!
//! The hash is a file format in its own right. It is defined by one ordered
//! field table, independent of how the simulation lays out its structs, and
//! is computed from the read-only [`StateView`](crate::StateView) alone.
//! The module will hold, as later task groups add them:
//!
//! - the conversion of a `StateView` into plain per-block field values, a
//!   `walk` over them in table order, `hash()` and `hash_at_round_end()`;
//! - `layout`: the field table (block, name, width, repeat, condition);
//! - `codes`: the mappings from the simulation's enums to their wire codes.
