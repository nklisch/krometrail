//! Canonical duration capture orchestration for the opt-in qualification path.
//!
//! The orchestration owns no browser, storage, or timeline implementation. It consumes the
//! production ports assembled by `QualificationRuntime`, which keeps scripted tests and the
//! authorized run on the same authority graph.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::Arc,
    time::Duration,
};

use krometrail_core::{
    AnchorScope, BrowserConnectRequest, BrowserOperationContext, BrowserOperationRequest,
    BrowserOperationResult, BrowserSessionPort, BrowserSessionState, BrowserStatus,
    BrowserStopOutcome, CaptureGap, CaptureGapPolicy, CaptureGapStore, ClickRequest,
    ElementLocator, ElementState, EncodedFrame, ErrorCode, FrameSource, ImageFormat,
    InteractionAnchorSource, InteractionId, InteractionLocator, InteractionWindow, LaunchBrowser,
    ManagedProfile, Modifiers, MouseButton, NavigatePageRequest, NonEmptyText, ObservationPart,
    PageOperationOutcome, PageSelection, RetentionPolicy, SessionRange, SessionTime, TargetId,
    TemporalQuery, TemporalQueryRequest, TemporalRangeAnchor, WaitCondition, WaitOutcome,
    WaitRequest,
};
use temporal_evaluation::{
    BenchmarkDefinition, CaptureQualificationMeasurements, CaptureTrial, CaseDefinition,
    ConditionId, DEVICE_SCALE_FACTOR_MILLI, DurationQualificationMeasurement, EvaluationStatus,
    EvidenceAvailability, FIXTURE_ROOT, FailureRecord, FixtureFile, GapEvidence, RetentionState,
    RunFailureCode, ScopeIdentity, SourceFrameEvidence, SourceInterval, TimeRangeNs, TrialIdentity,
    VIEWPORT_HEIGHT, VIEWPORT_WIDTH, Viewport,
};

use super::fixture_observation::{
    FixtureSequenceObservation, FixtureStateObservation, FrameGeometry,
    MovementSequenceObservation, TemporalFixtureObservation,
    observe_fixture_frame_with_expected_geometry,
};
use super::{
    BrowserPreflight, LiveQualificationConfig, OptInDecision, QualificationLifecycle,
    QualificationRuntime, live_error,
};

pub const INTERVAL_BEFORE: Duration = Duration::from_millis(250);
pub const INTERVAL_AFTER: Duration = Duration::from_millis(1_000);
pub const WAIT_TIMEOUT: Duration = Duration::from_secs(5);
pub const WAIT_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Observable barriers used by the scripted harness and the real run. A later stage cannot be
/// recorded before its production authority has reported the earlier stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CaptureBarrier {
    SessionReady,
    TargetReady,
    ViewportVerified,
    Navigated,
    Clicked,
    Settled,
    IntervalResolved,
}

pub fn barrier_order_is_valid(stages: &[CaptureBarrier]) -> bool {
    let mut previous = None;
    for stage in stages {
        let rank = match stage {
            CaptureBarrier::SessionReady => 0,
            CaptureBarrier::TargetReady => 1,
            CaptureBarrier::ViewportVerified => 2,
            CaptureBarrier::Navigated => 3,
            CaptureBarrier::Clicked => 4,
            CaptureBarrier::Settled => 5,
            CaptureBarrier::IntervalResolved => 6,
        };
        if previous.is_some_and(|previous| rank < previous) {
            return false;
        }
        previous = Some(rank);
    }
    true
}

/// The capture configuration intentionally uses PNG. The test-only pixel predicates need exact
/// retained pixels; changing to JPEG would turn compression artifacts into purported fixture
/// state. This is still the production capture coordinator and store, not a second recorder.
pub(crate) fn qualification_capture_config() -> krometrail_cdp::CaptureConfig {
    krometrail_cdp::CaptureConfig {
        format: ImageFormat::Png,
        jpeg_quality: None,
        ..krometrail_cdp::CaptureConfig::default()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureTrialMeasurement {
    pub trial: CaptureTrial,
    pub interaction_id: Option<InteractionId>,
    pub interval: Option<SourceInterval>,
    pub resolved_range: Option<krometrail_core::ResolvedRange>,
    pub observations: Vec<TemporalFixtureObservation>,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
    pub retained_frame_count: u64,
    pub observed_frame_count: u64,
    pub source_time_sample_count: u64,
    pub gap_ids: Vec<String>,
    pub observed_viewport: Option<Viewport>,
    pub observed_device_scale_factor: Option<u16>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptureQualificationRun {
    pub measurements: Vec<CaptureTrialMeasurement>,
    pub capture: CaptureQualificationMeasurements,
    pub manifest_trials: Vec<TrialIdentity>,
    pub status: EvaluationStatus,
}

/// The exact production authorities needed to make one interaction-anchored interval.
/// Keeping these as ports makes fake ordering/gap/clock tests independent from SQLite and Chrome.
pub struct IntervalAuthorities<'a> {
    pub query: &'a dyn TemporalQuery,
    pub frames: &'a dyn FrameSource,
    pub gaps: &'a dyn CaptureGapStore,
    pub interactions: &'a dyn InteractionAnchorSource,
}

/// Validate the committed fixture and definition before browser discovery or launch.
pub fn validate_fixture_before_launch() -> krometrail_core::Result<BenchmarkDefinition> {
    let definition = BenchmarkDefinition::canonical();
    definition.validate().map_err(|error| {
        live_error(
            ErrorCode::InvalidInput,
            "canonical temporal benchmark definition is invalid",
        )
        .with_recovery(NonEmptyText::new(error.to_string()).expect("contract error text"))
    })?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_ROOT);
    let mut files = BTreeMap::new();
    for fixture_file in &definition.fixture.files {
        let bytes = fs::read(root.join(&fixture_file.path)).map_err(|_| {
            live_error(
                ErrorCode::PersistenceFailed,
                "canonical temporal benchmark fixture could not be read",
            )
        })?;
        files.insert(fixture_file.path.clone(), bytes);
    }
    validate_fixture_sources(&definition, &files)?;
    Ok(definition)
}

/// Pure drift check used by prelaunch code and mutation tests. The definition remains the only
/// source of case and duration identity; this function never introduces another registry.
pub fn validate_fixture_sources(
    definition: &BenchmarkDefinition,
    files: &BTreeMap<String, Vec<u8>>,
) -> krometrail_core::Result<()> {
    let mut source_text = BTreeMap::new();
    for fixture_file in &definition.fixture.files {
        let bytes = files.get(&fixture_file.path).ok_or_else(|| {
            live_error(
                ErrorCode::PersistenceFailed,
                "canonical temporal benchmark fixture file is missing",
            )
        })?;
        let actual = FixtureFile::from_bytes(fixture_file.path.clone(), bytes).map_err(|_| {
            live_error(
                ErrorCode::InvalidInput,
                "canonical temporal benchmark fixture file identity is invalid",
            )
        })?;
        if actual.sha256 != fixture_file.sha256 {
            return Err(live_error(
                ErrorCode::InvalidInput,
                "canonical temporal benchmark fixture hash drifted",
            ));
        }
        source_text.insert(
            fixture_file.path.clone(),
            String::from_utf8(bytes.clone()).map_err(|_| {
                live_error(
                    ErrorCode::InvalidInput,
                    "canonical temporal benchmark fixture is not UTF-8",
                )
            })?,
        );
    }
    let html = source_text
        .get("index.html")
        .ok_or_else(|| live_error(ErrorCode::InvalidInput, "benchmark HTML is missing"))?;
    let javascript = source_text
        .get("benchmark.js")
        .ok_or_else(|| live_error(ErrorCode::InvalidInput, "benchmark script is missing"))?;
    let css = source_text
        .get("benchmark.css")
        .ok_or_else(|| live_error(ErrorCode::InvalidInput, "benchmark stylesheet is missing"))?;
    let readme = source_text
        .get("README.md")
        .ok_or_else(|| live_error(ErrorCode::InvalidInput, "benchmark README is missing"))?;

    for case in &definition.cases {
        if !javascript.contains(&format!("\"{}\"", case.case_id)) {
            return Err(live_error(
                ErrorCode::InvalidInput,
                "benchmark case registry drifted from the committed fixture",
            ));
        }
    }
    for duration_ms in &definition.duration_ms {
        if !javascript.contains(&duration_ms.to_string()) {
            return Err(live_error(
                ErrorCode::InvalidInput,
                "benchmark duration registry drifted from the committed fixture",
            ));
        }
    }
    for required in [
        "id=\"run\"",
        "running = false",
        "requestAnimationFrame",
        "performance.now()",
        "resetVisuals();",
    ] {
        if !html.contains(required) && !javascript.contains(required) {
            return Err(live_error(
                ErrorCode::InvalidInput,
                "benchmark observable control contract drifted",
            ));
        }
    }
    if !css.contains("width: 800px")
        || !css.contains("height: 450px")
        || !readme.contains("intended fixture interval")
    {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "benchmark viewport or timing contract drifted",
        ));
    }
    Ok(())
}

/// Build the canonical matrix once. Every caller, manifest identity, and live URL consumes this
/// result rather than maintaining a case/duration/repetition list of its own.
pub fn canonical_capture_trials(
    definition: &BenchmarkDefinition,
) -> krometrail_core::Result<Vec<CaptureTrial>> {
    definition
        .matrix
        .capture_trials(&definition.cases, &definition.duration_ms)
        .map_err(|error| {
            live_error(
                ErrorCode::InvalidInput,
                "canonical capture matrix could not be constructed",
            )
            .with_recovery(NonEmptyText::new(error.to_string()).expect("contract error text"))
        })
}

pub fn canonical_manifest_trials(
    definition: &BenchmarkDefinition,
) -> krometrail_core::Result<Vec<TrialIdentity>> {
    canonical_capture_trials(definition).map(|trials| {
        trials
            .into_iter()
            .map(|trial| TrialIdentity {
                trial_id: trial.trial_id,
                case_id: trial.case_id,
                family: trial.family,
                duration_ms: trial.duration_ms,
                repetition: trial.repetition,
                condition_id: ConditionId::AFinalScreenshot,
            })
            .collect()
    })
}

pub fn interaction_interval_request(
    session_id: krometrail_core::SessionId,
    target_id: TargetId,
    interaction_id: InteractionId,
) -> krometrail_core::Result<TemporalQueryRequest> {
    let scope = AnchorScope::new(Some(session_id), Some(target_id));
    let window = InteractionWindow::new(INTERVAL_BEFORE, INTERVAL_AFTER).map_err(|_| {
        live_error(
            ErrorCode::InvalidInput,
            "capture interval window is invalid",
        )
    })?;
    TemporalQueryRequest::new(
        TemporalRangeAnchor::Interaction {
            scope,
            interaction_id,
            window: Some(window),
        },
        RetentionPolicy::RequireComplete,
        CaptureGapPolicy::Include,
    )
    .map_err(|_| {
        live_error(
            ErrorCode::InvalidInput,
            "interaction interval request is invalid",
        )
    })
}

/// The exact interval pair returned by the production resolver. Keeping both values together
/// prevents later qualification stages from reconstructing a natural interaction anchor from its
/// serialized projection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedSourceInterval {
    pub interval: SourceInterval,
    pub resolved_range: krometrail_core::ResolvedRange,
}

/// Resolve one exact interval through the production temporal query and then re-read the durable
/// gap/source authorities. No ordinal arithmetic is used to invent missing frames or gaps.
pub async fn resolve_source_interval_for_interaction(
    authorities: &IntervalAuthorities<'_>,
    session_id: krometrail_core::SessionId,
    target_id: TargetId,
    interaction_id: InteractionId,
) -> krometrail_core::Result<ResolvedSourceInterval> {
    let request = interaction_interval_request(session_id, target_id, interaction_id)?;
    let resolved = authorities.query.resolve_range(request).await?;
    if resolved.session_id != session_id
        || resolved.target_id != target_id
        || !resolved.interaction_ids.contains(&interaction_id)
        || !matches!(
            resolved.resolved_anchor.reference,
            krometrail_core::ResolvedAnchorReference::Interaction { interaction_id: id }
                if id == interaction_id
        )
    {
        return Err(live_error(
            ErrorCode::PersistenceFailed,
            "temporal query returned an interaction interval with the wrong identity",
        ));
    }
    let anchor = authorities
        .interactions
        .interaction_anchor(interaction_id)
        .await?
        .ok_or_else(|| {
            live_error(
                ErrorCode::NotFound,
                "interaction anchor was not retained by the control timeline",
            )
        })?;
    if anchor.session_id != session_id || anchor.target_id != target_id {
        return Err(live_error(
            ErrorCode::PersistenceFailed,
            "control timeline interaction scope disagrees with the query range",
        ));
    }

    let declared_gaps = authorities
        .gaps
        .gaps(session_id, target_id, resolved.resolved_range)
        .await?;
    let query_gap_ids = resolved
        .gaps
        .iter()
        .map(|gap| gap.id().to_string())
        .collect::<Vec<_>>();
    let store_gap_ids = declared_gaps
        .iter()
        .map(|gap| gap.id().to_string())
        .collect::<Vec<_>>();
    if query_gap_ids != store_gap_ids {
        return Err(live_error(
            ErrorCode::PersistenceFailed,
            "temporal query and capture-gap store returned different declared gaps",
        ));
    }
    if !resolved.retention_warnings.is_empty() {
        return Err(live_error(
            ErrorCode::BudgetExhausted,
            "source interval retention was interrupted before measurement",
        ));
    }

    let metadata = authorities
        .frames
        .frame_metadata_by_id(resolved.frame_ids.clone())
        .await?;
    let encoded = authorities
        .frames
        .frames_by_id(resolved.frame_ids.clone())
        .await?;
    if metadata.len() != resolved.frame_ids.len()
        || encoded.len() != resolved.frame_ids.len()
        || metadata
            .iter()
            .zip(&resolved.frame_ids)
            .any(|(frame, id)| frame.id() != *id)
        || encoded
            .iter()
            .zip(&resolved.frame_ids)
            .any(|(frame, id)| frame.metadata().id() != *id)
    {
        return Err(live_error(
            ErrorCode::EvidenceInvalidated,
            "retained source identity order changed during interval construction",
        ));
    }

    let frames = metadata
        .iter()
        .zip(encoded)
        .map(|(frame, encoded)| {
            let source_time_ns = frame
                .source_time()
                .map(|source| u64::try_from(source.as_nanos()))
                .transpose()
                .map_err(|_| {
                    live_error(
                        ErrorCode::InvalidTime,
                        "captured source timestamp cannot be represented in the qualification contract",
                    )
                })?;
            let hash = temporal_evaluation::sha256_prefixed(encoded.bytes());
            SourceFrameEvidence::new(
                frame.id().to_string(),
                frame.capture_ordinal().get(),
                source_time_ns,
                frame.observed_time().as_nanos(),
                frame.session_time().as_nanos(),
                hash,
                EvidenceAvailability::Retained,
            )
            .map_err(|_| live_error(ErrorCode::EvidenceInvalidated, "retained source metadata is invalid"))
        })
        .collect::<krometrail_core::Result<Vec<_>>>()?;
    let gaps = declared_gaps
        .iter()
        .map(gap_evidence)
        .collect::<krometrail_core::Result<Vec<_>>>()?;
    let interval_id = format!("interval-{interaction_id}");
    let interval = SourceInterval::new(
        interval_id,
        ScopeIdentity::new(session_id.to_string(), target_id.to_string())
            .map_err(|_| live_error(ErrorCode::InvalidInput, "source interval scope is invalid"))?,
        TimeRangeNs::new(
            resolved.requested_range.start().as_nanos(),
            resolved.requested_range.end().as_nanos(),
        )
        .map_err(|_| {
            live_error(
                ErrorCode::InvalidTime,
                "source interval request range is invalid",
            )
        })?,
        TimeRangeNs::new(
            resolved.resolved_range.start().as_nanos(),
            resolved.resolved_range.end().as_nanos(),
        )
        .map_err(|_| {
            live_error(
                ErrorCode::InvalidTime,
                "source interval resolved range is invalid",
            )
        })?,
        resolved.resolved_anchor.effective_time.as_nanos(),
        frames,
        gaps,
        RetentionState::Retained,
    )
    .map_err(|_| {
        live_error(
            ErrorCode::PersistenceFailed,
            "source interval identity validation failed",
        )
    })?;
    Ok(ResolvedSourceInterval {
        interval,
        resolved_range: resolved,
    })
}

pub async fn source_interval_for_interaction(
    authorities: &IntervalAuthorities<'_>,
    session_id: krometrail_core::SessionId,
    target_id: TargetId,
    interaction_id: InteractionId,
) -> krometrail_core::Result<SourceInterval> {
    Ok(
        resolve_source_interval_for_interaction(authorities, session_id, target_id, interaction_id)
            .await?
            .interval,
    )
}

fn gap_evidence(gap: &CaptureGap) -> krometrail_core::Result<GapEvidence> {
    GapEvidence::new(
        gap.id().to_string(),
        gap.range().start().as_nanos(),
        gap.range().end().as_nanos(),
        gap.reason().as_str(),
        gap.estimated_missing_frames().map(|value| value.get()),
    )
    .map_err(|_| {
        live_error(
            ErrorCode::PersistenceFailed,
            "declared capture gap is invalid",
        )
    })
}

/// Run the capture matrix against one connected production session. Callers must invoke the
/// prelaunch drift check before this function; the high-level opt-in entry point does so.
pub async fn capture_connected_session(
    runtime: &QualificationRuntime,
    session: Arc<dyn BrowserSessionPort>,
    lifecycle: &QualificationLifecycle,
    definition: &BenchmarkDefinition,
) -> krometrail_core::Result<CaptureQualificationRun> {
    let trials = canonical_capture_trials(definition)?;
    let status = session.status().await?;
    let target_id = selected_target(&status)?;
    verify_browser_ready(&status, target_id)?;
    let authorities = IntervalAuthorities {
        query: runtime.dependencies.temporal_queries.as_ref(),
        frames: runtime.dependencies.frames.as_ref(),
        gaps: runtime.dependencies.gaps.as_ref(),
        interactions: runtime.store.as_ref(),
    };
    let mut measurements = Vec::with_capacity(trials.len());
    for trial in trials {
        let case = definition
            .case(&trial.case_id)
            .ok_or_else(|| live_error(ErrorCode::InvalidInput, "matrix case is not canonical"))?;
        let url = lifecycle.temporal_benchmark_url(&trial.case_id, trial.duration_ms);
        navigate(&session, target_id, url).await?;
        verify_viewport(&session, target_id, lifecycle.viewport()).await?;
        let interaction_id = run_fixture(&session, target_id).await?;
        let interval = resolve_source_interval_for_interaction(
            &authorities,
            status.session_id,
            target_id,
            interaction_id,
        )
        .await;
        measurements.push(
            measure_trial(
                trial,
                case,
                interaction_id,
                interval,
                &authorities,
                lifecycle.viewport(),
            )
            .await?,
        );
    }
    let capture = summarize_capture(definition, &measurements)?;
    let manifest_trials = canonical_manifest_trials(definition)?;
    let status = measurements
        .iter()
        .map(|measurement| measurement.status)
        .max_by_key(|status| status.precedence())
        .unwrap_or(EvaluationStatus::Inconclusive);
    Ok(CaptureQualificationRun {
        measurements,
        capture,
        manifest_trials,
        status,
    })
}

/// Authorized real-run entry point. It performs no work unless both existing and feature-specific
/// opt-ins are present and validates the committed fixture before browser discovery.
pub(crate) async fn run_opted_in_capture(
    config: LiveQualificationConfig,
) -> krometrail_core::Result<CaptureQualificationRun> {
    if OptInDecision::from_environment() != OptInDecision::Authorized {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live capture requires both explicit opt-in environment gates",
        ));
    }
    let definition = validate_fixture_before_launch()?;
    let preflight = super::run_preflight(config.clone()).await?;
    let BrowserPreflight::Ready(installation) = preflight.browser.as_ref().ok_or_else(|| {
        live_error(
            ErrorCode::BrowserNotFound,
            "live browser preflight did not run",
        )
    })?
    else {
        return Err(live_error(
            ErrorCode::BrowserNotFound,
            "live browser preflight did not find a required browser",
        ));
    };
    let lifecycle = QualificationLifecycle::start(&config, &preflight).await?;
    let runtime = match super::build_qualification_runtime(&config, OptInDecision::Authorized) {
        Ok(runtime) => runtime,
        Err(error) => {
            let _ = lifecycle.cleanup();
            return Err(error);
        }
    };
    let wrapper =
        qualification_wrapper(installation, lifecycle.viewport(), config.wrapper_variant());
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
        Err(error) => {
            let _ = lifecycle.cleanup();
            let _ = runtime.cleanup();
            return Err(error);
        }
    };
    let result =
        capture_connected_session(&runtime, Arc::clone(&session), &lifecycle, &definition).await;
    let stop = session.stop().await;
    let _ = wrapper;
    let cleanup = lifecycle.cleanup();
    let _ = runtime.cleanup();
    stop_result(result, stop, cleanup)
}

pub(crate) fn qualification_wrapper(
    installation: &krometrail_core::BrowserInstallation,
    viewport: krometrail_cdp::qualification_support::ChromeViewport,
    variant: krometrail_cdp::qualification_support::ChromeWrapperVariant,
) -> Option<krometrail_cdp::qualification_support::ChromeWrapper> {
    #[cfg(unix)]
    {
        Some(
            krometrail_cdp::qualification_support::ChromeWrapper::new_with_viewport(
                installation.executable.clone(),
                installation.product,
                variant,
                viewport,
            ),
        )
    }
    #[cfg(not(unix))]
    {
        let _ = (installation, viewport);
        None
    }
}

fn stop_result(
    result: krometrail_core::Result<CaptureQualificationRun>,
    stop: krometrail_core::Result<BrowserStopOutcome>,
    cleanup: super::CleanupObservation,
) -> krometrail_core::Result<CaptureQualificationRun> {
    let run = result?;
    let _ = stop?;
    if !cleanup.is_clean() {
        return Err(live_error(
            ErrorCode::PersistenceFailed,
            "live capture cleanup did not release all managed resources",
        ));
    }
    Ok(run)
}

fn selected_target(status: &BrowserStatus) -> krometrail_core::Result<TargetId> {
    status.selected_target_id.ok_or_else(|| {
        live_error(
            ErrorCode::TargetFailed,
            "live capture session has no selected benchmark target",
        )
    })
}

fn verify_browser_ready(
    status: &BrowserStatus,
    target_id: TargetId,
) -> krometrail_core::Result<()> {
    if status.state != BrowserSessionState::Ready {
        return Err(live_error(
            ErrorCode::InvalidLifecycleTransition,
            "live capture session is not connected",
        ));
    }
    if !status
        .pages
        .iter()
        .any(|page| page.target.target.id() == target_id)
    {
        return Err(live_error(
            ErrorCode::TargetFailed,
            "live capture target is not supervised",
        ));
    }
    Ok(())
}

async fn navigate(
    session: &Arc<dyn BrowserSessionPort>,
    target_id: TargetId,
    url: String,
) -> krometrail_core::Result<()> {
    let request =
        NavigatePageRequest::new(PageSelection::Target(target_id), url).map_err(|_| {
            live_error(
                ErrorCode::InvalidInput,
                "benchmark navigation URL is invalid",
            )
        })?;
    let result = session
        .execute(
            BrowserOperationRequest::NavigatePage(request),
            BrowserOperationContext::default(),
        )
        .await?;
    match result {
        BrowserOperationResult::NavigatePage(value)
            if matches!(value.outcome, PageOperationOutcome::Succeeded(_)) =>
        {
            Ok(())
        }
        _ => Err(live_error(
            ErrorCode::NavigationFailed,
            "benchmark navigation did not produce a successful live observation",
        )),
    }
}

async fn verify_viewport(
    session: &Arc<dyn BrowserSessionPort>,
    target_id: TargetId,
    expected: krometrail_cdp::qualification_support::ChromeViewport,
) -> krometrail_core::Result<()> {
    let request =
        BrowserOperationRequest::InspectPage(krometrail_core::InspectPageRequest::new(target_id));
    let result = session
        .execute(request, BrowserOperationContext::default())
        .await?;
    let BrowserOperationResult::InspectPage(page) = result else {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "viewport inspection returned the wrong operation",
        ));
    };
    let viewport = page.viewport;
    if viewport.layout_viewport.size.width.round() as u32 != expected.width
        || viewport.layout_viewport.size.height.round() as u32 != expected.height
        || (viewport.device_scale_factor.get() - expected.scale_factor()).abs() > f64::EPSILON
    {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "observed browser viewport or device scale does not match the canonical profile",
        ));
    }
    Ok(())
}

async fn run_fixture(
    session: &Arc<dyn BrowserSessionPort>,
    target_id: TargetId,
) -> krometrail_core::Result<InteractionId> {
    let click = ClickRequest::new(
        PageSelection::Target(target_id),
        InteractionLocator::element(ElementLocator::CssSelector(
            NonEmptyText::new("#run").expect("static benchmark selector"),
        )),
        MouseButton::Left,
        Modifiers::default(),
        1,
        false,
    )?;
    let result = session
        .execute(
            BrowserOperationRequest::Click(click),
            BrowserOperationContext::default(),
        )
        .await?;
    let BrowserOperationResult::Click(value) = result else {
        return Err(live_error(
            ErrorCode::InvalidInput,
            "benchmark click returned the wrong operation",
        ));
    };
    if !matches!(value.observation.page, ObservationPart::Available(_)) {
        return Err(live_error(
            ErrorCode::CaptureRejected,
            "benchmark click did not return a live observation",
        ));
    }
    let interaction_id = value.record.id;
    wait_for_settle(session, target_id).await?;
    Ok(interaction_id)
}

async fn wait_for_settle(
    session: &Arc<dyn BrowserSessionPort>,
    target_id: TargetId,
) -> krometrail_core::Result<()> {
    let request = WaitRequest::new(
        PageSelection::Target(target_id),
        WaitCondition::Element {
            locator: ElementLocator::CssSelector(
                NonEmptyText::new("#run").expect("static benchmark selector"),
            ),
            state: ElementState::Enabled,
        },
        WAIT_TIMEOUT,
        WAIT_POLL_INTERVAL,
    )?;
    let result = session
        .execute(
            BrowserOperationRequest::Wait(request),
            BrowserOperationContext::default(),
        )
        .await?;
    match result {
        BrowserOperationResult::Wait(value)
            if matches!(value.outcome, WaitOutcome::Satisfied { .. }) =>
        {
            Ok(())
        }
        _ => Err(live_error(
            ErrorCode::CaptureRejected,
            "benchmark settle barrier did not observe an enabled run control",
        )),
    }
}

async fn measure_trial(
    trial: CaptureTrial,
    case: &CaseDefinition,
    interaction_id: InteractionId,
    interval: krometrail_core::Result<ResolvedSourceInterval>,
    authorities: &IntervalAuthorities<'_>,
    expected_viewport: krometrail_cdp::qualification_support::ChromeViewport,
) -> krometrail_core::Result<CaptureTrialMeasurement> {
    let ResolvedSourceInterval {
        interval,
        resolved_range,
    } = match interval {
        Ok(interval) => interval,
        Err(error) => {
            let status = if matches!(
                error.code,
                ErrorCode::BudgetExhausted | ErrorCode::CaptureRejected
            ) {
                EvaluationStatus::Inconclusive
            } else {
                EvaluationStatus::Blocked
            };
            return Ok(CaptureTrialMeasurement {
                trial,
                interaction_id: None,
                interval: None,
                resolved_range: None,
                observations: Vec::new(),
                status,
                failure: Some(failure_for_error(error.code)),
                retained_frame_count: 0,
                observed_frame_count: 0,
                source_time_sample_count: 0,
                gap_ids: Vec::new(),
                observed_viewport: None,
                observed_device_scale_factor: None,
            });
        }
    };
    let encoded = authorities
        .frames
        .frames_by_id(
            interval
                .frames
                .iter()
                .filter_map(|frame| frame.id.parse::<krometrail_core::FrameId>().ok())
                .collect(),
        )
        .await?;
    if encoded.len() != interval.frames.len() {
        return Err(live_error(
            ErrorCode::EvidenceInvalidated,
            "source interval payload count changed during observation",
        ));
    }
    let observations = encoded
        .iter()
        .map(|frame| {
            let metadata = frame.metadata();
            observe_fixture_frame_with_expected_geometry(
                frame.bytes(),
                case,
                FrameGeometry {
                    width: metadata.viewport().width(),
                    height: metadata.viewport().height(),
                    device_scale_factor_milli: (metadata.device_scale_factor().get() * 1_000.0)
                        .round() as u16,
                },
                FrameGeometry {
                    width: expected_viewport.width,
                    height: expected_viewport.height,
                    device_scale_factor_milli: expected_viewport.device_scale_factor_milli,
                },
            )
        })
        .collect::<Vec<_>>();
    let raw_frames = encoded.iter().map(EncodedFrame::bytes).collect::<Vec<_>>();
    let observed_geometry = encoded
        .first()
        .map(|frame| FrameGeometry {
            width: frame.metadata().viewport().width(),
            height: frame.metadata().viewport().height(),
            device_scale_factor_milli: (frame.metadata().device_scale_factor().get() * 1_000.0)
                .round() as u16,
        })
        .unwrap_or(FrameGeometry::CANONICAL);
    let sequence = super::fixture_observation::observe_fixture_sequence_with_expected_geometry(
        &raw_frames,
        case,
        observed_geometry,
        FrameGeometry {
            width: expected_viewport.width,
            height: expected_viewport.height,
            device_scale_factor_milli: expected_viewport.device_scale_factor_milli,
        },
    );
    let status = trial_status(case, &interval, &observations, &sequence);
    let first_metadata = encoded.first().map(EncodedFrame::metadata);
    let observed_device_scale_factor = first_metadata
        .map(|metadata| (metadata.device_scale_factor().get() * 1_000.0).round() as u16);
    let observed_viewport = first_metadata.map(|metadata| {
        let scale = metadata.device_scale_factor().get().max(1.0);
        Viewport {
            width: (f64::from(metadata.viewport().width()) / scale).round() as u32,
            height: (f64::from(metadata.viewport().height()) / scale).round() as u32,
        }
    });
    let failure = match status {
        EvaluationStatus::Pass => None,
        EvaluationStatus::Fail => Some(failure_for_error(ErrorCode::BudgetExhausted)),
        EvaluationStatus::Inconclusive => Some(failure_for_error(ErrorCode::CaptureRejected)),
        EvaluationStatus::Blocked => Some(failure_for_error(ErrorCode::InvalidInput)),
        EvaluationStatus::Skipped => Some(failure_for_error(ErrorCode::Unsupported)),
    };
    let source_time_sample_count = interval
        .frames
        .iter()
        .filter(|frame| frame.source_time_ns.is_some())
        .count() as u64;
    Ok(CaptureTrialMeasurement {
        trial,
        interaction_id: Some(interaction_id),
        interval: Some(interval.clone()),
        resolved_range: Some(resolved_range),
        observations,
        status,
        failure,
        retained_frame_count: interval.retained_frames().count() as u64,
        observed_frame_count: interval
            .frames
            .iter()
            .filter(|frame| frame.availability == EvidenceAvailability::Retained)
            .count() as u64,
        source_time_sample_count,
        gap_ids: interval.gap_ids(),
        observed_viewport,
        observed_device_scale_factor,
    })
}

fn trial_status(
    case: &CaseDefinition,
    interval: &SourceInterval,
    observations: &[TemporalFixtureObservation],
    sequence: &FixtureSequenceObservation,
) -> EvaluationStatus {
    if interval.has_unresolved_gap() || interval.retention != RetentionState::Retained {
        return EvaluationStatus::Inconclusive;
    }
    if observations.iter().any(|observation| {
        matches!(
            observation.state,
            FixtureStateObservation::Unknown(
                super::fixture_observation::UnknownReason::ViewportMismatch
                    | super::fixture_observation::UnknownReason::ScaleMismatch
            )
        )
    }) {
        return EvaluationStatus::Blocked;
    }
    if observations
        .iter()
        .any(|observation| matches!(observation.state, FixtureStateObservation::Unknown(_)))
    {
        return EvaluationStatus::Inconclusive;
    }
    if !sequence_supports_case(case, sequence) {
        return EvaluationStatus::Inconclusive;
    }
    EvaluationStatus::Pass
}

fn sequence_supports_case(case: &CaseDefinition, sequence: &FixtureSequenceObservation) -> bool {
    let has_baseline = sequence
        .frames
        .iter()
        .any(|frame| frame.state == FixtureStateObservation::Baseline);
    let has_final = sequence
        .frames
        .iter()
        .any(|frame| frame.state == FixtureStateObservation::Final);
    let has_changed = sequence
        .frames
        .iter()
        .any(|frame| frame.state == FixtureStateObservation::Changed);
    match case.family {
        temporal_evaluation::CaseFamily::MovementReversal => {
            has_baseline
                && has_changed
                && has_final
                && sequence.movement == MovementSequenceObservation::Reversal
        }
        temporal_evaluation::CaseFamily::DomOpaqueMotion => {
            has_baseline && has_final && has_changed
        }
        temporal_evaluation::CaseFamily::StableControl => {
            has_final
                && matches!(
                    sequence.movement,
                    MovementSequenceObservation::Monotonic | MovementSequenceObservation::Stable
                )
        }
        temporal_evaluation::CaseFamily::Flicker
        | temporal_evaluation::CaseFamily::TransientLayout => {
            has_baseline && has_changed && has_final
        }
    }
}

fn summarize_capture(
    definition: &BenchmarkDefinition,
    measurements: &[CaptureTrialMeasurement],
) -> krometrail_core::Result<CaptureQualificationMeasurements> {
    let mut per_duration = Vec::with_capacity(definition.duration_ms.len());
    let mut gap_ids = BTreeSet::new();
    let mut source_frame_count = 0_u64;
    let mut observed_frame_count = 0_u64;
    let mut source_time_sample_count = 0_u64;
    for measurement in measurements {
        gap_ids.extend(measurement.gap_ids.iter().cloned());
        source_frame_count = source_frame_count.saturating_add(measurement.retained_frame_count);
        observed_frame_count =
            observed_frame_count.saturating_add(measurement.observed_frame_count);
        source_time_sample_count =
            source_time_sample_count.saturating_add(measurement.source_time_sample_count);
    }
    for &duration_ms in &definition.duration_ms {
        let rows = measurements
            .iter()
            .filter(|measurement| measurement.trial.duration_ms == duration_ms)
            .collect::<Vec<_>>();
        let total = rows.len() as u32;
        let eligible = rows
            .iter()
            .filter(|measurement| measurement.interval.is_some())
            .count() as u32;
        let observed = rows
            .iter()
            .filter(|measurement| measurement.status == EvaluationStatus::Pass)
            .count() as u32;
        let status = duration_status(duration_ms, total, eligible, observed, &rows);
        per_duration.push(DurationQualificationMeasurement {
            duration_ms,
            eligible_count: eligible,
            observed_count: observed,
            eligibility_rate_basis_points: basis_points(eligible, total),
            coverage_rate_basis_points: basis_points(observed, eligible),
            status,
        });
    }
    let observed_viewport = measurements
        .iter()
        .find_map(|measurement| measurement.observed_viewport)
        .unwrap_or(Viewport {
            width: VIEWPORT_WIDTH,
            height: VIEWPORT_HEIGHT,
        });
    let observed_device_scale_factor = measurements
        .iter()
        .find_map(|measurement| measurement.observed_device_scale_factor)
        .unwrap_or(DEVICE_SCALE_FACTOR_MILLI);
    let gap_count = gap_ids.len() as u64;
    Ok(CaptureQualificationMeasurements {
        requested_durations_ms: definition.duration_ms.clone(),
        repetitions: definition.matrix.capture_repetitions,
        observed_viewport,
        observed_device_scale_factor,
        source_frame_count,
        observed_frame_count,
        source_time_sample_count,
        gap_ids: gap_ids.into_iter().collect(),
        gap_count,
        per_duration,
    })
}

fn duration_status(
    duration_ms: u16,
    total: u32,
    eligible: u32,
    observed: u32,
    rows: &[&CaptureTrialMeasurement],
) -> EvaluationStatus {
    if total == 0 || eligible == 0 {
        return EvaluationStatus::Inconclusive;
    }
    if rows
        .iter()
        .any(|row| row.status == EvaluationStatus::Blocked)
    {
        return EvaluationStatus::Blocked;
    }
    if rows.iter().any(|row| row.interval.is_none()) {
        return EvaluationStatus::Inconclusive;
    }
    if observed < eligible {
        return EvaluationStatus::Inconclusive;
    }
    let Some(threshold) = capture_threshold(duration_ms) else {
        return EvaluationStatus::Pass;
    };
    if basis_points(observed, eligible) < threshold {
        EvaluationStatus::Fail
    } else {
        EvaluationStatus::Pass
    }
}

/// Only the criteria explicitly stated by EVALUATION.md are decisive here. The 16/33 ms rows
/// remain measured but do not acquire a threshold claim in this story.
fn capture_threshold(duration_ms: u16) -> Option<u16> {
    match duration_ms {
        50 => Some(8_000),
        100 | 200 => Some(9_500),
        _ => None,
    }
}

fn basis_points(numerator: u32, denominator: u32) -> u16 {
    if denominator == 0 {
        return 0;
    }
    ((u64::from(numerator) * 10_000) / u64::from(denominator)).min(10_000) as u16
}

fn failure_for_error(code: ErrorCode) -> FailureRecord {
    let code = match code {
        ErrorCode::BudgetExhausted => RunFailureCode::Retention,
        ErrorCode::CaptureRejected => RunFailureCode::CaptureGap,
        ErrorCode::EvidenceInvalidated | ErrorCode::PersistenceFailed => {
            RunFailureCode::CorruptSource
        }
        ErrorCode::InvalidInput => RunFailureCode::Unavailable,
        _ => RunFailureCode::InsufficientEvidence,
    };
    FailureRecord {
        code,
        phase: "capture".into(),
        reason: "capture evidence is incomplete or below the applicable criterion".into(),
        recovery: "inspect retained source frames and declared gaps before retrying".into(),
        retryable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU64;
    use uuid::Uuid;

    use krometrail_core::{
        BrowserOperationKind, CaptureOrdinal, CapturedFrame, DeviceScaleFactor, FrameId,
        ImageFormat, InteractionAnchor, InteractionTiming, ObservedTime, PortFuture, SourceTime,
        TargetId,
    };

    struct FakeQuery {
        range: krometrail_core::ResolvedRange,
    }
    impl TemporalQuery for FakeQuery {
        fn resolve_range(
            &self,
            _request: TemporalQueryRequest,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::ResolvedRange>> {
            Box::pin(std::future::ready(Ok(self.range.clone())))
        }
    }

    struct FakeGaps {
        gaps: Vec<CaptureGap>,
    }
    impl CaptureGapStore for FakeGaps {
        fn append_gap(&self, _gap: CaptureGap) -> PortFuture<'_, krometrail_core::Result<()>> {
            Box::pin(std::future::ready(Ok(())))
        }
        fn gaps(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
            _range: SessionRange,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<CaptureGap>>> {
            Box::pin(std::future::ready(Ok(self.gaps.clone())))
        }
    }

    struct FakeFrames {
        metadata: Vec<CapturedFrame>,
        encoded: Vec<EncodedFrame>,
    }
    impl FrameSource for FakeFrames {
        fn frames_by_id(
            &self,
            ids: Vec<krometrail_core::FrameId>,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
            let result = ids
                .into_iter()
                .map(|id| {
                    self.encoded
                        .iter()
                        .find(|frame| frame.metadata().id() == id)
                        .cloned()
                })
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| live_error(ErrorCode::NotFound, "fake frame missing"));
            Box::pin(std::future::ready(result))
        }
        fn frame_metadata_by_id(
            &self,
            ids: Vec<krometrail_core::FrameId>,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
            let result = ids
                .into_iter()
                .map(|id| self.metadata.iter().find(|frame| frame.id() == id).cloned())
                .collect::<Option<Vec<_>>>()
                .ok_or_else(|| live_error(ErrorCode::NotFound, "fake metadata missing"));
            Box::pin(std::future::ready(result))
        }
        fn frames_in_range(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
            _range: SessionRange,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
            Box::pin(std::future::ready(Ok(self.encoded.clone())))
        }
        fn frames_in_ordinal_range(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
            _start: CaptureOrdinal,
            _end: CaptureOrdinal,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<EncodedFrame>>> {
            Box::pin(std::future::ready(Ok(self.encoded.clone())))
        }
        fn frame_metadata_in_range(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
            _range: SessionRange,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
            Box::pin(std::future::ready(Ok(self.metadata.clone())))
        }
        fn frame_metadata_in_ordinal_range(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
            _start: CaptureOrdinal,
            _end: CaptureOrdinal,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<CapturedFrame>>> {
            Box::pin(std::future::ready(Ok(self.metadata.clone())))
        }
        fn frame_availability(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
        ) -> PortFuture<'_, krometrail_core::Result<krometrail_core::FrameAvailability>> {
            Box::pin(std::future::ready(Err(live_error(
                ErrorCode::Unsupported,
                "fake availability unused",
            ))))
        }
    }

    struct FakeInteractions {
        anchor: InteractionAnchor,
    }
    impl InteractionAnchorSource for FakeInteractions {
        fn interaction_anchor(
            &self,
            _interaction: InteractionId,
        ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
            Box::pin(std::future::ready(Ok(Some(self.anchor.clone()))))
        }
        fn latest_interaction_anchor(
            &self,
            _session: krometrail_core::SessionId,
            _target: TargetId,
        ) -> PortFuture<'_, krometrail_core::Result<Option<InteractionAnchor>>> {
            Box::pin(std::future::ready(Ok(Some(self.anchor.clone()))))
        }
    }

    fn ids() -> (
        krometrail_core::SessionId,
        TargetId,
        InteractionId,
        Vec<FrameId>,
    ) {
        (
            krometrail_core::SessionId::from_uuid(Uuid::from_u128(1)),
            TargetId::from_uuid(Uuid::from_u128(2)),
            InteractionId::from_uuid(Uuid::from_u128(3)),
            vec![
                FrameId::from_uuid(Uuid::from_u128(4)),
                FrameId::from_uuid(Uuid::from_u128(5)),
            ],
        )
    }

    fn fake_frame(
        session: krometrail_core::SessionId,
        target: TargetId,
        id: FrameId,
        ordinal: u64,
        session_ns: u64,
    ) -> EncodedFrame {
        let metadata = CapturedFrame::new(
            id,
            session,
            target,
            CaptureOrdinal::new(ordinal).unwrap(),
            Some(SourceTime::from_nanos(10 + ordinal as i128)),
            ObservedTime::from_nanos(30 + session_ns),
            SessionTime::from_nanos(session_ns),
            ImageFormat::Png,
            krometrail_core::PixelDimensions::new(800, 450).unwrap(),
            krometrail_core::PixelDimensions::new(800, 450).unwrap(),
            DeviceScaleFactor::new(1.0).unwrap(),
            Vec::new(),
        )
        .unwrap();
        EncodedFrame::new(metadata, vec![ordinal as u8, 1, 2]).unwrap()
    }

    fn fake_range(
        session: krometrail_core::SessionId,
        target: TargetId,
        interaction: InteractionId,
        frames: &[FrameId],
        gaps: Vec<CaptureGap>,
    ) -> krometrail_core::ResolvedRange {
        let range =
            SessionRange::new(SessionTime::from_nanos(100), SessionTime::from_nanos(200)).unwrap();
        krometrail_core::ResolvedRange::new_with_anchor(
            session,
            target,
            krometrail_core::TemporalRangeAnchorKind::Interaction,
            krometrail_core::ResolvedAnchor::new(
                krometrail_core::ResolvedAnchorReference::Interaction {
                    interaction_id: interaction,
                },
                SessionTime::from_nanos(150),
                SessionTime::from_nanos(150),
            )
            .unwrap(),
            range,
            range,
            frames.to_vec(),
            vec![interaction],
            Vec::new(),
            Vec::new(),
            gaps,
            Vec::new(),
            krometrail_core::RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn interval_preserves_ordered_identities_clocks_hashes_and_declared_gaps() {
        let (session, target, interaction, frame_ids) = ids();
        let anchor = InteractionAnchor::new(
            interaction,
            session,
            target,
            BrowserOperationKind::Click,
            InteractionTiming::new(
                SessionTime::from_nanos(100),
                SessionTime::from_nanos(110),
                SessionTime::from_nanos(120),
                Some(SessionTime::from_nanos(130)),
            )
            .unwrap(),
        )
        .unwrap();
        let gap = CaptureGap::new(
            krometrail_core::GapId::from_uuid(Uuid::from_u128(6)),
            session,
            target,
            SessionRange::new(SessionTime::from_nanos(140), SessionTime::from_nanos(145)).unwrap(),
            ObservedTime::from_nanos(145),
            krometrail_core::CaptureGapReason::CaptureStopped,
            NonZeroU64::new(1),
            Some("scripted declared gap".into()),
        )
        .unwrap();
        let frames = vec![
            fake_frame(session, target, frame_ids[0], 7, 150),
            fake_frame(session, target, frame_ids[1], 9, 180),
        ];
        let authorities = IntervalAuthorities {
            query: &FakeQuery {
                range: fake_range(session, target, interaction, &frame_ids, vec![gap.clone()]),
            },
            frames: &FakeFrames {
                metadata: frames
                    .iter()
                    .map(|frame| frame.metadata().clone())
                    .collect(),
                encoded: frames.clone(),
            },
            gaps: &FakeGaps {
                gaps: vec![gap.clone()],
            },
            interactions: &FakeInteractions { anchor },
        };
        let interval = source_interval_for_interaction(&authorities, session, target, interaction)
            .await
            .unwrap();
        assert_eq!(
            interval.frame_ids(),
            frame_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(interval.frames[0].capture_ordinal, 7);
        assert_eq!(interval.frames[1].capture_ordinal, 9);
        assert_eq!(interval.frames[0].source_time_ns, Some(17));
        assert_eq!(interval.frames[0].observed_time_ns, 180);
        assert_eq!(interval.frames[0].session_time_ns, 150);
        assert_eq!(interval.gap_ids(), vec![gap.id().to_string()]);
        assert_eq!(
            interval.frames[0].encoded_sha256,
            temporal_evaluation::sha256_prefixed(&[7, 1, 2])
        );
        assert_eq!(interval.retention, RetentionState::Retained);
    }

    #[test]
    fn canonical_matrix_and_manifest_identity_have_one_order_and_no_duplicate_registry() {
        let definition = BenchmarkDefinition::canonical();
        let trials = canonical_capture_trials(&definition).unwrap();
        assert_eq!(
            trials.len(),
            definition.cases.len()
                * definition.duration_ms.len()
                * usize::from(definition.matrix.capture_repetitions)
        );
        assert_eq!(
            trials.first().unwrap().trial_id,
            "capture:movement-reversal/basic/16/0"
        );
        assert_eq!(
            trials.last().unwrap().trial_id,
            "capture:stable/smooth-panel/200/29"
        );
        let identities = canonical_manifest_trials(&definition).unwrap();
        assert_eq!(identities.len(), trials.len());
        let unique = identities
            .iter()
            .map(|trial| trial.trial_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), identities.len());
        assert!(
            identities
                .iter()
                .all(|trial| trial.condition_id == ConditionId::AFinalScreenshot)
        );
    }

    #[test]
    fn scripted_barriers_reject_out_of_order_evidence() {
        assert!(barrier_order_is_valid(&[
            CaptureBarrier::SessionReady,
            CaptureBarrier::TargetReady,
            CaptureBarrier::ViewportVerified,
            CaptureBarrier::Navigated,
            CaptureBarrier::Clicked,
            CaptureBarrier::Settled,
            CaptureBarrier::IntervalResolved,
        ]));
        assert!(!barrier_order_is_valid(&[
            CaptureBarrier::SessionReady,
            CaptureBarrier::TargetReady,
            CaptureBarrier::Clicked,
            CaptureBarrier::ViewportVerified,
        ]));
    }

    #[test]
    fn interaction_window_is_bounded_and_not_based_on_duration() {
        let (session, target, interaction, _) = ids();
        let request = interaction_interval_request(session, target, interaction).unwrap();
        let TemporalRangeAnchor::Interaction {
            window: Some(window),
            ..
        } = request.anchor
        else {
            panic!("interaction anchor expected")
        };
        assert_eq!(window.before(), INTERVAL_BEFORE);
        assert_eq!(window.after(), INTERVAL_AFTER);
    }

    #[test]
    fn fixture_drift_is_rejected_before_launch() {
        let definition = BenchmarkDefinition::canonical();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_ROOT);
        let mut files = definition
            .fixture
            .files
            .iter()
            .map(|file| (file.path.clone(), fs::read(root.join(&file.path)).unwrap()))
            .collect::<BTreeMap<_, _>>();
        files.get_mut("benchmark.js").unwrap().push(b' ');
        assert!(validate_fixture_sources(&definition, &files).is_err());
    }

    #[test]
    fn per_duration_statuses_are_explicit_and_only_supported_thresholds_decide() {
        assert_eq!(capture_threshold(16), None);
        assert_eq!(capture_threshold(33), None);
        assert_eq!(capture_threshold(50), Some(8_000));
        assert_eq!(capture_threshold(100), Some(9_500));
        assert_eq!(capture_threshold(200), Some(9_500));
        assert_eq!(basis_points(19, 20), 9_500);
    }
}
