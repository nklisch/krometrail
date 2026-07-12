use crate::ids::IdValue;

/// Supplies raw opaque identifier values so domain identity allocation is deterministic in tests.
pub trait IdSource: Send + Sync {
    fn next(&self) -> IdValue;
}
