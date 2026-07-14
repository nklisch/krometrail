//! Session-owned browser-event collection and network activity routing.
//!
//! CDP values are normalized into the core allowlist before they reach bounded
//! queues. The session domain authority is the only owner of semantic named
//! subscriptions and domain enablement.

use std::{num::NonZeroUsize, time::Duration};

mod domain;
mod network;
mod normalize;
mod pipeline;
mod privacy;
mod signals;
mod status;

pub(crate) use domain::{
    EventTargetBinding, NetworkSetupError, PageSignalSetupError, SessionDomainAuthority,
};
pub(crate) use network::{
    NetworkActivity, NetworkActivityKind, NetworkReceiveError, NetworkRequestKey,
};
pub(crate) use signals::{PageSignalKind, PageSignalReceiveError, PageSignalReceiver};
pub use status::BrowserEventStatus;

const HARD_MAX_ACTIVE_TARGETS: usize = 32;
const HARD_MAX_TARGET_QUEUE: usize = 1_024;
const HARD_MAX_PENDING_BYTES: usize = 64 * 1024 * 1024;
const HARD_MAX_BATCH_ROWS: usize = krometrail_core::MAX_BROWSER_EVENT_BATCH_ROWS;
const HARD_MAX_BATCH_BYTES: usize = krometrail_core::MAX_BROWSER_EVENT_BATCH_BYTES;
const HARD_MAX_NETWORK_FANOUT: usize = 8_192;
const HARD_MAX_REQUEST_CORRELATIONS: usize = 16_384;
const HARD_MAX_GAP_LEDGER: usize = 256;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserEventConfig {
    pub enabled: bool,
    pub max_active_targets: NonZeroUsize,
    pub per_target_queue_capacity: NonZeroUsize,
    pub global_pending_bytes: NonZeroUsize,
    pub store_batch_rows: NonZeroUsize,
    pub store_batch_bytes: NonZeroUsize,
    pub network_fanout_capacity: NonZeroUsize,
    pub request_map_capacity: NonZeroUsize,
    pub gap_ledger_capacity: NonZeroUsize,
    pub persistence_retry_initial: Duration,
    pub persistence_retry_max: Duration,
}

impl Default for BrowserEventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_active_targets: NonZeroUsize::new(32).expect("default target cap is non-zero"),
            per_target_queue_capacity: NonZeroUsize::new(256)
                .expect("default event queue is non-zero"),
            global_pending_bytes: NonZeroUsize::new(16 * 1024 * 1024)
                .expect("default pending-byte budget is non-zero"),
            store_batch_rows: NonZeroUsize::new(128).expect("default batch rows are non-zero"),
            store_batch_bytes: NonZeroUsize::new(256 * 1024)
                .expect("default batch bytes are non-zero"),
            network_fanout_capacity: NonZeroUsize::new(1_024)
                .expect("default network fanout is non-zero"),
            request_map_capacity: NonZeroUsize::new(4_096)
                .expect("default request map is non-zero"),
            gap_ledger_capacity: NonZeroUsize::new(64).expect("default gap ledger is non-zero"),
            persistence_retry_initial: Duration::from_millis(10),
            persistence_retry_max: Duration::from_millis(250),
        }
    }
}

impl BrowserEventConfig {
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> krometrail_core::Result<()> {
        let invalid = self.max_active_targets.get() > HARD_MAX_ACTIVE_TARGETS
            || self.per_target_queue_capacity.get() > HARD_MAX_TARGET_QUEUE
            || self.global_pending_bytes.get() > HARD_MAX_PENDING_BYTES
            || self.store_batch_rows.get() > HARD_MAX_BATCH_ROWS
            || self.store_batch_bytes.get() > HARD_MAX_BATCH_BYTES
            || self.network_fanout_capacity.get() > HARD_MAX_NETWORK_FANOUT
            || self.request_map_capacity.get() > HARD_MAX_REQUEST_CORRELATIONS
            || self.gap_ledger_capacity.get() > HARD_MAX_GAP_LEDGER
            || self.persistence_retry_initial.is_zero()
            || self.persistence_retry_initial > self.persistence_retry_max;
        if invalid {
            return Err(krometrail_core::KrometrailError::new(
                krometrail_core::ErrorCode::InvalidInput,
                krometrail_core::NonEmptyText::new(
                    "browser event configuration exceeds its bounded limits",
                )
                .expect("static configuration error is non-empty"),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_disabled_mode_preserve_documented_bounds() {
        let config = BrowserEventConfig::default();
        config.validate().unwrap();
        assert!(config.enabled);
        assert_eq!(config.max_active_targets.get(), 32);
        assert_eq!(config.per_target_queue_capacity.get(), 256);
        assert_eq!(config.global_pending_bytes.get(), 16 * 1024 * 1024);
        assert_eq!(config.store_batch_rows.get(), 128);
        assert_eq!(config.store_batch_bytes.get(), 256 * 1024);
        assert_eq!(config.network_fanout_capacity.get(), 1_024);
        assert_eq!(config.request_map_capacity.get(), 4_096);
        assert_eq!(config.gap_ledger_capacity.get(), 64);
        assert!(!BrowserEventConfig::disabled().enabled);
    }
}
