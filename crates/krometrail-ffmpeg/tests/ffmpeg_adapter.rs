mod support;

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use krometrail_core::{
    CancellationSignal, ErrorCode, PortFuture, TemporalVideoEncoder, VideoEncodingContext,
};
use krometrail_ffmpeg::{
    FfmpegDiscoveryOptions, FfmpegQualification, FfmpegQualificationStage, FfmpegUnavailableReason,
    Mp4Check, Mp4Property, OutputValidationDetail, qualify_ffmpeg,
};
use sha2::Digest as _;

struct NeverCancelled;

impl CancellationSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}

struct ManualCancellation {
    cancelled: AtomicBool,
    notify: tokio::sync::Notify,
}

impl ManualCancellation {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            cancelled: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
        })
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
    }
}

impl CancellationSignal for ManualCancellation {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(async move {
            while !self.is_cancelled() {
                self.notify.notified().await;
            }
        })
    }
}

fn cancellation() -> Arc<dyn CancellationSignal> {
    Arc::new(NeverCancelled)
}

async fn qualify(fixture: &support::FixtureExecutable) -> FfmpegQualification {
    qualify_ffmpeg(
        FfmpegDiscoveryOptions::with_explicit_executable(fixture.path().to_owned()),
        cancellation(),
        Instant::now() + Duration::from_secs(5),
    )
    .await
}

async fn qualified(
    mode: &str,
) -> (
    support::FixtureExecutable,
    Arc<krometrail_ffmpeg::QualifiedFfmpegEncoder>,
) {
    let fixture = support::FixtureExecutable::new(mode);
    let FfmpegQualification::Qualified(encoder) = qualify(&fixture).await else {
        panic!("fixture must qualify before its request-time mode activates");
    };
    fixture.clear_observation();
    (fixture, encoder)
}

fn context(cancellation: Arc<dyn CancellationSignal>, duration: Duration) -> VideoEncodingContext {
    VideoEncodingContext {
        deadline: Instant::now() + duration,
        cancellation,
    }
}

async fn wait_for_active_pid(fixture: &support::FixtureExecutable) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(pid) = fixture.active_pid() {
            return pid;
        }
        assert!(Instant::now() < deadline, "compiled fixture did not start");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn produced_contract_qualification_returns_safe_exact_identity() {
    let fixture = support::FixtureExecutable::new("valid");
    let FfmpegQualification::Qualified(encoder) = qualify(&fixture).await else {
        panic!("compiled valid fixture must qualify");
    };
    assert_eq!(
        encoder.identity().implementation_version(),
        "ffmpeg fixture-1"
    );
    assert_eq!(encoder.identity().encoder_name(), "libx264");
    assert_eq!(
        encoder.identity().argument_policy_version(),
        "krometrail-ffmpeg-h264-v1"
    );
    assert!(!encoder.identity().implementation_version().contains('/'));
    assert!(!encoder.identity().implementation_version().contains('\\'));
}

#[tokio::test]
async fn terminal_hold_zero_fixture_qualifies_through_the_full_path() {
    let fixture = support::FixtureExecutable::new("terminal-hold-zero");
    assert!(matches!(
        qualify(&fixture).await,
        FfmpegQualification::Qualified(_)
    ));
}

#[tokio::test]
async fn output_validation_detail_reaches_unavailable_mapping() {
    let fixture = support::FixtureExecutable::new("wrong-dimensions");
    let FfmpegQualification::Unavailable(unavailable) = qualify(&fixture).await else {
        panic!("wrong dimensions must not qualify");
    };
    assert_eq!(
        unavailable.stage,
        FfmpegQualificationStage::OutputValidation
    );
    assert_eq!(unavailable.reason, FfmpegUnavailableReason::InvalidOutput);
    assert_eq!(
        unavailable.output_check,
        Some(OutputValidationDetail {
            check: Mp4Check::VideoDimensions,
            expected: Mp4Property::Dimensions {
                width: 2,
                height: 2,
            },
            observed: Mp4Property::Dimensions {
                width: 4,
                height: 2,
            },
        })
    );
}

#[tokio::test]
async fn version_only_or_failed_encoder_never_qualifies() {
    let invalid = support::FixtureExecutable::new("invalid");
    let FfmpegQualification::Unavailable(unavailable) = qualify(&invalid).await else {
        panic!("invalid MP4 must not qualify");
    };
    assert_eq!(
        unavailable.stage,
        FfmpegQualificationStage::OutputValidation
    );
    assert_eq!(unavailable.reason, FfmpegUnavailableReason::InvalidOutput);

    let exit = support::FixtureExecutable::new("exit");
    let FfmpegQualification::Unavailable(unavailable) = qualify(&exit).await else {
        panic!("failed libx264 encode must not qualify");
    };
    assert_eq!(unavailable.stage, FfmpegQualificationStage::EncodeProbe);
    assert_eq!(
        unavailable.reason,
        FfmpegUnavailableReason::UnsupportedEncoder
    );
}

#[tokio::test]
async fn invalid_explicit_candidate_is_path_free_and_never_falls_back() {
    let private_path = PathBuf::from("relative/private/ffmpeg");
    let result = qualify_ffmpeg(
        FfmpegDiscoveryOptions::with_explicit_executable(private_path.clone()),
        cancellation(),
        Instant::now() + Duration::from_secs(5),
    )
    .await;
    let FfmpegQualification::Unavailable(unavailable) = result else {
        panic!("relative explicit path must fail");
    };
    assert_eq!(
        unavailable.stage,
        FfmpegQualificationStage::ExecutableIdentity
    );
    assert_eq!(
        unavailable.reason,
        FfmpegUnavailableReason::InvalidCandidate
    );
    assert!(!format!("{unavailable:?}").contains(private_path.to_string_lossy().as_ref()));
}

#[tokio::test]
async fn bounded_version_report_and_build_identity_are_exact() {
    let first = support::FixtureExecutable::new("valid");
    let second = support::FixtureExecutable::new("valid-version2");
    let FfmpegQualification::Qualified(first) = qualify(&first).await else {
        panic!("first fixture must qualify");
    };
    let FfmpegQualification::Qualified(second) = qualify(&second).await else {
        panic!("second fixture must qualify");
    };
    assert_ne!(
        first.identity().build_report_sha256(),
        second.identity().build_report_sha256()
    );
    assert_ne!(
        first.identity().implementation_version(),
        second.identity().implementation_version()
    );

    let overflow = support::FixtureExecutable::new("version-overflow");
    let FfmpegQualification::Unavailable(unavailable) = qualify(&overflow).await else {
        panic!("oversized version report must fail");
    };
    assert_eq!(unavailable.stage, FfmpegQualificationStage::VersionProbe);
    assert_eq!(unavailable.reason, FfmpegUnavailableReason::ProcessFailed);
}

#[tokio::test]
async fn snapshotted_path_search_discovers_only_the_exact_platform_name() {
    let fixture = support::FixtureExecutable::new("valid");
    let search = std::env::join_paths([fixture.path().parent().unwrap()]).unwrap();
    let result = qualify_ffmpeg(
        FfmpegDiscoveryOptions::with_search_path(search),
        cancellation(),
        Instant::now() + Duration::from_secs(5),
    )
    .await;
    assert!(matches!(result, FfmpegQualification::Qualified(_)));
}

#[tokio::test]
async fn qualified_adapter_is_object_safe_and_returns_core_validated_bytes() {
    let (_fixture, encoder) = qualified("valid").await;
    let encoder: Arc<dyn TemporalVideoEncoder> = encoder;
    let clip = encoder
        .encode(
            support::video_request(),
            context(cancellation(), Duration::from_secs(5)),
        )
        .await
        .unwrap();
    assert_eq!(clip.identity(), encoder.identity());
    assert_eq!(clip.profile().max_encoded_bytes(), 1_000_000);
    assert_eq!(clip.encoded_bytes().len(), 1_508);
    assert_eq!(
        clip.output_hash().as_bytes(),
        temporal_vision::OutputHash::from_bytes(sha2::Sha256::digest(clip.encoded_bytes()).into())
            .as_bytes()
    );
}

#[tokio::test]
async fn request_failures_map_to_stable_safe_core_errors_and_remove_private_state() {
    for (mode, expected) in [
        (
            "invalid_after_qualification",
            ErrorCode::VideoEncodingFailed,
        ),
        ("exit_after_qualification", ErrorCode::VideoEncodingFailed),
        (
            "stderr-overflow_after_qualification",
            ErrorCode::VideoEncodingFailed,
        ),
        (
            "output-overflow_after_qualification",
            ErrorCode::ResourceLimitExceeded,
        ),
    ] {
        let (fixture, encoder) = qualified(mode).await;
        let error = encoder
            .encode(
                support::video_request(),
                context(cancellation(), Duration::from_secs(5)),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, expected, "mode {mode}");
        let private_workspace = fixture.working_directory().unwrap();
        assert!(
            !private_workspace.exists(),
            "mode {mode} leaked private state"
        );
        let rendered = format!("{error:?}");
        assert!(!rendered.contains(fixture.path().to_string_lossy().as_ref()));
        assert!(!rendered.contains(private_workspace.to_string_lossy().as_ref()));
    }
}

#[tokio::test]
async fn executable_drift_fails_as_unavailable_before_request_staging() {
    let (fixture, encoder) = qualified("valid").await;
    fixture.mutate_executable();
    let error = encoder
        .encode(
            support::video_request(),
            context(cancellation(), Duration::from_secs(5)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::VideoEncoderUnavailable);
    assert!(fixture.working_directory().is_none());
}

#[tokio::test]
async fn active_encode_honors_cancellation_and_deadline() {
    let (fixture, encoder) = qualified("hang_after_qualification").await;
    let active_cancellation = ManualCancellation::new();
    let trigger = Arc::clone(&active_cancellation);
    let trigger_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(40)).await;
        trigger.cancel();
    });
    let error = encoder
        .encode(
            support::video_request(),
            context(active_cancellation, Duration::from_secs(5)),
        )
        .await
        .unwrap_err();
    trigger_task.await.unwrap();
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(!fixture.working_directory().unwrap().exists());

    let (fixture, encoder) = qualified("hang_after_qualification").await;
    let error = encoder
        .encode(
            support::video_request(),
            context(cancellation(), Duration::from_millis(50)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::VideoEncodingFailed);
    assert!(!fixture.working_directory().unwrap().exists());
}

#[tokio::test]
async fn permit_wait_honors_cancellation_and_deadline() {
    let (fixture, encoder) = qualified("hang_after_qualification").await;
    let active_encoder = Arc::clone(&encoder);
    let active = tokio::spawn(async move {
        active_encoder
            .encode(
                support::video_request(),
                context(cancellation(), Duration::from_secs(5)),
            )
            .await
    });
    wait_for_active_pid(&fixture).await;

    let cancelled = ManualCancellation::new();
    cancelled.cancel();
    let error = encoder
        .encode(
            support::video_request(),
            context(cancelled, Duration::from_secs(5)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::Cancelled);

    let error = encoder
        .encode(
            support::video_request(),
            context(cancellation(), Duration::from_millis(40)),
        )
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::VideoEncodingFailed);
    active.abort();
    let _ = active.await;
}

#[cfg(unix)]
#[tokio::test]
async fn dropping_encode_future_force_kills_compiled_descendants() {
    let (fixture, encoder) = qualified("descendant_after_qualification").await;
    let active_encoder = Arc::clone(&encoder);
    let active = tokio::spawn(async move {
        active_encoder
            .encode(
                support::video_request(),
                context(cancellation(), Duration::from_secs(5)),
            )
            .await
    });
    let pid = wait_for_active_pid(&fixture).await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    active.abort();
    let _ = active.await;
    let group = libc::pid_t::try_from(pid).unwrap();
    let exists = unsafe { libc::kill(-group, 0) } == 0;
    assert!(
        !exists,
        "compiled FFmpeg descendant group survived future drop"
    );
    assert!(!fixture.working_directory().unwrap().exists());
}
