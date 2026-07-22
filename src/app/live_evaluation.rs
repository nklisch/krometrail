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
    BrowserConnectRequest, BrowserConnector, BrowserInstallation, BrowserProduct, DiskBudgetBytes,
    ErrorCode, IdSource, KrometrailError, LaunchBrowser, ManagedProfile, MonotonicClock,
    NonEmptyText, Result,
};
use krometrail_store::RecordingStore;
use temporal_evaluation::{
    Architecture, BrowserAvailability, EnvironmentIdentity, FailureRecord, Platform,
    PlatformLaneDeclaration, PlatformProfile, QualificationEvidenceMode, RunFailureCode,
    RunManifest,
};

mod barriers;
mod capture;
mod control;
mod fixture_observation;
mod latency;
mod recovery;
mod report;
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
    pub run_id: String,
    pub optional_browser: bool,
    pub retention_budget: DiskBudgetBytes,
    /// Platform identity changes only the declared qualification profile and expected capture
    /// scale; all production authorities remain shared with the normal live path.
    pub platform: Option<PlatformLaneDeclaration>,
}

impl Default for LiveQualificationConfig {
    fn default() -> Self {
        let output_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/temporal-evaluation/live");
        Self {
            output_root,
            browser_product: BrowserProduct::Chrome,
            run_id: format!("run-{}", std::process::id()),
            optional_browser: false,
            retention_budget: DiskBudgetBytes::default(),
            platform: None,
        }
    }
}

impl LiveQualificationConfig {
    pub fn viewport(&self) -> ChromeViewport {
        self.platform
            .map_or(ChromeViewport::LIVE, |platform| ChromeViewport {
                width: platform.viewport.width,
                height: platform.viewport.height,
                device_scale_factor_milli: platform.declared_device_scale_factor,
            })
    }

    pub fn qualification_profile(&self) -> &'static str {
        self.platform
            .map_or(temporal_evaluation::LIVE_QUALIFICATION_PROFILE, |_| {
                temporal_evaluation::PLATFORM_EVIDENCE_PROFILE
            })
    }

    pub fn wrapper_variant(&self) -> krometrail_cdp::qualification_support::ChromeWrapperVariant {
        match self.platform.map(|platform| platform.profile) {
            Some(PlatformProfile::HighDpi) => {
                krometrail_cdp::qualification_support::ChromeWrapperVariant::HighDpi
            }
            Some(PlatformProfile::DefaultDpi) | None => {
                krometrail_cdp::qualification_support::ChromeWrapperVariant::DefaultDpi
            }
        }
    }

    pub fn run_root(&self) -> PathBuf {
        let run_id = if safe_path_component(&self.run_id) {
            self.run_id.as_str()
        } else {
            "invalid-run-id"
        };
        self.output_root
            .join(self.browser_product.as_str())
            .join(run_id)
    }

    pub fn data_root(&self) -> PathBuf {
        self.run_root().join("store")
    }

    pub fn profile_root(&self) -> PathBuf {
        self.run_root().join("profiles")
    }

    pub fn output_path(&self) -> PathBuf {
        self.run_root().join("run-manifest.json")
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
            viewport: config.viewport(),
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
        // Never recursively remove the live output root: the finalized manifest is written there
        // after runtime cleanup, and a previous run may already have evidence below it. Remove
        // only empty directories so cleanup cannot delete another run's output.
        let _ = remove_empty_tree(data_root.parent().unwrap_or(&output_root));
        let _ = remove_empty_tree(&output_root);
        CleanupObservation {
            server_stopped: true,
            profile_deleted: profile_removed,
            store_flushed: data_removed,
            lock_released: true,
            output_finalized: false,
            remaining_managed_resources: u64::from(!data_removed) + u64::from(!profile_removed),
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
    let clock: Arc<dyn MonotonicClock> = Arc::new(super::ProcessMonotonicClock {
        origin: Instant::now(),
    });
    let storage = open_storage_with_budget(
        &config.data_root(),
        config.retention_budget,
        Arc::clone(&clock),
    )?;
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
}

fn safe_path_component(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

fn remove_tree(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    fs::remove_dir_all(path).is_ok()
}

/// Remove an empty directory tree without ever deleting a file. This is used for parent cleanup
/// around finalized evidence, where recursive deletion would be an unacceptable data-loss path.
fn remove_empty_tree(path: &Path) -> bool {
    if !path.exists() {
        return true;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        if entry.path().is_dir() && !remove_empty_tree(&entry.path()) {
            return false;
        }
        if entry.path().is_file() {
            return false;
        }
    }
    fs::remove_dir(path).is_ok()
}

fn live_error(code: ErrorCode, message: &'static str) -> KrometrailError {
    KrometrailError::new(
        code,
        NonEmptyText::new(message).expect("static live qualification error"),
    )
}

/// Run the complete operator-authorized qualification as one dependency-ordered composition.
///
/// The lower-level modules remain seams for focused tests, but this is the only live entry point:
/// it creates one lifecycle, one production runtime/store, and one browser session, then passes
/// the same authorities through capture, control, retention, recovery, resources, latency, and
/// report finalization. A missing or incomplete stage is represented in the canonical manifest;
/// it is never replaced with a fabricated pass.
pub async fn run_live_qualification(config: LiveQualificationConfig) -> Result<RunManifest> {
    run_live_qualification_with_decision(config, OptInDecision::from_environment()).await
}

/// Test-only injection seam for platform lanes. The public environment wrapper above remains the
/// only operator entry point; this seam lets deterministic tests prove the two-gate barrier
/// without mutating process environment or launching a browser.
pub async fn run_live_qualification_with_decision(
    config: LiveQualificationConfig,
    decision: OptInDecision,
) -> Result<RunManifest> {
    if decision != OptInDecision::Authorized {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live qualification requires both explicit opt-in gates",
        ));
    }

    // This is a pure committed-fixture/definition check. It intentionally precedes browser
    // discovery so drift cannot create a managed profile or loopback listener.
    let definition = capture::validate_fixture_before_launch()?;
    let preflight = run_preflight_with_decision(config.clone(), decision).await?;
    let Some(browser) = preflight.browser.as_ref() else {
        return Err(live_error(
            ErrorCode::BrowserNotFound,
            "authorized qualification preflight did not produce a browser result",
        ));
    };
    let BrowserPreflight::Ready(installation) = browser else {
        let observations = base_observations(&config, browser_availability(browser));
        return finalize_observations(&config, observations, clean_without_resources());
    };

    let lifecycle = match QualificationLifecycle::start(&config, &preflight).await {
        Ok(lifecycle) => lifecycle,
        Err(_) => {
            let observations = base_observations(
                &config,
                BrowserAvailability::Unavailable {
                    reason: "qualification lifecycle could not become ready".into(),
                    recovery: "inspect the loopback listener and temporary profile permissions, then retry".into(),
                },
            );
            return finalize_observations(&config, observations, clean_without_resources());
        }
    };
    let runtime = match build_qualification_runtime(&config, OptInDecision::Authorized) {
        Ok(runtime) => runtime,
        Err(_) => {
            let cleanup = lifecycle.cleanup();
            let observations = base_observations(
                &config,
                BrowserAvailability::Unavailable {
                    reason: "qualification production runtime could not be opened".into(),
                    recovery: "inspect the temporary qualification store and retry".into(),
                },
            );
            return finalize_observations(&config, observations, cleanup);
        }
    };

    let wrapper = capture::qualification_wrapper(
        installation,
        lifecycle.viewport(),
        config.wrapper_variant(),
    );
    let initial_url =
        lifecycle.temporal_benchmark_url(&definition.cases[0].case_id, definition.duration_ms[0]);
    let session = match runtime
        .dependencies
        .browser
        .connect(BrowserConnectRequest::Launch(LaunchBrowser {
            executable: wrapper.as_ref().map(|wrapper| wrapper.path.clone()),
            profile: ManagedProfile::Temporary,
            initial_url: Some(initial_url),
            every_nth_frame: krometrail_core::EveryNthFrame::default(),
            focus: krometrail_core::BrowserFocusPolicy::default(),
        }))
        .await
    {
        Ok(session) => session,
        Err(_) => {
            drop(wrapper);
            let lifecycle_cleanup = lifecycle.cleanup();
            let runtime_cleanup = runtime.cleanup();
            let observations = base_observations(
                &config,
                BrowserAvailability::Unavailable {
                    reason: "qualification browser session could not be launched".into(),
                    recovery: "inspect the supported browser installation and retry".into(),
                },
            );
            return finalize_observations(
                &config,
                observations,
                merge_cleanup(lifecycle_cleanup, runtime_cleanup, true),
            );
        }
    };

    let browser_status = session.status().await.ok();
    let observed_browser = browser_status
        .as_ref()
        .map(|status| {
            report::observed_browser(&status.compatibility.version, "live-qualification-cdp")
        })
        .unwrap_or_else(|| BrowserAvailability::Unavailable {
            reason: "qualification browser identity could not be observed".into(),
            recovery: "verify the browser protocol handshake and retry".into(),
        });

    let capture =
        capture::capture_connected_session(&runtime, Arc::clone(&session), &lifecycle, &definition)
            .await
            .ok();
    let control = if capture.is_some() {
        control::run_control_session(&runtime, Arc::clone(&session), &lifecycle)
            .await
            .ok()
    } else {
        None
    };

    // Retention and latency consume one exact interval returned by the capture resolver. The
    // retention probe cleans only its own probe sessions, preserving this interval for latency.
    let interval = capture.as_ref().and_then(|run| {
        run.measurements.iter().find_map(|measurement| {
            measurement
                .interval
                .as_ref()
                .zip(measurement.resolved_range.as_ref())
        })
    });
    let mut retention = match interval {
        Some((_, resolved_range)) => {
            retention::qualify_retention_preserving_interval(&runtime, resolved_range, None)
                .await
                .ok()
        }
        None => None,
    };

    // Recovery owns a separate temporary scenario root because its production implementation
    // intentionally closes and reopens a store. The browser/capture runtime remains alive and is
    // still the shared authority for the real evidence and later latency call.
    let recovery = recovery::qualify_recovery(&recovery_scenario_config(&config))
        .await
        .ok();
    let resources = Some(resource_usage::sample_process_resources());
    let latency = match interval {
        Some((source_interval, resolved_range)) => {
            latency::measure_latency(&runtime, source_interval, resolved_range)
                .await
                .ok()
        }
        None => None,
    };
    // Stop capture before deleting the preserved source session. This is the final production
    // evidence fence; cleanup never races an active writer against session deletion.
    let stop_succeeded = session.stop().await.is_ok();
    if stop_succeeded {
        if let Some(retention) = retention.as_mut() {
            let _ = retention::finalize_preserved_retention(&runtime, retention).await;
        }
    }
    drop(wrapper);
    let lifecycle_cleanup = lifecycle.cleanup();
    let runtime_cleanup = runtime.cleanup();
    let cleanup = merge_cleanup(lifecycle_cleanup, runtime_cleanup, stop_succeeded);
    let mut observations = base_observations(&config, observed_browser);
    report::project_capture_config(
        &mut observations
            .krometrail
            .as_mut()
            .expect("contract seed provides Krometrail identity")
            .capture_config,
        browser_status.as_ref(),
    );
    observations.capture = capture;
    observations.control = control;
    observations.retention = retention;
    observations.recovery = recovery;
    observations.resources = resources;
    observations.latency = latency;
    finalize_observations(&config, observations, cleanup)
}

fn base_observations(
    config: &LiveQualificationConfig,
    browser: BrowserAvailability,
) -> report::QualificationObservations {
    let mut observations = report::QualificationObservations::contract_seed();
    observations.browser = browser;
    observations.optional_configuration = config.optional_browser;
    observations.platform = config.platform;
    observations.evidence_mode = QualificationEvidenceMode::OperatorAuthorizedLiveCapture;
    observations.retention_budget = config.retention_budget;
    observations.environment = Some(EnvironmentIdentity {
        platform: if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Other
        },
        architecture: if cfg!(target_arch = "x86_64") {
            Architecture::X86_64
        } else if cfg!(target_arch = "aarch64") {
            Architecture::Aarch64
        } else {
            Architecture::Other
        },
        os_release_class: std::env::consts::OS.into(),
    });
    observations
}

fn browser_availability(preflight: &BrowserPreflight) -> BrowserAvailability {
    match preflight {
        BrowserPreflight::Ready(_) => BrowserAvailability::Unavailable {
            reason: "browser identity was not observed".into(),
            recovery: "complete the browser protocol handshake and retry".into(),
        },
        BrowserPreflight::Blocked(failure) => BrowserAvailability::Blocked {
            reason: failure.reason.clone(),
            recovery: failure.recovery.clone(),
        },
        BrowserPreflight::Skipped(failure) => BrowserAvailability::Skipped {
            product: temporal_evaluation::BrowserProduct::Chromium,
            reason: failure.reason.clone(),
            recovery: failure.recovery.clone(),
        },
    }
}

fn clean_without_resources() -> CleanupObservation {
    CleanupObservation {
        server_stopped: true,
        profile_deleted: true,
        store_flushed: true,
        lock_released: true,
        output_finalized: false,
        remaining_managed_resources: 0,
    }
}

fn merge_cleanup(
    lifecycle: CleanupObservation,
    runtime: CleanupObservation,
    stop_succeeded: bool,
) -> CleanupObservation {
    CleanupObservation {
        server_stopped: lifecycle.server_stopped,
        profile_deleted: lifecycle.profile_deleted && runtime.profile_deleted,
        store_flushed: runtime.store_flushed,
        lock_released: lifecycle.lock_released && runtime.lock_released,
        output_finalized: false,
        remaining_managed_resources: lifecycle
            .remaining_managed_resources
            .saturating_add(runtime.remaining_managed_resources)
            .saturating_add(u64::from(!stop_succeeded)),
    }
}

fn recovery_scenario_config(config: &LiveQualificationConfig) -> LiveQualificationConfig {
    LiveQualificationConfig {
        output_root: config.run_root().join("recovery-scenario"),
        run_id: "store-recovery".into(),
        ..config.clone()
    }
}

fn finalize_observations(
    config: &LiveQualificationConfig,
    observations: report::QualificationObservations,
    cleanup: CleanupObservation,
) -> Result<RunManifest> {
    let mut run = report::assemble_manifest(observations)?;
    report::finalize_manifest_value(&mut run, cleanup, &config.output_path())
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
            config.output_path().ends_with(
                format!(
                    "target/temporal-evaluation/live/chrome/{}/run-manifest.json",
                    config.run_id
                )
                .as_str()
            )
        );
        assert!(config.data_root().starts_with(&config.output_root));
        assert!(config.profile_root().starts_with(&config.output_root));

        let unsafe_config = LiveQualificationConfig {
            run_id: "../private".into(),
            ..config
        };
        assert!(
            unsafe_config
                .run_root()
                .ends_with("target/temporal-evaluation/live/chrome/invalid-run-id")
        );
    }

    #[tokio::test]
    async fn live_orchestrator_rejects_missing_opt_in_without_touching_output() {
        if OptInDecision::from_environment() == OptInDecision::Authorized {
            // This test is deliberately never allowed to exercise the live path. The ignored
            // integration test below is the only browser-launching test.
            return;
        }
        let root = std::env::temp_dir().join(format!(
            "krometrail-live-orchestrator-disabled-{}",
            uuid::Uuid::new_v4()
        ));
        let config = LiveQualificationConfig {
            output_root: root.clone(),
            ..LiveQualificationConfig::default()
        };
        let error = run_live_qualification(config).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidLifecycleTransition);
        assert!(!root.exists());
    }

    #[test]
    fn recovery_scenario_isolated_from_the_live_store_and_output_manifest() {
        let config = LiveQualificationConfig::default();
        let recovery = recovery_scenario_config(&config);
        assert!(recovery.run_root().starts_with(config.run_root()));
        assert_ne!(recovery.data_root(), config.data_root());
        assert!(recovery.output_path().ends_with("run-manifest.json"));
    }

    #[tokio::test]
    #[ignore = "requires KROMETRAIL_REAL_CHROME_TESTS=1 and KROMETRAIL_LIVE_CAPTURE_EVALUATION=1"]
    async fn opted_in_real_qualification_writes_only_the_canonical_manifest() {
        assert_eq!(
            OptInDecision::from_environment(),
            OptInDecision::Authorized,
            "set both live qualification opt-in variables to run this test"
        );
        let config = LiveQualificationConfig::default();
        let manifest = run_live_qualification(config.clone())
            .await
            .expect("authorized live qualification");
        let output = config.output_path();
        let expected_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/temporal-evaluation/live");
        assert!(output.starts_with(expected_root));
        assert!(output.ends_with("run-manifest.json"));
        let bytes = fs::read(&output).expect("canonical live manifest");
        assert_eq!(
            temporal_evaluation::RunManifest::from_canonical_json(&bytes).unwrap(),
            manifest
        );
        let entries = fs::read_dir(config.run_root())
            .expect("live run root")
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(entries, vec![std::ffi::OsString::from("run-manifest.json")]);
    }
}
