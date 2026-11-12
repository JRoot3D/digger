//! Per-tick trace lines and the decoded state dump for divergence triage
//! (design D12 of `add-recording-and-conformance`).

use crate::Observer;

/// Writes one trace line per tick. It only observes:
/// [`ReplayObserver`](super::ReplayObserver) tells it the values actually
/// used and ignores what it returns.
///
/// No trace sink exists yet; task group 10 adds it.
#[derive(Debug)]
pub(crate) enum TraceSink {}

impl Observer for TraceSink {}
