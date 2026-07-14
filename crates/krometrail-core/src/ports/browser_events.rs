use crate::{browser::BrowserEventBatch, error::Result};

use super::PortFuture;

/// Durable append boundary for normalized browser events.
///
/// Implementations receive only validated, privacy-safe batches. The port is
/// runtime neutral and object safe so persistence cannot leak inward or select
/// the core executor.
pub trait BrowserEventSink: Send + Sync {
    fn append_event_batch(&self, batch: BrowserEventBatch) -> PortFuture<'_, Result<()>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_event_sink_is_object_safe() {
        fn accepts_trait_object(_: Option<&dyn BrowserEventSink>) {}
        accepts_trait_object(None);
    }
}
