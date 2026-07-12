use std::{
    sync::Arc,
    time::{Instant, SystemTime},
};

use krometrail_core::{
    AttachBrowser, BrowserConnectRequest, BrowserConnector, BrowserSessionPort, CaptureGap,
    EncodedFrame, ErrorCode, IdSource, IdValue, KrometrailError, MonotonicClock, NonEmptyText,
    PortFuture, RecordingSink, Result, SessionId, SessionRange, TimelineObservation, TimelineStore,
    WallClock,
};
use uuid::Uuid;

// These imports make the root's assembly boundary explicit. Implementations will
// move into these inward-dependent crates as their capabilities land; this root
// remains the only place allowed to choose and connect them.
use krometrail_cdp as _;
use krometrail_mcp as _;
use krometrail_store as _;
use temporal_vision as _;

use crate::cli::Command;

pub(crate) struct RuntimeDependencies {
    pub clock: Arc<dyn MonotonicClock>,
    pub wall_clock: Arc<dyn WallClock>,
    pub ids: Arc<dyn IdSource>,
    pub browser: Arc<dyn BrowserConnector>,
    pub recording: Arc<dyn RecordingSink>,
    pub timeline: Arc<dyn TimelineStore>,
}

pub(crate) struct Runtime {
    dependencies: RuntimeDependencies,
}

impl Runtime {
    pub(crate) fn new(dependencies: RuntimeDependencies) -> Self {
        Self { dependencies }
    }

    pub(crate) async fn run(self, command: Command) -> Result<()> {
        match command {
            Command::Doctor => {
                // Touch the injected process services at the runtime boundary. The
                // browser operation remains the authoritative availability check;
                // clocks and IDs are ready for later commands without leaking their
                // implementations into core.
                let _ = self.dependencies.clock.now();
                let _ = self.dependencies.wall_clock.now();
                let _ = self.dependencies.ids.next();
                let _ = (&self.dependencies.recording, &self.dependencies.timeline);
                self.dependencies
                    .browser
                    .connect(BrowserConnectRequest::Attach(AttachBrowser {
                        endpoint: "unconfigured".to_owned(),
                    }))
                    .await?;
                Ok(())
            }
        }
    }
}

pub(crate) fn build_runtime() -> Runtime {
    Runtime::new(RuntimeDependencies {
        clock: Arc::new(ProcessMonotonicClock {
            origin: Instant::now(),
        }),
        wall_clock: Arc::new(SystemWallClock),
        ids: Arc::new(ProcessIdSource),
        // Do not select a fake-success adapter. This explicit unavailable
        // implementation makes the pre-CDP state observable at the boundary.
        browser: Arc::new(UnavailableBrowserConnector),
        recording: Arc::new(UnavailableRecordingSink),
        timeline: Arc::new(UnavailableTimelineStore),
    })
}

struct ProcessMonotonicClock {
    origin: Instant,
}

impl MonotonicClock for ProcessMonotonicClock {
    fn now(&self) -> krometrail_core::ObservedTime {
        let nanos = self.origin.elapsed().as_nanos();
        let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
        krometrail_core::ObservedTime::from_nanos(nanos)
    }
}

struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

struct ProcessIdSource;

impl IdSource for ProcessIdSource {
    fn next(&self) -> IdValue {
        // UUID v4 randomness keeps persisted identities distinct across
        // independently started processes; core only sees the IdSource port.
        IdValue::from_uuid(Uuid::new_v4())
    }
}

struct UnavailableBrowserConnector;

impl BrowserConnector for UnavailableBrowserConnector {
    fn connect(
        &self,
        _request: BrowserConnectRequest,
    ) -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>> {
        Box::pin(std::future::ready(Err(unavailable(
            "browser transport is not available in this build",
        ))))
    }
}

struct UnavailableRecordingSink;

impl RecordingSink for UnavailableRecordingSink {
    fn append_frame(&self, _frame: EncodedFrame) -> PortFuture<'_, Result<()>> {
        Box::pin(std::future::ready(Err(unavailable(
            "recording storage is not available in this build",
        ))))
    }

    fn append_gap(&self, _gap: CaptureGap) -> PortFuture<'_, Result<()>> {
        Box::pin(std::future::ready(Err(unavailable(
            "recording storage is not available in this build",
        ))))
    }

    fn flush(&self, _session_id: SessionId) -> PortFuture<'_, Result<()>> {
        Box::pin(std::future::ready(Err(unavailable(
            "recording storage is not available in this build",
        ))))
    }
}

struct UnavailableTimelineStore;

impl TimelineStore for UnavailableTimelineStore {
    fn append(&self, _observation: TimelineObservation) -> PortFuture<'_, Result<()>> {
        Box::pin(std::future::ready(Err(unavailable(
            "timeline storage is not available in this build",
        ))))
    }

    fn range(
        &self,
        _session_id: SessionId,
        _target_id: krometrail_core::TargetId,
        _range: SessionRange,
    ) -> PortFuture<'_, Result<Vec<TimelineObservation>>> {
        Box::pin(std::future::ready(Err(unavailable(
            "timeline storage is not available in this build",
        ))))
    }
}

fn unavailable(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Unsupported,
        NonEmptyText::new(message).expect("static unavailable message is non-empty"),
    )
    .with_recovery(
        NonEmptyText::new("wait for the corresponding Rust infrastructure adapter")
            .expect("static recovery message is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn independently_constructed_sources_do_not_repeat_sequences() {
        let first = ProcessIdSource;
        let second = ProcessIdSource;
        let first_ids: Vec<_> = (0..32).map(|_| first.next()).collect();
        let second_ids: Vec<_> = (0..32).map(|_| second.next()).collect();

        assert_eq!(
            HashSet::<IdValue>::from_iter(first_ids.iter().copied()).len(),
            32
        );
        assert_eq!(
            HashSet::<IdValue>::from_iter(second_ids.iter().copied()).len(),
            32
        );
        assert!(first_ids.iter().all(|id| !second_ids.contains(id)));
    }

    #[test]
    fn process_ids_are_uuid_v4_values() {
        let id = ProcessIdSource.next();
        assert_eq!(id.as_uuid().get_version_num(), 4);
        assert_eq!(id.as_uuid().get_variant(), uuid::Variant::RFC4122);
    }
}
