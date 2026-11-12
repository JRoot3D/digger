//! The DRF and eDRF recorders (design D9 of `add-recording-and-conformance`).

use crate::Observer;

/// A recorder writing the game being played as a recording. It only
/// observes: [`ReplayObserver`](super::ReplayObserver) tells it the values
/// actually used and ignores what it returns.
///
/// No recorder exists yet; task group 8 adds the DRF and eDRF recorders.
#[derive(Debug)]
pub(crate) enum Recorder {}

impl Observer for Recorder {}
