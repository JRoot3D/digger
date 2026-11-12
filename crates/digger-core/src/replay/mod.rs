//! Playing back, verifying and recording games (game-recording and
//! replay-verification specs; design D4 and D6–D8 of
//! `add-recording-and-conformance`).
//!
//! Everything here is built on the simulation's one seam,
//! [`Game::step_observed`](crate::Game::step_observed) and its
//! [`Observer`](crate::Observer) hooks: [`ReplayObserver`] composes a
//! playback source, a recorder, a checkpoint verifier and a trace sink,
//! each optional, and forwards every hook to them in a fixed order (see
//! `docs/conformance.md`, "Simulation seam"). The driver never sleeps and
//! never reads a clock; pacing is the caller's.
//!
//! The module will hold, as later task groups add them:
//!
//! - `Replay`, `ReplayOptions`, `ReplayReport`, `Outcome` and `verify()`;
//! - `playback`: the playback sources (DRF and eDRF), [`ReplayObserver`]
//!   and take-over;
//! - `verifier`: checkpoint comparison and divergence reports;
//! - `recorder`: the DRF and eDRF recorders;
//! - `trace`: per-tick trace lines and the decoded state dump.

mod playback;
mod recorder;
mod trace;
mod verifier;

pub use playback::ReplayObserver;
