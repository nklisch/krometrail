use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
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
        ids: Arc::new(ProcessIdSource::default()),
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

#[derive(Default)]
struct ProcessIdSource {
    next: AtomicU64,
}

impl IdSource for ProcessIdSource {
    fn next(&self) -> IdValue {
        let sequence = self.next.fetch_add(1, Ordering::Relaxed);
        IdValue::from_uuid(Uuid::from_u128(u128::from(sequence) + 1))
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
