use std::{
    sync::Arc,
    time::{Instant, SystemTime},
};

use krometrail_core::{
    ArtifactGeneration, ArtifactStore, BrowserConnector, BrowserEventSink, CapabilityId,
    CapabilitySnapshot, DiskBudgetBytes, ErrorCode, FrameSource, IdSource, IdValue,
    InteractionEvidenceSink, KrometrailError, MonotonicClock, NonEmptyText, ProgressiveEvidence,
    ProgressiveEvidenceStore, RecordingCatalog, RecordingSink, ResolvedRangeHandles, Result,
    RetentionLifecycle, RetentionStore, TemporalContextQuery, TemporalDebugBundles, TemporalQuery,
    TemporalVideoEncoder, TemporalVideoGeneration, WallClock,
};
#[cfg(feature = "qualification-support")]
use krometrail_core::{CaptureGapStore, TimelineStore};
use uuid::Uuid;

// These imports make the root's assembly boundary explicit. Implementations will
// move into these inward-dependent crates as their capabilities land; this root
// remains the only place allowed to choose and connect them.
use crate::{
    artifacts::{ArtifactWorkLimits, TemporalVisionArtifactService},
    debug_bundle::{BundleWorkLimits, TemporalDebugBundleService, TemporalDebugEvidenceStore},
    progressive::ProgressiveEvidenceService,
    range_handles::ProcessResolvedRangeHandles,
    video::{TemporalVideoGenerationService, VideoGenerationLimits},
};
use krometrail_cdp::{
    BrowserEventConfig, CaptureConfig, LauncherConfig, ProductionBrowserConnector,
    SystemChromeLauncher,
};
use krometrail_ffmpeg::{
    FfmpegDiscoveryOptions, FfmpegQualification, FfmpegUnavailable, qualify_ffmpeg,
};
use krometrail_mcp::{DiagnosticContext, McpConfig, McpDependencies, build_service};
use krometrail_store::{
    IndexStoreConfig, InstanceOwnership, RecordingStore, RecoveryReport, RotationConfig,
    SegmentStoreConfig, SegmentWriter, SqliteIndex, recover,
};

#[cfg(all(test, feature = "qualification-support"))]
pub(crate) mod live_evaluation;

#[cfg(all(test, feature = "qualification-support"))]
pub(crate) mod platform_evidence;

pub(crate) struct RuntimeDependencies {
    // Runtime is the MCP-serving runtime only: doctor composes its own discovery
    // authority in src/doctor.rs and never builds these services. Every Arc here
    // is either read by compose_temporal_video/mcp_dependencies or keeps a store
    // projection alive through the services that retain it.
    pub browser: Arc<dyn BrowserConnector>,
    pub frames: Arc<dyn FrameSource>,
    pub ids: Arc<dyn IdSource>,
    pub temporal_debug_bundles: Arc<dyn TemporalDebugBundles>,
    pub progressive_evidence: Arc<dyn ProgressiveEvidence>,
    pub temporal_context: Arc<dyn TemporalContextQuery>,
    pub range_handles: Arc<dyn ResolvedRangeHandles>,
    pub artifacts: Arc<dyn ArtifactStore>,
    pub diagnostics: DiagnosticContext,
    // The feature-gated qualification runtime composes the full recording
    // runtime and its observation modules read these projections directly
    // (live_evaluation capture/control). The default surface stays lean, so
    // they exist only under the qualification feature.
    #[cfg(feature = "qualification-support")]
    pub clock: Arc<dyn MonotonicClock>,
    #[cfg(feature = "qualification-support")]
    pub wall_clock: Arc<dyn WallClock>,
    #[cfg(feature = "qualification-support")]
    pub recording: Arc<dyn RecordingSink>,
    #[cfg(feature = "qualification-support")]
    pub retention: Arc<dyn RetentionStore>,
    #[cfg(feature = "qualification-support")]
    pub timeline: Arc<dyn TimelineStore>,
    #[cfg(feature = "qualification-support")]
    pub catalog: Arc<dyn RecordingCatalog>,
    #[cfg(feature = "qualification-support")]
    pub gaps: Arc<dyn CaptureGapStore>,
    #[cfg(feature = "qualification-support")]
    pub temporal_queries: Arc<dyn TemporalQuery>,
    #[cfg(feature = "qualification-support")]
    pub artifact_generation: Arc<dyn ArtifactGeneration>,
}

/// Composition hook retained for the later agent-facing video surface. The caller supplies the
/// qualified encoder; this feature does not conditionally discover FFmpeg or wire MCP tools.
#[allow(dead_code)]
pub(crate) fn build_temporal_video_generation(
    frames: Arc<dyn FrameSource>,
    artifacts: Arc<dyn ArtifactStore>,
    ids: Arc<dyn IdSource>,
    encoder: Arc<dyn TemporalVideoEncoder>,
    limits: VideoGenerationLimits,
) -> Result<Arc<dyn TemporalVideoGeneration>> {
    Ok(Arc::new(TemporalVideoGenerationService::new(
        frames, artifacts, ids, encoder, limits,
    )?))
}

struct StorageDependencies {
    store: Arc<RecordingStore>,
    recording: Arc<dyn RecordingSink>,
    retention: Arc<dyn RetentionStore>,
    catalog: Arc<dyn RecordingCatalog>,
    frames: Arc<dyn FrameSource>,
    temporal_queries: Arc<dyn TemporalQuery>,
    browser_event_sink: Arc<dyn BrowserEventSink>,
    temporal_context: Arc<dyn TemporalContextQuery>,
    artifacts: Arc<dyn ArtifactStore>,
    // The feature-gated qualification runtime composes from these projections;
    // the default product runtime wires capture and queries without them.
    #[cfg(feature = "qualification-support")]
    timeline: Arc<dyn TimelineStore>,
    #[cfg(feature = "qualification-support")]
    gaps: Arc<dyn CaptureGapStore>,
    // The qualification composition root transfers this authority-owned report to its observation
    // runtime; the normal product runtime intentionally does not publish startup diagnostics.
    #[allow(dead_code)]
    recovery: RecoveryReport,
}

impl RuntimeDependencies {
    fn mcp_dependencies(
        &self,
        temporal_video: Option<Arc<dyn TemporalVideoGeneration>>,
    ) -> McpDependencies {
        McpDependencies {
            browser: Arc::clone(&self.browser),
            temporal_debug_bundles: Arc::clone(&self.temporal_debug_bundles),
            progressive_evidence: Arc::clone(&self.progressive_evidence),
            temporal_context: Arc::clone(&self.temporal_context),
            range_handles: Arc::clone(&self.range_handles),
            temporal_video,
            diagnostics: self.diagnostics.clone(),
        }
    }
}

pub(crate) struct Runtime {
    dependencies: RuntimeDependencies,
}

impl Runtime {
    pub(crate) fn new(dependencies: RuntimeDependencies) -> Self {
        Self { dependencies }
    }

    pub(crate) async fn run_mcp(self) -> Result<()> {
        // FFmpeg is optional and MCP-only. One bounded qualification result controls the
        // immutable capability snapshot and the optional retained generation service.
        let qualification = qualify_ffmpeg(
            FfmpegDiscoveryOptions::from_process_environment(),
            Arc::new(StartupCancellation),
            Instant::now() + krometrail_ffmpeg::FFMPEG_QUALIFICATION_TIMEOUT,
        )
        .await;
        let (encoder, unavailable, identity) = match qualification {
            FfmpegQualification::Qualified(encoder) => {
                let identity = encoder.identity().clone();
                let encoder: Arc<dyn TemporalVideoEncoder> = encoder;
                (Some(encoder), None, Some(identity))
            }
            FfmpegQualification::Unavailable(unavailable) => (None, Some(unavailable), None),
        };
        let (mcp_config, temporal_video) = compose_temporal_video(&self.dependencies, encoder)?;
        log_temporal_video_availability(
            temporal_video.as_ref(),
            unavailable.as_ref(),
            identity.as_ref(),
        );
        build_service(
            self.dependencies.mcp_dependencies(temporal_video),
            mcp_config,
        )?
        .serve_stdio()
        .await
    }
}

pub(crate) fn build_runtime(diagnostics: DiagnosticContext) -> Result<Runtime> {
    let clock: Arc<dyn MonotonicClock> = Arc::new(ProcessMonotonicClock {
        origin: Instant::now(),
    });
    let ids: Arc<dyn IdSource> = Arc::new(ProcessIdSource);
    let data_directory = data_directory();
    let wall_clock: Arc<dyn WallClock> = Arc::new(SystemWallClock);
    let disk_budget = configured_disk_budget()?;
    let storage = open_storage_with_budget(&data_directory, disk_budget, Arc::clone(&clock))?;
    let range_handles: Arc<dyn ResolvedRangeHandles> = Arc::new(ProcessResolvedRangeHandles::new(
        Arc::clone(&ids),
        Arc::clone(&storage.frames),
    ));
    // One capability selection governs both collection and the MCP surface. Browser events are
    // default-enabled by the core registry, while an explicit selection can still disable their
    // semantic subscriptions without changing capture or control composition.
    let mcp_config = McpConfig::default();
    let browser_event_config = browser_event_config(&mcp_config);
    let profile_root = std::env::var_os("KROMETRAIL_PROFILE_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_directory.join("browser-profiles"));
    let artifact_generation: Arc<dyn ArtifactGeneration> =
        Arc::new(TemporalVisionArtifactService::new(
            Arc::clone(&storage.frames),
            Arc::clone(&storage.artifacts),
            Arc::clone(&ids),
            ArtifactWorkLimits::default(),
        )?);
    let progressive_evidence: Arc<dyn ProgressiveEvidence> =
        Arc::new(ProgressiveEvidenceService::new(
            Arc::clone(&storage.store) as Arc<dyn ProgressiveEvidenceStore>,
            Arc::clone(&artifact_generation),
        ));
    // One bundle service over the same concrete store (temporal query, timeline/
    // interaction evidence, temporal context) and the shared artifact service.
    // The two-request/20-second limits bound orchestration independently of the
    // artifact service's own scheduler/cache/single-flight permits.
    let temporal_debug_bundles: Arc<dyn TemporalDebugBundles> =
        Arc::new(TemporalDebugBundleService::new(
            Arc::clone(&storage.temporal_queries),
            Arc::clone(&storage.store) as Arc<dyn TemporalDebugEvidenceStore>,
            Arc::clone(&artifact_generation),
            Arc::clone(&storage.temporal_context),
            BundleWorkLimits::default(),
        )?);
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
            Arc::clone(&storage.retention),
            CaptureConfig::default(),
        )
        .with_browser_events(
            Arc::clone(&clock),
            Arc::clone(&ids),
            Arc::clone(&storage.browser_event_sink),
            browser_event_config,
        )
        .with_session_catalog(
            Arc::clone(&storage.catalog),
            Arc::clone(&wall_clock),
            disk_budget,
            mcp_config.enabled_capabilities().to_vec(),
        )
        .with_managed_download_root(data_directory.join("browser-downloads"))
        .with_interaction_evidence(Arc::clone(&storage.store) as Arc<dyn InteractionEvidenceSink>),
    );
    Ok(Runtime::new(RuntimeDependencies {
        browser,
        frames: storage.frames,
        ids,
        temporal_debug_bundles,
        progressive_evidence,
        temporal_context: storage.temporal_context,
        range_handles,
        artifacts: storage.artifacts,
        diagnostics,
        #[cfg(feature = "qualification-support")]
        clock,
        #[cfg(feature = "qualification-support")]
        wall_clock,
        #[cfg(feature = "qualification-support")]
        recording: storage.recording,
        #[cfg(feature = "qualification-support")]
        retention: storage.retention,
        #[cfg(feature = "qualification-support")]
        timeline: storage.timeline,
        #[cfg(feature = "qualification-support")]
        catalog: storage.catalog,
        #[cfg(feature = "qualification-support")]
        gaps: storage.gaps,
        #[cfg(feature = "qualification-support")]
        temporal_queries: storage.temporal_queries,
        #[cfg(feature = "qualification-support")]
        artifact_generation,
    }))
}

fn compose_temporal_video(
    dependencies: &RuntimeDependencies,
    encoder: Option<Arc<dyn TemporalVideoEncoder>>,
) -> Result<(McpConfig, Option<Arc<dyn TemporalVideoGeneration>>)> {
    let temporal_video = encoder
        .map(|encoder| {
            build_temporal_video_generation(
                Arc::clone(&dependencies.frames),
                Arc::clone(&dependencies.artifacts),
                Arc::clone(&dependencies.ids),
                encoder,
                VideoGenerationLimits::default(),
            )
        })
        .transpose()?;
    let runtime_qualified = temporal_video
        .is_some()
        .then_some(CapabilityId::TemporalVideo)
        .into_iter()
        .collect::<Vec<_>>();
    let snapshot = CapabilitySnapshot::resolve_defaults(&runtime_qualified)?;
    Ok((McpConfig::from_snapshot(snapshot), temporal_video))
}

fn log_temporal_video_availability(
    service: Option<&Arc<dyn TemporalVideoGeneration>>,
    unavailable: Option<&FfmpegUnavailable>,
    identity: Option<&krometrail_core::VideoEncoderIdentity>,
) {
    if service.is_some() {
        let identity = identity.expect("qualified video service carries encoder identity");
        tracing::info!(
            event = "capability.availability",
            capability = "temporal-video",
            availability = "qualified",
            encoder = identity.encoder_name(),
            adapter_version = identity.adapter_version(),
            argument_policy = identity.argument_policy_version(),
            restart_required_for_change = true,
            "temporal video startup availability resolved"
        );
    } else {
        tracing::info!(
            event = "capability.availability",
            capability = "temporal-video",
            availability = "unavailable",
            qualification_stage = ?unavailable.map(|value| value.stage),
            reason = ?unavailable.map(|value| value.reason),
            failed_check = unavailable
                .and_then(|value| value.output_check)
                .map(|detail| detail.check.name()),
            expected_property = unavailable
                .and_then(|value| value.output_check)
                .map(|detail| detail.expected.to_string()),
            observed_property = unavailable
                .and_then(|value| value.output_check)
                .map(|detail| detail.observed.to_string()),
            restart_required_for_change = true,
            "temporal video startup availability resolved"
        );
    }
}

struct StartupCancellation;

impl krometrail_core::CancellationSignal for StartupCancellation {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> krometrail_core::PortFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}

fn browser_event_config(capabilities: &McpConfig) -> BrowserEventConfig {
    if capabilities.is_enabled(CapabilityId::BrowserEvents) {
        BrowserEventConfig::default()
    } else {
        BrowserEventConfig::disabled()
    }
}

fn open_storage_with_budget(
    data_directory: &std::path::Path,
    budget: DiskBudgetBytes,
    clock: Arc<dyn MonotonicClock>,
) -> Result<StorageDependencies> {
    // Claim an exclusive instance root before touching any retained data. Every
    // destructive startup path below — legacy clearing, incompatible-cache
    // clearing, recovery — runs only against storage this process owns.
    let ownership = InstanceOwnership::acquire_new(data_directory)?;
    let instance_root = ownership.root().to_path_buf();

    // Say so, loudly, where ownership cannot be proved. Isolation still holds —
    // this process writes only its own root — but nothing here can tell a dead
    // root from a live one, so abandoned roots are never reclaimed and shared
    // budget accounting is unavailable. Both cost disk; the alternative, guessing
    // that a root is dead, costs another process's evidence.
    if !krometrail_store::OWNERSHIP_IS_ENFORCED {
        tracing::warn!(
            event = "retention.instance_ownership_unenforced",
            "this platform cannot prove exclusive ownership of an instance root: \
             abandoned roots will not be reclaimed and instances will not share one \
             total disk budget"
        );
    }

    // The pre-isolation flat layout has no supported consumer, so it is cleared
    // rather than migrated. Only recording-cache members are removed; browser
    // profiles, diagnostics, downloads, plugin state, and configuration share
    // this directory and must survive.
    if krometrail_store::has_legacy_flat_store(data_directory) {
        let bytes = krometrail_store::clear_legacy_flat_store(data_directory)?;
        tracing::warn!(
            event = "retention.legacy_store_cleared",
            reclaimed_bytes = bytes,
            "cleared the pre-isolation recording cache"
        );
    }
    reclaim_abandoned_instances(data_directory, &instance_root);

    let segments_directory = instance_root.join("segments");
    // Open and validate metadata before capture infrastructure can accept writes.
    let index = Arc::new(SqliteIndex::open(IndexStoreConfig {
        database_path: instance_root.join("index.sqlite3"),
        segments_directory: segments_directory.clone(),
        busy_timeout: std::time::Duration::from_secs(5),
    })?);
    // Recovery reconciles every unsealed segment left by a previous process and
    // must complete before a writer exists. Opening the writer first let a
    // freshly created open segment coexist with recovery's directory sweep.
    let recovery = recover(index.as_ref())?;
    tracing::info!(
        open_segments_sealed = recovery.open_segments_sealed,
        segments_repaired = recovery.segments_repaired,
        segments_quarantined = recovery.segments_quarantined,
        frames_recovered = recovery.frames_recovered,
        frames_removed = recovery.frames_removed,
        "recording store recovery complete"
    );
    let segments = Arc::new(SegmentWriter::open(SegmentStoreConfig {
        directory: segments_directory,
        rotation: RotationConfig::suggested(),
    })?);
    // The live-instance count divides the configured total across concurrent
    // instances: each one enforces `total / live`. The count comes from the
    // instance locks already held, so there is nothing to publish and nothing to
    // go stale.
    // Ownership moves into the census, which the store holds, so the advisory
    // lock lives exactly as long as the storage that depends on it.
    let census = krometrail_store::InstanceCensus::new(data_directory, ownership);
    let store = Arc::new(RecordingStore::with_retention(
        segments,
        Arc::clone(&index),
        configured_retention(budget)?,
        Some(census),
        clock,
    )?);
    Ok(StorageDependencies {
        store: Arc::clone(&store),
        recovery,
        recording: Arc::clone(&store) as Arc<dyn RecordingSink>,
        retention: Arc::clone(&store) as Arc<dyn RetentionStore>,
        temporal_queries: Arc::clone(&store) as Arc<dyn TemporalQuery>,
        browser_event_sink: Arc::clone(&store) as Arc<dyn BrowserEventSink>,
        temporal_context: Arc::clone(&store) as Arc<dyn TemporalContextQuery>,
        artifacts: Arc::clone(&store) as Arc<dyn ArtifactStore>,
        catalog: Arc::clone(&index) as Arc<dyn RecordingCatalog>,
        #[cfg(feature = "qualification-support")]
        timeline: Arc::clone(&store) as Arc<dyn TimelineStore>,
        #[cfg(feature = "qualification-support")]
        gaps: Arc::clone(&index) as Arc<dyn CaptureGapStore>,
        frames: store as Arc<dyn FrameSource>,
    })
}

/// Reclaims instance roots abandoned by processes that are no longer running.
///
/// Tier one of retention: an abandoned root is the cheapest possible reclaim
/// because nothing live can reference it. Acquiring the root's lock is both the
/// liveness test and the ownership transfer, so reclamation always runs as that
/// root's legitimate owner and can never race a live process.
///
/// Reclamation is best effort. A root that cannot be reclaimed must never keep
/// this process from starting, so failures are reported and stepped over.
fn reclaim_abandoned_instances(data_directory: &std::path::Path, owned: &std::path::Path) {
    let roots = match krometrail_store::sibling_instance_roots(data_directory, owned) {
        Ok(roots) => roots,
        Err(error) => {
            tracing::warn!(
                event = "retention.instance_scan_failed",
                error = %error.message.as_str(),
                "could not enumerate abandoned instance roots"
            );
            return;
        }
    };
    for candidate in roots {
        match candidate.claim() {
            // Held by a live process, or no longer the directory that was
            // classified: not ours, not abandoned, leave it alone.
            Ok(None) => {}
            Ok(Some(ownership)) => match krometrail_store::reclaim_instance_root(&ownership) {
                Ok(bytes) => tracing::info!(
                    event = "retention.instance_reclaimed",
                    reclaimed_bytes = bytes,
                    "reclaimed an abandoned instance recording cache"
                ),
                Err(error) => tracing::warn!(
                    event = "retention.instance_reclaim_failed",
                    error = %error.message.as_str(),
                    "could not reclaim an abandoned instance recording cache"
                ),
            },
            Err(error) => tracing::warn!(
                event = "retention.instance_lock_failed",
                error = %error.message.as_str(),
                "could not test an instance root for abandonment"
            ),
        }
    }
}

/// Resolves the retention lifecycle from the environment.
///
/// Age-out is on by default: a store with no age policy accumulates until it
/// reaches the budget wall and then stays there. Setting the max age to `0`
/// disables expiry explicitly for callers who want size-only retention.
fn configured_retention(budget: DiskBudgetBytes) -> Result<RetentionLifecycle> {
    let max_age = match std::env::var_os("KROMETRAIL_RETENTION_MAX_AGE_SECS") {
        Some(value) => {
            let seconds = value
                .to_str()
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or_else(invalid_retention_max_age)?;
            (seconds != 0).then(|| std::time::Duration::from_secs(seconds))
        }
        None => Some(krometrail_core::DEFAULT_RETENTION_MAX_AGE),
    };
    RetentionLifecycle::new(
        budget,
        max_age,
        krometrail_core::DEFAULT_TRIM_HIGH_WATER_PERCENT,
        krometrail_core::DEFAULT_ARTIFACT_GRACE,
    )
}

fn invalid_retention_max_age() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new("KROMETRAIL_RETENTION_MAX_AGE_SECS must be a non-negative integer")
            .expect("static retention error is non-empty"),
    )
    .with_recovery(
        NonEmptyText::new(
            "set KROMETRAIL_RETENTION_MAX_AGE_SECS to a decimal second count, or 0 to disable age-out",
        )
        .expect("static retention recovery is non-empty"),
    )
}

fn configured_disk_budget() -> Result<DiskBudgetBytes> {
    parse_disk_budget(std::env::var_os("KROMETRAIL_DISK_BUDGET_BYTES").as_deref())
}

fn parse_disk_budget(value: Option<&std::ffi::OsStr>) -> Result<DiskBudgetBytes> {
    let Some(value) = value else {
        return Ok(DiskBudgetBytes::default());
    };
    let value = value.to_str().ok_or_else(invalid_disk_budget)?;
    let bytes = value.parse::<u64>().map_err(|_| invalid_disk_budget())?;
    DiskBudgetBytes::new(bytes).map_err(|_| invalid_disk_budget())
}

fn invalid_disk_budget() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new("KROMETRAIL_DISK_BUDGET_BYTES must be a positive integer")
            .expect("static budget error is non-empty"),
    )
    .with_recovery(
        NonEmptyText::new("set KROMETRAIL_DISK_BUDGET_BYTES to a positive decimal byte count")
            .expect("static budget recovery is non-empty"),
    )
}

pub(crate) fn data_directory() -> std::path::PathBuf {
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

    let fallback = std::env::temp_dir().join("krometrail-data");
    tracing::warn!("platform data directory unavailable; using the system temporary directory");
    fallback
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_process_clock() -> Arc<dyn MonotonicClock> {
        Arc::new(ProcessMonotonicClock {
            origin: Instant::now(),
        })
    }

    use krometrail_core::PortFuture;
    use std::collections::HashSet;

    struct DiscoveryOnlyFake;

    impl BrowserConnector for DiscoveryOnlyFake {
        fn installations(
            &self,
        ) -> PortFuture<'_, Result<Vec<krometrail_core::BrowserInstallation>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }

        fn managed_profiles(
            &self,
        ) -> PortFuture<'_, Result<Vec<krometrail_core::ManagedProfileSummary>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }

        fn connect(
            &self,
            _request: krometrail_core::BrowserConnectRequest,
        ) -> PortFuture<'_, Result<Arc<dyn krometrail_core::BrowserSessionPort>>> {
            panic!(
                "composition tests must not connect to a browser; doctor \
                 never constructs this runtime"
            );
        }
    }

    #[tokio::test]
    async fn mcp_dependency_projection_shares_runtime_services() {
        let root =
            std::env::temp_dir().join(format!("krometrail-mcp-projection-{}", Uuid::new_v4()));
        let storage =
            open_storage_with_budget(&root, DiskBudgetBytes::default(), test_process_clock())
                .unwrap();
        let ids: Arc<dyn IdSource> = Arc::new(ProcessIdSource);
        let artifact_generation: Arc<dyn ArtifactGeneration> = Arc::new(
            TemporalVisionArtifactService::new(
                Arc::clone(&storage.frames),
                Arc::clone(&storage.artifacts),
                Arc::clone(&ids),
                ArtifactWorkLimits::default(),
            )
            .unwrap(),
        );
        let progressive_evidence: Arc<dyn ProgressiveEvidence> =
            Arc::new(ProgressiveEvidenceService::new(
                Arc::clone(&storage.store) as Arc<dyn ProgressiveEvidenceStore>,
                Arc::clone(&artifact_generation),
            ));
        let temporal_debug_bundles: Arc<dyn TemporalDebugBundles> = Arc::new(
            TemporalDebugBundleService::new(
                Arc::clone(&storage.temporal_queries),
                Arc::clone(&storage.store) as Arc<dyn TemporalDebugEvidenceStore>,
                Arc::clone(&artifact_generation),
                Arc::clone(&storage.temporal_context),
                BundleWorkLimits::default(),
            )
            .unwrap(),
        );
        let range_handles: Arc<dyn ResolvedRangeHandles> = Arc::new(
            ProcessResolvedRangeHandles::new(Arc::clone(&ids), Arc::clone(&storage.frames)),
        );
        let browser: Arc<dyn BrowserConnector> = Arc::new(DiscoveryOnlyFake);
        let dependencies = RuntimeDependencies {
            browser: Arc::clone(&browser),
            frames: Arc::clone(&storage.frames),
            ids,
            temporal_debug_bundles,
            progressive_evidence,
            temporal_context: Arc::clone(&storage.temporal_context),
            range_handles,
            artifacts: Arc::clone(&storage.artifacts),
            diagnostics: DiagnosticContext::default(),
            #[cfg(feature = "qualification-support")]
            clock: test_process_clock(),
            #[cfg(feature = "qualification-support")]
            wall_clock: Arc::new(SystemWallClock),
            #[cfg(feature = "qualification-support")]
            recording: Arc::clone(&storage.recording),
            #[cfg(feature = "qualification-support")]
            retention: Arc::clone(&storage.retention),
            #[cfg(feature = "qualification-support")]
            timeline: Arc::clone(&storage.timeline),
            #[cfg(feature = "qualification-support")]
            catalog: Arc::clone(&storage.catalog),
            #[cfg(feature = "qualification-support")]
            gaps: Arc::clone(&storage.gaps),
            #[cfg(feature = "qualification-support")]
            temporal_queries: Arc::clone(&storage.temporal_queries),
            #[cfg(feature = "qualification-support")]
            artifact_generation: Arc::clone(&artifact_generation),
        };
        let mcp_dependencies = dependencies.mcp_dependencies(None);
        assert!(Arc::ptr_eq(
            &mcp_dependencies.browser,
            &dependencies.browser
        ));
        assert!(Arc::ptr_eq(
            &mcp_dependencies.temporal_context,
            &dependencies.temporal_context,
        ));
        assert!(Arc::ptr_eq(
            &mcp_dependencies.progressive_evidence,
            &dependencies.progressive_evidence,
        ));
        assert!(Arc::ptr_eq(
            &mcp_dependencies.temporal_debug_bundles,
            &dependencies.temporal_debug_bundles,
        ));
        assert!(Arc::ptr_eq(
            &mcp_dependencies.range_handles,
            &dependencies.range_handles,
        ));
        drop(dependencies);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn storage_composition_shares_one_store_and_fails_on_unusable_paths() {
        let root = std::env::temp_dir().join(format!("krometrail-storage-test-{}", Uuid::new_v4()));
        let storage =
            open_storage_with_budget(&root, DiskBudgetBytes::default(), test_process_clock())
                .unwrap();
        let concrete = Arc::as_ptr(&storage.store) as *const ();
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.browser_event_sink) as *const (),
            "browser-event writes must use the one recording store",
        );
        let browser_event_source =
            Arc::clone(&storage.store) as Arc<dyn krometrail_core::BrowserEventSource>;
        assert_eq!(
            concrete,
            Arc::as_ptr(&browser_event_source) as *const (),
            "browser-event reads must use the one recording store",
        );
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.temporal_context) as *const (),
            "temporal context must use the one recording store",
        );
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.frames) as *const (),
            "progressive and artifact frame reads must use the one recording store",
        );
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.retention) as *const (),
            "progressive pin operations must use the one recording store",
        );
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.artifacts) as *const (),
            "artifact reads and publication must use the one recording store",
        );
        drop(storage);
        std::fs::remove_dir_all(&root).unwrap();

        let occupied =
            std::env::temp_dir().join(format!("krometrail-storage-file-{}", Uuid::new_v4()));
        std::fs::write(&occupied, b"not a data directory").unwrap();
        assert!(
            open_storage_with_budget(&occupied, DiskBudgetBytes::default(), test_process_clock(),)
                .is_err()
        );
        std::fs::remove_file(occupied).unwrap();
    }

    #[test]
    fn browser_event_composition_follows_the_shared_capability_selection() {
        let defaults = McpConfig::default();
        assert!(defaults.is_enabled(CapabilityId::BrowserEvents));
        assert!(browser_event_config(&defaults).enabled);

        let explicitly_disabled =
            McpConfig::new(vec![CapabilityId::Control, CapabilityId::TemporalVision]).unwrap();
        assert!(explicitly_disabled.is_enabled(CapabilityId::Control));
        assert!(explicitly_disabled.is_enabled(CapabilityId::TemporalVision));
        assert!(!explicitly_disabled.is_enabled(CapabilityId::BrowserEvents));
        assert!(!browser_event_config(&explicitly_disabled).enabled);
    }

    struct CompositionEncoder {
        identity: krometrail_core::VideoEncoderIdentity,
    }

    impl CompositionEncoder {
        fn new() -> Self {
            Self {
                identity: krometrail_core::VideoEncoderIdentity::new(
                    "fixture-ffmpeg",
                    [7; 32],
                    "libx264",
                    "fixture-adapter",
                    "fixture-policy",
                )
                .unwrap(),
            }
        }
    }

    impl TemporalVideoEncoder for CompositionEncoder {
        fn identity(&self) -> &krometrail_core::VideoEncoderIdentity {
            &self.identity
        }

        fn encode(
            &self,
            _request: krometrail_core::VideoEncodeRequest,
            _context: krometrail_core::VideoEncodingContext,
        ) -> PortFuture<'_, Result<krometrail_core::VideoEncodedClip>> {
            panic!("composition test must not encode")
        }
    }

    #[test]
    fn one_startup_result_controls_video_capability_and_service_construction() {
        let root =
            std::env::temp_dir().join(format!("krometrail-video-composition-{}", Uuid::new_v4()));
        let storage =
            open_storage_with_budget(&root, DiskBudgetBytes::default(), test_process_clock())
                .unwrap();
        let ids: Arc<dyn IdSource> = Arc::new(ProcessIdSource);
        let artifact_generation: Arc<dyn ArtifactGeneration> = Arc::new(
            TemporalVisionArtifactService::new(
                Arc::clone(&storage.frames),
                Arc::clone(&storage.artifacts),
                Arc::clone(&ids),
                ArtifactWorkLimits::default(),
            )
            .unwrap(),
        );
        let range_handles: Arc<dyn ResolvedRangeHandles> = Arc::new(
            ProcessResolvedRangeHandles::new(Arc::clone(&ids), Arc::clone(&storage.frames)),
        );
        let dependencies = RuntimeDependencies {
            browser: Arc::new(DiscoveryOnlyFake),
            frames: Arc::clone(&storage.frames),
            ids,
            progressive_evidence: Arc::new(ProgressiveEvidenceService::new(
                Arc::clone(&storage.store) as Arc<dyn ProgressiveEvidenceStore>,
                Arc::clone(&artifact_generation),
            )),
            temporal_debug_bundles: Arc::new(
                TemporalDebugBundleService::new(
                    Arc::clone(&storage.store) as Arc<dyn TemporalQuery>,
                    Arc::clone(&storage.store) as Arc<dyn TemporalDebugEvidenceStore>,
                    Arc::clone(&artifact_generation),
                    Arc::clone(&storage.store) as Arc<dyn TemporalContextQuery>,
                    BundleWorkLimits::default(),
                )
                .unwrap(),
            ),
            temporal_context: Arc::clone(&storage.temporal_context),
            range_handles,
            artifacts: Arc::clone(&storage.artifacts),
            diagnostics: DiagnosticContext::default(),
            #[cfg(feature = "qualification-support")]
            clock: test_process_clock(),
            #[cfg(feature = "qualification-support")]
            wall_clock: Arc::new(SystemWallClock),
            #[cfg(feature = "qualification-support")]
            recording: Arc::clone(&storage.recording),
            #[cfg(feature = "qualification-support")]
            retention: Arc::clone(&storage.retention),
            #[cfg(feature = "qualification-support")]
            timeline: Arc::clone(&storage.timeline),
            #[cfg(feature = "qualification-support")]
            catalog: Arc::clone(&storage.catalog),
            #[cfg(feature = "qualification-support")]
            gaps: Arc::clone(&storage.gaps),
            #[cfg(feature = "qualification-support")]
            temporal_queries: Arc::clone(&storage.temporal_queries),
            #[cfg(feature = "qualification-support")]
            artifact_generation: Arc::clone(&artifact_generation),
        };

        let (unavailable_config, unavailable_service) =
            compose_temporal_video(&dependencies, None).unwrap();
        assert!(!unavailable_config.is_enabled(CapabilityId::TemporalVideo));
        assert!(unavailable_service.is_none());

        let encoder: Arc<dyn TemporalVideoEncoder> = Arc::new(CompositionEncoder::new());
        let (qualified_config, qualified_service) =
            compose_temporal_video(&dependencies, Some(encoder)).unwrap();
        assert!(qualified_config.is_enabled(CapabilityId::TemporalVideo));
        let qualified_service = qualified_service.expect("qualified startup builds the service");
        let mcp = dependencies.mcp_dependencies(Some(Arc::clone(&qualified_service)));
        assert!(Arc::ptr_eq(
            mcp.temporal_video.as_ref().unwrap(),
            &qualified_service
        ));

        drop(dependencies);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disk_budget_configuration_defaults_and_rejects_invalid_boundaries() {
        assert_eq!(parse_disk_budget(None).unwrap(), DiskBudgetBytes::default());
        assert_eq!(
            parse_disk_budget(Some(std::ffi::OsStr::new("12345")))
                .unwrap()
                .get(),
            12_345
        );
        for invalid in ["", "0", "-1", "1.5", "ten"] {
            let error = parse_disk_budget(Some(std::ffi::OsStr::new(invalid))).unwrap_err();
            assert_eq!(error.code, ErrorCode::InvalidInput);
            assert_eq!(
                error.message.as_str(),
                "KROMETRAIL_DISK_BUDGET_BYTES must be a positive integer"
            );
        }
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

    #[tokio::test]
    async fn bundle_composition_shares_one_store_and_one_artifact_service() {
        let root =
            std::env::temp_dir().join(format!("krometrail-bundle-composition-{}", Uuid::new_v4()));
        let storage =
            open_storage_with_budget(&root, DiskBudgetBytes::default(), test_process_clock())
                .unwrap();
        let concrete = Arc::as_ptr(&storage.store) as *const ();
        // Every store projection the bundle service receives points at the one
        // concrete RecordingStore.
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.temporal_queries) as *const (),
            "bundle temporal query must use the one recording store",
        );
        assert_eq!(
            concrete,
            Arc::as_ptr(&storage.temporal_context) as *const (),
            "bundle temporal context must use the one recording store",
        );
        let bundle_evidence: Arc<dyn TemporalDebugEvidenceStore> =
            Arc::clone(&storage.store) as Arc<dyn TemporalDebugEvidenceStore>;
        assert_eq!(
            concrete,
            Arc::as_ptr(&bundle_evidence) as *const (),
            "bundle timeline/interaction evidence must use the one recording store",
        );
        // The shared artifact service is constructed once and cloned to both
        // progressive and bundle paths.
        let ids: Arc<dyn IdSource> = Arc::new(ProcessIdSource);
        let artifact_generation: Arc<dyn ArtifactGeneration> = Arc::new(
            TemporalVisionArtifactService::new(
                Arc::clone(&storage.frames),
                Arc::clone(&storage.artifacts),
                Arc::clone(&ids),
                ArtifactWorkLimits::default(),
            )
            .unwrap(),
        );
        let progressive = ProgressiveEvidenceService::new(
            Arc::clone(&storage.store) as Arc<dyn ProgressiveEvidenceStore>,
            Arc::clone(&artifact_generation),
        );
        let bundle = TemporalDebugBundleService::new(
            Arc::clone(&storage.temporal_queries),
            bundle_evidence,
            Arc::clone(&artifact_generation),
            Arc::clone(&storage.temporal_context),
            BundleWorkLimits::default(),
        )
        .unwrap();
        // Interrogate what each service retained, not the local Arc it was built from: a
        // constructor that dropped or substituted the shared generator must fail here.
        assert!(
            Arc::ptr_eq(progressive.artifact_generation(), &artifact_generation),
            "progressive evidence must resolve artifacts through the one artifact service",
        );
        assert!(
            Arc::ptr_eq(bundle.artifact_generation(), &artifact_generation),
            "debug bundles must resolve artifacts through the one artifact service",
        );
        drop(storage);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn blocked_artifact_generation_permits_frame_persistence() {
        use krometrail_core::{
            ArtifactGenerationContext, ArtifactGenerationRequest, ArtifactGenerationResult,
            CaptureOrdinal, DeviceScaleFactor, EncodedFrame, FrameId, ObservedTime,
            OrientationPolicy, PixelDimensions, PortFuture, SessionRange, SessionTime,
            TemporalDebugBundleContext, TemporalDebugBundleRequest, TemporalRangeAnchor,
            VisualEpoch,
        };
        use tokio::sync::Notify;

        let root = std::env::temp_dir().join(format!("krometrail-bundle-gate-{}", Uuid::new_v4()));
        let storage =
            open_storage_with_budget(&root, DiskBudgetBytes::default(), test_process_clock())
                .unwrap();
        let session = krometrail_core::SessionId::from_uuid(Uuid::from_u128(1));
        let target = krometrail_core::TargetId::from_uuid(Uuid::from_u128(2));

        // Append one frame so range resolution succeeds.
        let frame_id = FrameId::from_uuid(Uuid::from_u128(10));
        let metadata = krometrail_core::CapturedFrame::new(
            frame_id,
            session,
            target,
            CaptureOrdinal::new(1).unwrap(),
            None,
            ObservedTime::from_nanos(2),
            SessionTime::from_nanos(1),
            krometrail_core::ImageFormat::Png,
            PixelDimensions::new(2, 2).unwrap(),
            PixelDimensions::new(2, 2).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap();
        let frame = EncodedFrame::new(metadata, vec![0_u8, 1, 2]).unwrap();
        storage.recording.append_frame(frame).await.unwrap();

        // Spy artifact service that blocks during generate.
        let block = Arc::new(Notify::new());
        let reached = Arc::new(Notify::new());
        struct BlockingGeneration {
            block: Arc<Notify>,
            reached: Arc<Notify>,
            range: krometrail_core::ResolvedRange,
            frame_id: FrameId,
        }
        impl ArtifactGeneration for BlockingGeneration {
            fn generate(
                &self,
                _request: ArtifactGenerationRequest,
                _ctx: ArtifactGenerationContext,
            ) -> PortFuture<'_, krometrail_core::Result<ArtifactGenerationResult>> {
                let block = Arc::clone(&self.block);
                let reached = Arc::clone(&self.reached);
                let range = self.range.clone();
                let frame_id = self.frame_id;
                Box::pin(async move {
                    reached.notify_one();
                    block.notified().await;
                    Ok(ArtifactGenerationResult {
                        range,
                        epochs: vec![VisualEpoch {
                            index: 0,
                            frame_ids: vec![frame_id],
                            image: PixelDimensions::new(2, 2).unwrap(),
                            viewport: PixelDimensions::new(2, 2).unwrap(),
                            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
                        }],
                        outcomes: vec![],
                        artifact_grace_overridden: false,
                    })
                })
            }
        }
        // Resolve the range once to feed the spy's result.
        let resolved = storage
            .temporal_queries
            .resolve_range(
                krometrail_core::TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
                    scope: krometrail_core::IntervalAnchorScope::new(session, target),
                    range: SessionRange::new(
                        SessionTime::from_nanos(1),
                        SessionTime::from_nanos(1),
                    )
                    .unwrap(),
                })
                .unwrap(),
            )
            .await
            .unwrap();
        let spy_generation = Arc::new(BlockingGeneration {
            block: Arc::clone(&block),
            reached: Arc::clone(&reached),
            range: resolved.clone(),
            frame_id,
        });

        // Spy context query that returns a minimal context without touching the store.
        struct SpyContext {
            range: krometrail_core::ResolvedRange,
        }
        impl krometrail_core::TemporalContextQuery for SpyContext {
            fn context(
                &self,
                _request: krometrail_core::TemporalContextRequest,
            ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::TemporalContext>>
            {
                let range = self.range.clone();
                Box::pin(async move { Ok(minimal_temporal_context(range)) })
            }
        }
        let spy_context = Arc::new(SpyContext {
            range: resolved.clone(),
        });

        let bundle_service = TemporalDebugBundleService::new(
            Arc::clone(&storage.temporal_queries),
            Arc::clone(&storage.store) as Arc<dyn TemporalDebugEvidenceStore>,
            Arc::clone(&spy_generation) as Arc<dyn ArtifactGeneration>,
            Arc::clone(&spy_context) as Arc<dyn krometrail_core::TemporalContextQuery>,
            BundleWorkLimits::default(),
        )
        .unwrap();

        let request = TemporalDebugBundleRequest::new(
            krometrail_core::TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
                scope: krometrail_core::IntervalAnchorScope::new(session, target),
                range: SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(1))
                    .unwrap(),
            })
            .unwrap(),
            vec![],
            OrientationPolicy::Include,
            krometrail_core::BundleEpochScope::Anchor,
        )
        .unwrap();

        // Start the bundle in a background task.
        let handle = tokio::spawn(async move {
            bundle_service
                .bundle(request, TemporalDebugBundleContext::default())
                .await
        });

        // Wait for artifact generation to start — all store reads have completed
        // and the mutation gate has been released.
        reached.notified().await;

        // While artifact generation is blocked, append another frame. This must
        // succeed because the recording mutation gate is not held across visual work.
        let second_metadata = krometrail_core::CapturedFrame::new(
            FrameId::from_uuid(Uuid::from_u128(11)),
            session,
            target,
            CaptureOrdinal::new(2).unwrap(),
            None,
            ObservedTime::from_nanos(4),
            SessionTime::from_nanos(3),
            krometrail_core::ImageFormat::Png,
            PixelDimensions::new(2, 2).unwrap(),
            PixelDimensions::new(2, 2).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            vec![],
        )
        .unwrap();
        let second_frame = EncodedFrame::new(second_metadata, vec![3_u8, 4, 5]).unwrap();
        let append_result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            storage.recording.append_frame(second_frame),
        )
        .await;
        assert!(
            append_result.is_ok(),
            "frame persistence must acquire the mutation gate during blocked artifact work"
        );
        assert!(append_result.unwrap().is_ok());

        // Unblock artifact generation and verify the bundle completes.
        block.notify_one();
        let _bundle = handle.await.unwrap().unwrap();

        drop(storage);
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn minimal_temporal_context(
        range: krometrail_core::ResolvedRange,
    ) -> krometrail_core::TemporalContext {
        use krometrail_core::{
            BrowserEventContext, CaptureGapSummary, CaptureQuality, CaptureStatusEvidence,
            FramePoint,
        };
        krometrail_core::TemporalContext {
            range,
            capture_quality: CaptureQuality {
                requested_range: krometrail_core::SessionRange::new(
                    krometrail_core::SessionTime::ZERO,
                    krometrail_core::SessionTime::from_nanos(1),
                )
                .unwrap(),
                retained_range: krometrail_core::SessionRange::new(
                    krometrail_core::SessionTime::ZERO,
                    krometrail_core::SessionTime::from_nanos(1),
                )
                .unwrap(),
                frame_count: 1,
                first_frame: FramePoint {
                    frame_id: krometrail_core::FrameId::from_uuid(Uuid::from_u128(10)),
                    capture_ordinal: krometrail_core::CaptureOrdinal::new(1).unwrap(),
                    session_time: krometrail_core::SessionTime::from_nanos(1),
                },
                last_frame: FramePoint {
                    frame_id: krometrail_core::FrameId::from_uuid(Uuid::from_u128(10)),
                    capture_ordinal: krometrail_core::CaptureOrdinal::new(1).unwrap(),
                    session_time: krometrail_core::SessionTime::from_nanos(1),
                },
                cadence: None,
                frame_warnings: vec![],
                gaps: vec![],
                gap_summary: CaptureGapSummary {
                    gap_count: 0,
                    covered_duration_nanos: 0,
                    known_missing_frames: 0,
                    has_unknown_missing_estimate: false,
                },
                retention_warnings: vec![],
                epochs: vec![],
                capture_status: CaptureStatusEvidence {
                    at_range_start: None,
                    at_range_end: None,
                    transitions: vec![],
                },
                warnings: vec![],
            },
            browser_events: BrowserEventContext {
                effective_range: krometrail_core::SessionRange::new(
                    krometrail_core::SessionTime::ZERO,
                    krometrail_core::SessionTime::from_nanos(1),
                )
                .unwrap(),
                matched_count: 0,
                returned_count: 0,
                events: vec![],
                next_cursor: None,
                collection_gaps: vec![],
                unavailable_ranges: vec![],
                warnings: vec![],
            },
        }
    }
}
