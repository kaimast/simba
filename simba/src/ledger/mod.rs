mod conventional;
mod nakamoto;

pub use conventional::*;
pub use nakamoto::*;

/// Tracks the all existing blocks and the, currently existing, global state
/// This should not be used by nodes directly, but only for collecting statistics
#[allow(dead_code)]
pub trait GlobalLedger {}

/// Tracks the state of the ledger from the view of a node
/// This might be out of sync as nodes are not guaranteed to see all blocks as soon as they mined
/// Nodes can also be affected by network partitions or hardware failures
#[allow(dead_code)]
pub trait NodeLedger {}
