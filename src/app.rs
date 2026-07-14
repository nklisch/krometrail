use std::{
    sync::Arc,
    time::{Instant, SystemTime},
};

use krometrail_core::{
    BrowserConnector, CaptureGapStore, ErrorCode, FrameSource, IdSource, IdValue, KrometrailError,
    MonotonicClock, NonEmptyText, RecordingCatalog, RecordingSink, Result, TimelineStore,
    WallClock,
};
use uuid::Uuid;

// These imports make the root's assembly boundary explicit. Implementations will
// move into these inward-dependent crates as their capabilities land; this root
// remains the only place allowed to choose and connect them.
use krometrail_cdp::{
    CaptureConfig, LauncherConfig, ProductionBrowserConnector, SystemChromeLauncher,
};
use krometrail_mcp as _;
use krometrail_store::{
    IndexStoreConfig, IndexedRecordingSink, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex, recover,
};
use temporal_vision as _;

use crate::cli::Command;

pub(crate) struct RuntimeDependencies {
    pub clock: Arc<dyn MonotonicClock>,
    pub wall_clock: Arc<dyn WallClock>,
    pub ids: Arc<dyn IdSource>,
    pub browser: Arc<dyn BrowserConnector>,
    pub recording: Arc<dyn RecordingSink>,
    pub timeline: Arc<dyn TimelineStore>,
    pub catalog: Arc<dyn RecordingCatalog>,
    pub gaps: Arc<dyn CaptureGapStore>,
    pub frames: Arc<dyn FrameSource>,
}

struct StorageDependencies {
    recording: Arc<dyn RecordingSink>,
    timeline: Arc<dyn TimelineStore>,
    catalog: Arc<dyn RecordingCatalog>,
    gaps: Arc<dyn CaptureGapStore>,
    frames: Arc<dyn FrameSource>,
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
                let _ = (
                    &self.dependencies.recording,
                    &self.dependencies.timeline,
                    &self.dependencies.catalog,
                    &self.dependencies.gaps,
                    &self.dependencies.frames,
                );
                let installations = self.dependencies.browser.installations().await?;
                if installations.is_empty() {
                    return Err(browser_not_found());
                }
                println!("browser available: {} installation(s)", installations.len());
                Ok(())
            }
        }
    }
}

pub(crate) fn build_runtime() -> Result<Runtime> {
    let clock: Arc<dyn MonotonicClock> = Arc::new(ProcessMonotonicClock {
        origin: Instant::now(),
    });
    let ids: Arc<dyn IdSource> = Arc::new(ProcessIdSource);
    let data_directory = data_directory();
    let storage = open_storage(&data_directory)?;
    let profile_root = std::env::var_os("KROMETRAIL_PROFILE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_directory.join("browser-profiles"));
    let browser: Arc<dyn BrowserConnector> = Arc::new(
        ProductionBrowserConnector::new(
            Arc::new(SystemChromeLauncher::new(LauncherConfig::new(profile_root))),
            Arc::new(
                krometrail_cdp::transport::CdpkitTransportFactory::new()
                    .with_command_timeout(std::time::Duration::from_secs(3)),
            ),
        )
        .with_capture(
            Arc::clone(&clock),
            Arc::clone(&ids),
            Arc::clone(&storage.recording),
            CaptureConfig::default(),
        ),
    );
    Ok(Runtime::new(RuntimeDependencies {
        clock,
        wall_clock: Arc::new(SystemWallClock),
        ids,
        browser,
        recording: storage.recording,
        timeline: storage.timeline,
        catalog: storage.catalog,
        gaps: storage.gaps,
        frames: storage.frames,
    }))
}

fn open_storage(data_directory: &std::path::Path) -> Result<StorageDependencies> {
    let segments_directory = data_directory.join("segments");
    // Open and migrate metadata before capture infrastructure can accept writes.
    let index = Arc::new(SqliteIndex::open(IndexStoreConfig {
        database_path: data_directory.join("index.sqlite3"),
        segments_directory: segments_directory.clone(),
        busy_timeout: std::time::Duration::from_secs(5),
    })?);
    let segments = Arc::new(SegmentWriter::open(SegmentStoreConfig {
        directory: segments_directory,
        rotation: RotationConfig::suggested(),
    })?);
    let recovery = recover(index.as_ref())?;
    tracing::info!(
        open_segments_sealed = recovery.open_segments_sealed,
        segments_repaired = recovery.segments_repaired,
        segments_quarantined = recovery.segments_quarantined,
        frames_recovered = recovery.frames_recovered,
        frames_removed = recovery.frames_removed,
        "recording store recovery complete"
    );
    Ok(StorageDependencies {
        recording: Arc::new(IndexedRecordingSink::new(segments, Arc::clone(&index))),
        timeline: Arc::clone(&index) as Arc<dyn TimelineStore>,
        catalog: Arc::clone(&index) as Arc<dyn RecordingCatalog>,
        gaps: Arc::clone(&index) as Arc<dyn CaptureGapStore>,
        frames: index as Arc<dyn FrameSource>,
    })
}

fn data_directory() -> std::path::PathBuf {
    if let Some(configured) =
        std::env::var_os("KROMETRAIL_DATA_DIR").filter(|value| !value.is_empty())
    {
        return configured.into();
    }
    if cfg!(target_os = "macos") {
        if let Some(home) = std::env::var_os("HOME") {
            return std::path::PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("krometrail");
        }
    } else if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return std::path::PathBuf::from(data_home).join("krometrail");
    } else if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("krometrail");
    }

    tracing::warn!("platform data directory unavailable; using ./krometrail-data");
    std::path::PathBuf::from("krometrail-data")
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

fn browser_not_found() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::BrowserNotFound,
        NonEmptyText::new("no supported browser installation was found")
            .expect("static browser error message is non-empty"),
    )
    .with_retry(krometrail_core::RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("install Chrome or Chromium, then run doctor again")
            .expect("static browser recovery message is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::PortFuture;
    use std::{
        collections::HashSet,
        sync::atomic::{AtomicUsize, Ordering},
    };

    struct DiscoveryOnlyFake {
        installations_calls: AtomicUsize,
    }

    impl BrowserConnector for DiscoveryOnlyFake {
        fn installations(
            &self,
        ) -> PortFuture<'_, Result<Vec<krometrail_core::BrowserInstallation>>> {
            self.installations_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(Ok(Vec::new())))
        }

        fn connect(
            &self,
            _request: krometrail_core::BrowserConnectRequest,
        ) -> PortFuture<'_, Result<Arc<dyn krometrail_core::BrowserSessionPort>>> {
            panic!("doctor must not connect to a browser");
        }
    }

    #[tokio::test]
    async fn doctor_is_discovery_only() {
        let browser = Arc::new(DiscoveryOnlyFake {
            installations_calls: AtomicUsize::new(0),
        });
        let recording_directory =
            std::env::temp_dir().join(format!("krometrail-doctor-test-{}", Uuid::new_v4()));
        let storage = open_storage(&recording_directory).unwrap();
        let runtime = Runtime::new(RuntimeDependencies {
            clock: Arc::new(ProcessMonotonicClock {
                origin: Instant::now(),
            }),
            wall_clock: Arc::new(SystemWallClock),
            ids: Arc::new(ProcessIdSource),
            browser: Arc::clone(&browser) as Arc<dyn BrowserConnector>,
            recording: storage.recording,
            timeline: storage.timeline,
            catalog: storage.catalog,
            gaps: storage.gaps,
            frames: storage.frames,
        });
        let error = runtime.run(Command::Doctor).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::BrowserNotFound);
        assert_eq!(browser.installations_calls.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(recording_directory).unwrap();
    }

    #[tokio::test]
    async fn storage_composition_shares_lossless_gap_metadata_and_fails_before_runtime() {
        let root = std::env::temp_dir().join(format!("krometrail-storage-test-{}", Uuid::new_v4()));
        let storage = open_storage(&root).unwrap();
        let session = krometrail_core::SessionId::from_uuid(Uuid::from_u128(1));
        let target = krometrail_core::TargetId::from_uuid(Uuid::from_u128(2));
        let gap = krometrail_core::CaptureGap::new(
            krometrail_core::GapId::from_uuid(Uuid::from_u128(3)),
            session,
            target,
            krometrail_core::SessionRange::new(
                krometrail_core::SessionTime::from_nanos(1),
                krometrail_core::SessionTime::from_nanos(2),
            )
            .unwrap(),
            krometrail_core::ObservedTime::from_nanos(3),
            krometrail_core::CaptureGapReason::CaptureStopped,
            std::num::NonZeroU64::new(1),
            Some("shutdown boundary".into()),
        )
        .unwrap();
        storage.recording.append_gap(gap.clone()).await.unwrap();
        assert_eq!(
            storage
                .gaps
                .gaps(
                    session,
                    target,
                    krometrail_core::SessionRange::new(
                        krometrail_core::SessionTime::from_nanos(2),
                        krometrail_core::SessionTime::from_nanos(2),
                    )
                    .unwrap(),
                )
                .await
                .unwrap(),
            std::slice::from_ref(&gap)
        );
        drop(storage);
        std::fs::remove_dir_all(&root).unwrap();

        let occupied =
            std::env::temp_dir().join(format!("krometrail-storage-file-{}", Uuid::new_v4()));
        std::fs::write(&occupied, b"not a data directory").unwrap();
        assert!(open_storage(&occupied).is_err());
        std::fs::remove_file(occupied).unwrap();
    }

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
