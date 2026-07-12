use std::time::SystemTime;

use crate::time::ObservedTime;

/// Supplies the monotonic observation clock used for session ordering.
pub trait MonotonicClock: Send + Sync {
    fn now(&self) -> ObservedTime;
}

/// Supplies wall-clock timestamps for human-facing session metadata.
pub trait WallClock: Send + Sync {
    fn now(&self) -> SystemTime;
}
