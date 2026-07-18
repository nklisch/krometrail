mod support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use krometrail_core::{CancellationSignal, PortFuture};
use krometrail_ffmpeg::{
    FfmpegDiscoveryOptions, FfmpegQualification, FfmpegQualificationStage, FfmpegUnavailableReason,
    qualify_ffmpeg,
};

struct NeverCancelled;

impl CancellationSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(std::future::pending())
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
