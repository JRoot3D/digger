//! Checkpoint comparison and divergence reports (design D6 of
//! `add-recording-and-conformance`).

use crate::Observer;

/// Compares the state hash with a recording's checkpoints. It only
/// observes: [`ReplayObserver`](super::ReplayObserver) tells it the values
/// actually used and ignores what it returns.
///
/// No verifier exists yet; task group 7 adds it.
#[derive(Debug)]
pub(crate) enum Verifier {}

impl Observer for Verifier {}
