//! Test-only composition and lifecycle boundary for authorized live qualification.
#![allow(dead_code)]
//!
//! This module is compiled only for tests with the explicit `qualification-support` feature. It
//! deliberately owns no product command and never selects the operator data/profile defaults.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use super::{
    ArtifactWorkLimits, BundleWorkLimits, McpConfig, ProgressiveEvidenceService,
    RuntimeDependencies, SystemWallClock, TemporalDebugBundleService, TemporalDebugEvidenceStore,
    TemporalVisionArtifactService, browser_event_config, open_storage_with_budget,
};
use krometrail_cdp::qualification_support::{ChromeViewport, FixtureServer, real_browser_lock};
use krometrail_cdp::{LauncherConfig, ProductionBrowserConnector, SystemChromeLauncher};
use krometrail_core::{
    BrowserConnector, BrowserInstallation, BrowserProduct, DiskBudgetBytes, ErrorCode, IdSource,
    KrometrailError, MonotonicClock, NonEmptyText, Result,
};
use krometrail_store::RecordingStore;
use temporal_evaluation::{
    EvaluationStatus, FailureRecord, LiveQualification, RunFailureCode, RunManifest,
};

mod barriers;
mod capture;
mod control;
mod fixture_observation;
mod latency;
mod recovery;
mod resource_usage;
mod retention;

pub(crate) use capture::qualification_capture_config;

pub const LIVE_CAPTURE_ENV: &str = "KROMETRAIL_LIVE_CAPTURE_EVALUATION";
pub const REAL_BROWSER_ENV: &str = "KROMETRAIL_REAL_CHROME_TESTS";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptInDecision {
    Disabled,
    Authorized,
}

impl OptInDecision {
    pub fn from_environment() -> Self {
        if std::env::var(REAL_BROWSER_ENV).as_deref() == Ok("1")
            && std::env::var(LIVE_CAPTURE_ENV).as_deref() == Ok("1")
        {
            Self::Authorized
        } else {
            Self::Disabled
        }
    }

    pub const fn from_flags(real_browser_authorized: bool, live_capture_enabled: bool) -> Self {
        if real_browser_authorized && live_capture_enabled {
            Self::Authorized
        } else {
            Self::Disabled
        }
    }
}

#[derive(Clone, Debug)]
pub struct LiveQualificationConfig {
    pub output_root: PathBuf,
    pub browser_product: BrowserProduct,
    pub optional_browser: bool,
    pub retention_budget: DiskBudgetBytes,
}

impl Default for LiveQualificationConfig {
    fn default() -> Self {
        let output_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/temporal-evaluation/live");
        Self {
            output_root,
            browser_product: BrowserProduct::Chrome,
            optional_browser: false,
            retention_budget: DiskBudgetBytes::default(),
        }
    }
}

impl LiveQualificationConfig {
    pub fn data_root(&self) -> PathBuf {
        self.output_root.join("store")
    }

    pub fn profile_root(&self) -> PathBuf {
        self.output_root.join("profiles")
    }

    pub fn output_path(&self) -> PathBuf {
        self.output_root.join("run-manifest.json")
    }

    pub fn uses_optional_linux_chromium(&self) -> bool {
        self.optional_browser
            && self.browser_product == BrowserProduct::Chromium
            && cfg!(target_os = "linux")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserPreflight {
    Ready(BrowserInstallation),
    Blocked(FailureRecord),
    Skipped(FailureRecord),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreflightResult {
    pub decision: OptInDecision,
    pub browser: Option<BrowserPreflight>,
}

/// Resources that must exist before any managed launch. The server has an observable readiness
/// barrier and the lock remains held until this guard is dropped; the actual profile directory is
/// then created by the existing `SystemChromeLauncher` below this test-owned root.
pub struct QualificationLifecycle {
    browser_lock: Option<tokio::sync::MutexGuard<'static, ()>>,
    server: FixtureServer,
    profile_root: PathBuf,
    fixture_url: String,
    viewport: ChromeViewport,
}

impl QualificationLifecycle {
    pub async fn start(
        config: &LiveQualificationConfig,
        preflight: &PreflightResult,
    ) -> Result<Self> {
        if preflight.decision != OptInDecision::Authorized
            || !matches!(preflight.browser, Some(BrowserPreflight::Ready(_)))
        {
            return Err(live_error(
                ErrorCode::InvalidLifecycleTransition,
                "qualification lifecycle requires an authorized ready preflight",
            ));
        }
        let browser_lock = real_browser_lock().await;
        let mut server = FixtureServer::start().map_err(|_| {
            live_error(
                ErrorCode::BrowserLaunchFailed,
                "qualification fixture server could not become ready",
            )
        })?;
        let fixture_url = server.url();
        fs::create_dir_all(config.profile_root()).map_err(|_| {
            server.shutdown();
            live_error(
                ErrorCode::PersistenceFailed,
                "qualification profile boundary could not be prepared",
            )
        })?;
        Ok(Self {
            browser_lock: Some(browser_lock),
            fixture_url,
            server,
            profile_root: config.profile_root(),
            viewport: ChromeViewport::LIVE,
        })
    }

    pub fn fixture_url(&self) -> &str {
        &self.fixture_url
    }

    pub(crate) fn lock_held(&self) -> bool {
        self.browser_lock.is_some()
    }

    pub(crate) fn server_ready(&self) -> bool {
        !self.fixture_url.is_empty()
    }

    pub fn temporal_benchmark_url(&self, case_id: &str, duration_ms: u16) -> String {
        self.server.temporal_benchmark_url(case_id, duration_ms)
    }

    pub fn profile_root(&self) -> &Path {
        &self.profile_root
    }

    pub const fn viewport(&self) -> ChromeViewport {
        self.viewport
    }

    pub fn cleanup(mut self) -> CleanupObservation {
        self.server.shutdown();
        let profile_deleted = remove_tree(&self.profile_root);
        self.browser_lock.take();
        CleanupObservation {
            server_stopped: true,
            profile_deleted,
            store_flushed: true,
            lock_released: true,
            output_finalized: false,
            remaining_managed_resources: u64::from(!profile_deleted),
        }
    }
}

impl Drop for QualificationLifecycle {
    fn drop(&mut self) {
        self.server.shutdown();
        let _ = remove_tree(&self.profile_root);
        self.browser_lock.take();
    }
}

impl PreflightResult {
    fn disabled() -> Self {
        Self {
            decision: OptInDecision::Disabled,
            browser: None,
        }
    }
}

/// Browser discovery is delayed until both gates have passed. The lock serializes discovery and
/// launch against the existing real-browser tests; no profile, listener, or output is touched by
/// the disabled path.
pub async fn run_preflight(config: LiveQualificationConfig) -> Result<PreflightResult> {
    run_preflight_with_decision(config, OptInDecision::from_environment()).await
}

async fn run_preflight_with_decision(
    config: LiveQualificationConfig,
    decision: OptInDecision,
) -> Result<PreflightResult> {
    if decision == OptInDecision::Disabled {
        return Ok(PreflightResult::disabled());
    }

    let _browser_lock = krometrail_cdp::qualification_support::real_browser_lock().await;
    let installation = krometrail_cdp::discover_installations(None)
        .into_iter()
        .find(|installation| installation.product == config.browser_product);
    let Some(installation) = installation else {
        let failure = browser_failure(&config);
        let browser = if config.uses_optional_linux_chromium() {
            BrowserPreflight::Skipped(failure)
        } else {
            BrowserPreflight::Blocked(failure)
        };
        return Ok(PreflightResult {
            decision,
            browser: Some(browser),
        });
    };
    Ok(PreflightResult {
        decision,
        browser: Some(BrowserPreflight::Ready(installation)),
    })
}

fn browser_failure(config: &LiveQualificationConfig) -> FailureRecord {
    let optional = config.uses_optional_linux_chromium();
    FailureRecord {
        code: if optional {
            RunFailureCode::OptionalUnavailable
        } else {
            RunFailureCode::Unavailable
        },
        phase: "browser_preflight".into(),
        reason: if optional {
            "optional Linux Chromium is unavailable".into()
        } else {
            "required browser installation is unavailable".into()
        },
        recovery: if optional {
            "install the optional Linux Chromium configuration before collecting it".into()
        } else {
            "install a supported local Chrome or Chromium browser and retry".into()
        },
        retryable: true,
    }
}

/// The temporary graph returned to later capture stories. It deliberately exposes the concrete
/// store only as an observation seam while all operation authorities remain their core ports.
pub struct QualificationRuntime {
    pub(crate) dependencies: RuntimeDependencies,
    pub(crate) store: Arc<RecordingStore>,
    pub(crate) data_root: PathBuf,
    pub(crate) profile_root: PathBuf,
    pub(crate) output_root: PathBuf,
    pub(crate) recovery: krometrail_store::RecoveryReport,
}

impl QualificationRuntime {
    pub fn store(&self) -> &Arc<RecordingStore> {
        &self.store
    }

    pub(crate) fn dependencies(&self) -> &RuntimeDependencies {
        &self.dependencies
    }

    /// Drop authorities before deleting the temporary store. Repeating cleanup is harmless and
    /// reports remaining managed resources instead of pretending deletion succeeded.
    pub fn cleanup(self) -> CleanupObservation {
        let QualificationRuntime {
            dependencies,
            store,
            data_root,
            profile_root,
            output_root,
            recovery: _,
        } = self;
        drop(dependencies);
        drop(store);
        let data_removed = remove_tree(&data_root);
        let profile_removed = remove_tree(&profile_root);
        let output_removed = remove_tree(&output_root);
        CleanupObservation {
            server_stopped: true,
            profile_deleted: profile_removed,
            store_flushed: data_removed,
            lock_released: true,
            output_finalized: false,
            remaining_managed_resources: u64::from(!data_removed)
                + u64::from(!profile_removed)
                + u64::from(!output_removed),
        }
    }
}

/// Compose one production-authority graph without consulting `data_directory()` or the operator
/// profile root. Callers must have completed the two-gate preflight first.
pub fn build_qualification_runtime(
    config: &LiveQualificationConfig,
    decision: OptInDecision,
) -> Result<QualificationRuntime> {
    if decision != OptInDecision::Authorized {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live qualification requires both explicit opt-in gates",
        ));
    }
    let storage = open_storage_with_budget(&config.data_root(), config.retention_budget)?;
    let clock: Arc<dyn MonotonicClock> = Arc::new(super::ProcessMonotonicClock {
        origin: Instant::now(),
    });
    let ids: Arc<dyn IdSource> = Arc::new(super::ProcessIdSource);
    let mcp_config = McpConfig::default();
    let artifact_generation: Arc<dyn krometrail_core::ArtifactGeneration> =
        Arc::new(TemporalVisionArtifactService::new(
            Arc::clone(&storage.frames),
            Arc::clone(&storage.artifacts),
            Arc::clone(&ids),
            ArtifactWorkLimits::default(),
        )?);
    let progressive_evidence: Arc<dyn krometrail_core::ProgressiveEvidence> =
        Arc::new(ProgressiveEvidenceService::new(
            Arc::clone(&storage.store) as Arc<dyn krometrail_core::ProgressiveEvidenceStore>,
            Arc::clone(&artifact_generation),
        ));
    let temporal_debug_bundles: Arc<dyn krometrail_core::TemporalDebugBundles> =
        Arc::new(TemporalDebugBundleService::new(
            Arc::clone(&storage.temporal_queries),
            Arc::clone(&storage.store) as Arc<dyn TemporalDebugEvidenceStore>,
            Arc::clone(&artifact_generation),
            Arc::clone(&storage.temporal_context),
            BundleWorkLimits::default(),
        )?);
    let browser: Arc<dyn BrowserConnector> = Arc::new(
        ProductionBrowserConnector::new(
            Arc::new(SystemChromeLauncher::new(LauncherConfig {
                profile_root: config.profile_root(),
                startup_timeout: Duration::from_secs(30),
                shutdown_timeout: Duration::from_secs(5),
            })),
            Arc::new(
                krometrail_cdp::transport::CdpkitTransportFactory::new()
                    .with_command_timeout(Duration::from_secs(5)),
            ),
        )
        .with_capture(
            Arc::clone(&clock),
            Arc::clone(&ids),
            Arc::clone(&storage.recording),
            Arc::clone(&storage.retention),
            qualification_capture_config(),
        )
        .with_browser_events(
            Arc::clone(&clock),
            Arc::clone(&ids),
            Arc::clone(&storage.browser_event_sink),
            browser_event_config(&mcp_config),
        )
        .with_interaction_evidence(
            Arc::clone(&storage.store) as Arc<dyn krometrail_core::InteractionEvidenceSink>
        ),
    );
    Ok(QualificationRuntime {
        dependencies: RuntimeDependencies {
            clock,
            wall_clock: Arc::new(SystemWallClock),
            ids,
            browser,
            recording: storage.recording,
            retention: storage.retention,
            timeline: storage.timeline,
            catalog: storage.catalog,
            gaps: storage.gaps,
            frames: storage.frames,
            temporal_queries: storage.temporal_queries,
            temporal_context: storage.temporal_context,
            artifact_generation,
            progressive_evidence,
            temporal_debug_bundles,
            mcp_config,
        },
        store: storage.store,
        data_root: config.data_root(),
        profile_root: config.profile_root(),
        output_root: config.output_root.clone(),
        recovery: storage.recovery,
    })
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CleanupObservation {
    pub server_stopped: bool,
    pub profile_deleted: bool,
    pub store_flushed: bool,
    pub lock_released: bool,
    pub output_finalized: bool,
    pub remaining_managed_resources: u64,
}

impl CleanupObservation {
    pub const fn is_clean(&self) -> bool {
        self.server_stopped
            && self.profile_deleted
            && self.store_flushed
            && self.lock_released
            && self.remaining_managed_resources == 0
    }

    fn apply(&self, qualification: &mut LiveQualification) {
        qualification.cleanup = temporal_evaluation::CleanupQualificationMeasurements {
            server_stopped: self.server_stopped,
            profile_deleted: self.profile_deleted,
            store_flushed: self.store_flushed,
            lock_released: self.lock_released,
            output_finalized: true,
            remaining_managed_resources: self.remaining_managed_resources,
        };
        if !self.is_clean() {
            if let Some(gate) = qualification
                .gates
                .iter_mut()
                .find(|gate| gate.gate == temporal_evaluation::QualificationGateId::Cleanup)
            {
                gate.status = EvaluationStatus::Inconclusive;
                gate.failure = Some(FailureRecord {
                    code: RunFailureCode::Cleanup,
                    phase: "cleanup".into(),
                    reason: "managed qualification resources remain after cleanup".into(),
                    recovery: "remove the remaining managed resources before retrying".into(),
                    retryable: true,
                });
            }
        }
    }
}

/// Finalize exactly one existing `RunManifest` after cleanup. The path is fixed to the ignored
/// qualification boundary and never enters the manifest.
pub async fn finalize_manifest(
    mut run: RunManifest,
    cleanup: CleanupObservation,
) -> Result<PathBuf> {
    if OptInDecision::from_environment() != OptInDecision::Authorized {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live manifest finalization requires both explicit opt-in gates",
        ));
    }
    finalize_manifest_at(
        &mut run,
        cleanup,
        &LiveQualificationConfig::default().output_path(),
    )
}

fn finalize_manifest_at(
    run: &mut RunManifest,
    cleanup: CleanupObservation,
    path: &Path,
) -> Result<PathBuf> {
    let Some(qualification) = run.qualification.as_mut() else {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "live manifest finalization requires qualification measurements",
        ));
    };
    cleanup.apply(qualification);
    if !cleanup.is_clean() && run.status == EvaluationStatus::Pass {
        run.status = EvaluationStatus::Inconclusive;
        run.failure = Some(FailureRecord {
            code: RunFailureCode::Cleanup,
            phase: "cleanup".into(),
            reason: "qualification cleanup did not complete".into(),
            recovery: "remove the remaining managed resources before retrying".into(),
            retryable: true,
        });
    }
    run.validate().map_err(|error| {
        live_error(
            ErrorCode::InvalidInput,
            "live manifest failed final validation",
        )
        .with_recovery(NonEmptyText::new(error.to_string()).expect("contract error text"))
    })?;
    let bytes = run.canonical_bytes().map_err(|_| {
        live_error(
            ErrorCode::PersistenceFailed,
            "live manifest could not be canonicalized",
        )
    })?;
    let parent = path.parent().ok_or_else(|| {
        live_error(
            ErrorCode::PersistenceFailed,
            "live output boundary is invalid",
        )
    })?;
    fs::create_dir_all(parent).map_err(|_| {
        live_error(
            ErrorCode::PersistenceFailed,
            "live output boundary could not be prepared",
        )
    })?;
    let temporary = path.with_extension("json.partial");
    fs::write(&temporary, bytes).map_err(|_| {
        live_error(
            ErrorCode::PersistenceFailed,
            "live manifest could not be written",
        )
    })?;
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        live_error(
            ErrorCode::PersistenceFailed,
            "live manifest could not be finalized",
        )
    })?;
    Ok(path.to_owned())
}

fn remove_tree(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    fs::remove_dir_all(path).is_ok()
}

fn live_error(code: ErrorCode, message: &'static str) -> KrometrailError {
    KrometrailError::new(
        code,
        NonEmptyText::new(message).expect("static live qualification error"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_gates_are_required_before_authorization() {
        assert_eq!(
            OptInDecision::from_flags(false, false),
            OptInDecision::Disabled
        );
        assert_eq!(
            OptInDecision::from_flags(true, false),
            OptInDecision::Disabled
        );
        assert_eq!(
            OptInDecision::from_flags(false, true),
            OptInDecision::Disabled
        );
        assert_eq!(
            OptInDecision::from_flags(true, true),
            OptInDecision::Authorized
        );
    }

    #[tokio::test]
    async fn disabled_preflight_has_no_side_effect_path() {
        let root =
            std::env::temp_dir().join(format!("krometrail-live-disabled-{}", std::process::id()));
        let config = LiveQualificationConfig {
            output_root: root.clone(),
            ..LiveQualificationConfig::default()
        };
        let result = run_preflight_with_decision(config, OptInDecision::Disabled)
            .await
            .unwrap();
        assert_eq!(result, PreflightResult::disabled());
        assert!(!root.exists());
    }

    #[tokio::test]
    async fn lifecycle_rejects_disabled_preflight_before_listener_or_profile() {
        let root = std::env::temp_dir().join(format!(
            "krometrail-live-lifecycle-disabled-{}",
            std::process::id()
        ));
        let config = LiveQualificationConfig {
            output_root: root.clone(),
            ..LiveQualificationConfig::default()
        };
        let preflight = PreflightResult::disabled();
        assert!(
            QualificationLifecycle::start(&config, &preflight)
                .await
                .is_err()
        );
        assert!(!root.exists());
    }

    #[test]
    fn cleanup_observation_is_idempotent_and_never_claims_clean_resources() {
        let failed = CleanupObservation {
            remaining_managed_resources: 1,
            ..CleanupObservation::default()
        };
        assert!(!failed.is_clean());
        assert_eq!(failed, failed.clone());
    }

    #[test]
    fn qualification_graph_uses_one_concrete_recording_store() {
        let root =
            std::env::temp_dir().join(format!("krometrail-live-graph-{}", std::process::id()));
        let config = LiveQualificationConfig {
            output_root: root.clone(),
            ..LiveQualificationConfig::default()
        };
        let runtime = build_qualification_runtime(&config, OptInDecision::Authorized).unwrap();
        let concrete = Arc::as_ptr(&runtime.store) as *const ();
        let dependencies = runtime.dependencies();
        assert_eq!(concrete, Arc::as_ptr(&dependencies.recording) as *const ());
        assert_eq!(concrete, Arc::as_ptr(&dependencies.retention) as *const ());
        assert_eq!(concrete, Arc::as_ptr(&dependencies.timeline) as *const ());
        assert_eq!(concrete, Arc::as_ptr(&dependencies.frames) as *const ());
        assert_eq!(
            concrete,
            Arc::as_ptr(&dependencies.temporal_queries) as *const ()
        );
        let cleanup = runtime.cleanup();
        assert!(cleanup.is_clean());
        assert!(!root.exists());
    }

    #[test]
    fn output_root_is_the_only_default_boundary() {
        let config = LiveQualificationConfig::default();
        assert!(
            config
                .output_path()
                .ends_with("target/temporal-evaluation/live/run-manifest.json")
        );
        assert!(config.data_root().starts_with(&config.output_root));
        assert!(config.profile_root().starts_with(&config.output_root));
    }
}
