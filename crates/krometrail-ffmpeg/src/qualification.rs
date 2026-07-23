use std::{sync::Arc, time::Instant};

use krometrail_core::{
    CancellationSignal, DeviceScaleFactor, FrameId, ImageFormat, PresentationRange,
    PresentationTime, SessionRange, SessionTime, VideoEncodeFrame, VideoEncodeRequest,
    VideoEncoderIdentity, VideoEncodingProfile, VideoOutputGeometry, VideoPresentationPlan,
    VideoPresentationPolicy, VideoPresentationSegment, VideoSegmentSource, VideoTimingBasis,
    VisualEpoch,
};
use sha2::{Digest, Sha256};

use crate::{
    FFMPEG_ADAPTER_VERSION,
    control::OperationControl,
    discovery::{FfmpegDiscoveryOptions, QualifiedExecutable, discover_candidates},
    encoder::QualifiedFfmpegEncoder,
    error::{AdapterFailure, AdapterFailureKind, AdapterFailureStage},
    job::PreparedEncodeJob,
    mp4::{OutputValidationDetail, ValidatedMp4, validate_mp4},
    policy::{
        FFMPEG_ARGUMENT_POLICY_VERSION, FFMPEG_QUALIFICATION_TIMEOUT, MAX_FFMPEG_STDERR_BYTES,
        MAX_FFMPEG_VERSION_REPORT_BYTES,
    },
    process::{FfmpegInvocation, ManagedFfmpegProcess, ProcessLimits},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegQualificationStage {
    Discovery,
    ExecutableIdentity,
    VersionProbe,
    EncodeProbe,
    OutputValidation,
    ProcessCleanup,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FfmpegUnavailableReason {
    NotFound,
    InvalidCandidate,
    ChangedCandidate,
    TimedOut,
    UnsupportedEncoder,
    InvalidOutput,
    ProcessFailed,
    Cancelled,
    UnsupportedPlatform,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FfmpegUnavailable {
    pub stage: FfmpegQualificationStage,
    pub reason: FfmpegUnavailableReason,
    pub output_check: Option<OutputValidationDetail>,
}

pub enum FfmpegQualification {
    Qualified(Arc<QualifiedFfmpegEncoder>),
    Unavailable(FfmpegUnavailable),
}

pub async fn qualify_ffmpeg(
    options: FfmpegDiscoveryOptions,
    cancellation: Arc<dyn CancellationSignal>,
    deadline: Instant,
) -> FfmpegQualification {
    if !cfg!(any(target_os = "linux", target_os = "macos")) {
        return unavailable(
            FfmpegQualificationStage::Discovery,
            FfmpegUnavailableReason::UnsupportedPlatform,
        );
    }
    let qualification_deadline = deadline.min(Instant::now() + FFMPEG_QUALIFICATION_TIMEOUT);
    let control = OperationControl::new(cancellation, qualification_deadline);
    if let Err(failure) = control.check(AdapterFailureStage::ExecutableIdentity) {
        return FfmpegQualification::Unavailable(map_failure(&failure));
    }
    let explicit = options.is_explicit();
    let candidates = match discover_candidates(&options, &control).await {
        Ok(candidates) => candidates,
        Err(failure) => return FfmpegQualification::Unavailable(map_failure(&failure)),
    };
    if candidates.is_empty() {
        return unavailable(
            FfmpegQualificationStage::Discovery,
            FfmpegUnavailableReason::NotFound,
        );
    }

    let mut final_failure = FfmpegUnavailable {
        stage: FfmpegQualificationStage::Discovery,
        reason: FfmpegUnavailableReason::NotFound,
        output_check: None,
    };
    for candidate in candidates {
        if let Err(failure) = control.check(AdapterFailureStage::ExecutableIdentity) {
            return FfmpegQualification::Unavailable(map_failure(&failure));
        }
        let executable = match QualifiedExecutable::load(candidate, &control).await {
            Ok(executable) => executable,
            Err(failure) => {
                failure.trace();
                final_failure = map_failure(&failure);
                if explicit {
                    return FfmpegQualification::Unavailable(final_failure);
                }
                continue;
            }
        };
        match qualify_candidate(executable, &control).await {
            Ok(encoder) => return FfmpegQualification::Qualified(Arc::new(encoder)),
            Err(failure) => {
                failure.trace();
                final_failure = map_failure(&failure);
                if matches!(
                    final_failure.reason,
                    FfmpegUnavailableReason::Cancelled | FfmpegUnavailableReason::TimedOut
                ) || explicit
                {
                    return FfmpegQualification::Unavailable(final_failure);
                }
            }
        }
    }
    FfmpegQualification::Unavailable(final_failure)
}

async fn qualify_candidate(
    executable: QualifiedExecutable,
    control: &OperationControl,
) -> Result<QualifiedFfmpegEncoder, AdapterFailure> {
    executable.validate_unchanged(control).await?;
    let version = run_version_probe(&executable, control).await?;
    let build_report_sha256 = hash_build_report(&version.stdout, &version.stderr);
    let implementation_version =
        implementation_version(&version.stdout, &version.stderr, &build_report_sha256);

    let request = qualification_request()?;
    let validated = encode_validated(&executable, &request, control)
        .await
        .map_err(|failure| match failure.stage {
            AdapterFailureStage::OutputValidation | AdapterFailureStage::ProcessCleanup => failure,
            _ => failure.at_stage(AdapterFailureStage::EncodeProbe),
        })?;
    if validated.bytes.is_empty() {
        return Err(AdapterFailure::new(
            AdapterFailureStage::OutputValidation,
            AdapterFailureKind::InvalidOutput,
        ));
    }
    let identity = VideoEncoderIdentity::new(
        implementation_version,
        build_report_sha256,
        "libx264",
        FFMPEG_ADAPTER_VERSION,
        FFMPEG_ARGUMENT_POLICY_VERSION,
    )
    .map_err(|_| {
        AdapterFailure::new(
            AdapterFailureStage::ExecutableIdentity,
            AdapterFailureKind::InvalidCandidate,
        )
    })?;
    tracing::info!(
        event = "ffmpeg.qualification.succeeded",
        encoder = identity.encoder_name(),
        adapter_version = identity.adapter_version(),
        argument_policy = identity.argument_policy_version(),
        executable_sha256 = %HexDigest(*executable.executable_sha256()),
        "qualified user-installed FFmpeg encoder"
    );
    Ok(QualifiedFfmpegEncoder::new(executable, identity))
}

async fn run_version_probe(
    executable: &QualifiedExecutable,
    control: &OperationControl,
) -> Result<crate::process::SanitizedProcessOutcome, AdapterFailure> {
    executable.validate_unchanged(control).await?;
    let limits = process_limits(
        control.deadline(),
        MAX_FFMPEG_VERSION_REPORT_BYTES,
        MAX_FFMPEG_VERSION_REPORT_BYTES,
    )?;
    let mut process =
        ManagedFfmpegProcess::spawn(executable.path(), FfmpegInvocation::VersionProbe, limits)
            .await
            .map_err(|failure| failure.at_stage(AdapterFailureStage::VersionProbe))?;
    process
        .wait_or_cancel(control.cancellation(), limits)
        .await
        .map_err(|failure| failure.at_stage(AdapterFailureStage::VersionProbe))
}

pub(crate) async fn encode_validated(
    executable: &QualifiedExecutable,
    request: &VideoEncodeRequest,
    control: &OperationControl,
) -> Result<ValidatedMp4, AdapterFailure> {
    control.check(AdapterFailureStage::InputStaging)?;
    executable.validate_unchanged(control).await?;
    let job = PreparedEncodeJob::from_request(request, control).await?;
    executable.validate_unchanged(control).await?;
    let limits = process_limits(control.deadline(), 4 * 1024, MAX_FFMPEG_STDERR_BYTES)?;
    let invocation = FfmpegInvocation::Encode {
        arguments: job.arguments(),
        working_directory: job.workspace(),
        output_path: job.output_path(),
        output_limit: job.output_limit(),
    };
    let mut process = match ManagedFfmpegProcess::spawn(executable.path(), invocation, limits).await
    {
        Ok(process) => process,
        Err(failure) if failure.kind == AdapterFailureKind::Spawn => {
            match executable.validate_unchanged(control).await {
                Err(identity_failure)
                    if matches!(
                        identity_failure.kind,
                        AdapterFailureKind::InvalidCandidate | AdapterFailureKind::ChangedCandidate
                    ) =>
                {
                    return Err(AdapterFailure::new(
                        AdapterFailureStage::Spawn,
                        AdapterFailureKind::ChangedCandidate,
                    ));
                }
                Err(control_failure) => return Err(control_failure),
                Ok(()) => return Err(failure),
            }
        }
        Err(failure) => return Err(failure),
    };
    let outcome = process
        .wait_or_cancel(control.cancellation(), limits)
        .await?;
    tracing::debug!(
        event = "ffmpeg.encode.process_completed",
        stderr_bytes = outcome.stderr.len(),
        diagnostic_sha256 = %HexDigest(outcome.diagnostic_sha256),
        "FFmpeg process completed with bounded diagnostics"
    );
    let expected = job.expected();
    control
        .run_blocking(AdapterFailureStage::OutputValidation, move |control| {
            let bytes = job.read_output_blocking(&control)?;
            validate_mp4(bytes, expected, &control)
        })
        .await
}

fn process_limits(
    deadline: Instant,
    stdout_bytes: usize,
    stderr_bytes: usize,
) -> Result<ProcessLimits, AdapterFailure> {
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .ok_or_else(|| {
            AdapterFailure::new(
                AdapterFailureStage::ProcessWait,
                AdapterFailureKind::Deadline,
            )
        })?;
    let cpu_seconds = remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() != 0))
        .max(1);
    Ok(ProcessLimits {
        deadline,
        cpu_seconds,
        stdout_bytes,
        stderr_bytes,
    })
}

fn qualification_request() -> Result<VideoEncodeRequest, AdapterFailure> {
    let first: FrameId = "00000000-0000-0000-0000-000000000001"
        .parse()
        .map_err(|_| qualification_contract_failure())?;
    let second: FrameId = "00000000-0000-0000-0000-000000000002"
        .parse()
        .map_err(|_| qualification_contract_failure())?;
    let dimensions = krometrail_core::PixelDimensions::new(2, 2)
        .map_err(|_| qualification_contract_failure())?;
    let geometry = VideoOutputGeometry::new(dimensions, dimensions, dimensions)
        .map_err(|_| qualification_contract_failure())?;
    let second_time = SessionTime::from_nanos(100_000_000);
    let first_source = VideoSegmentSource::source_frame(first, SessionTime::ZERO)
        .map_err(|_| qualification_contract_failure())?;
    let second_source = VideoSegmentSource::source_frame(second, second_time)
        .map_err(|_| qualification_contract_failure())?;
    let range = SessionRange::new(SessionTime::ZERO, second_time)
        .map_err(|_| qualification_contract_failure())?;
    let plan = VideoPresentationPlan::new(
        VideoPresentationPolicy::RealTime,
        range,
        range,
        range,
        VisualEpoch {
            index: 0,
            frame_ids: vec![first, second],
            image: dimensions,
            viewport: dimensions,
            device_scale_factor: DeviceScaleFactor::new(1.0)
                .map_err(|_| qualification_contract_failure())?,
        },
        vec![first, second],
        vec![SessionTime::ZERO, second_time],
        vec![],
        vec![
            VideoPresentationSegment::new(
                0,
                first_source.clone(),
                PresentationRange::new(
                    PresentationTime::ZERO,
                    PresentationTime::from_nanos(100_000_000)
                        .map_err(|_| qualification_contract_failure())?,
                )
                .map_err(|_| qualification_contract_failure())?,
                VideoTimingBasis::RecordedDelta,
            )
            .map_err(|_| qualification_contract_failure())?,
            VideoPresentationSegment::new(
                1,
                second_source.clone(),
                PresentationRange::new(
                    PresentationTime::from_nanos(100_000_000)
                        .map_err(|_| qualification_contract_failure())?,
                    PresentationTime::from_nanos(350_000_000)
                        .map_err(|_| qualification_contract_failure())?,
                )
                .map_err(|_| qualification_contract_failure())?,
                VideoTimingBasis::TerminalHold,
            )
            .map_err(|_| qualification_contract_failure())?,
        ],
        geometry,
    )
    .map_err(|_| qualification_contract_failure())?;
    let first_png = qualification_png([0, 0, 0, 255])?;
    let second_png = qualification_png([255, 255, 255, 255])?;
    let frames = vec![
        VideoEncodeFrame::new(0, first_source, ImageFormat::Png, dimensions, first_png)
            .map_err(|_| qualification_contract_failure())?,
        VideoEncodeFrame::new(1, second_source, ImageFormat::Png, dimensions, second_png)
            .map_err(|_| qualification_contract_failure())?,
    ];
    let profile = VideoEncodingProfile::new(geometry, 1_000_000)
        .map_err(|_| qualification_contract_failure())?;
    VideoEncodeRequest::new(plan, frames, profile).map_err(|_| qualification_contract_failure())
}

fn qualification_png(color: [u8; 4]) -> Result<Vec<u8>, AdapterFailure> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|_| qualification_contract_failure())?;
        let mut pixels = [0_u8; 16];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
        writer
            .write_image_data(&pixels)
            .map_err(|_| qualification_contract_failure())?;
    }
    Ok(output)
}

fn qualification_contract_failure() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::EncodeProbe,
        AdapterFailureKind::Internal,
    )
}

fn hash_build_report(stdout: &[u8], stderr: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"krometrail-ffmpeg-build-report-v1\0");
    digest.update((stdout.len() as u64).to_be_bytes());
    digest.update(stdout);
    digest.update((stderr.len() as u64).to_be_bytes());
    digest.update(stderr);
    digest.finalize().into()
}

fn implementation_version(stdout: &[u8], stderr: &[u8], digest: &[u8; 32]) -> String {
    let line = stdout
        .split(|byte| *byte == b'\n' || *byte == b'\r')
        .chain(stderr.split(|byte| *byte == b'\n' || *byte == b'\r'))
        .find(|line| !line.is_empty());
    if let Some(line) = line {
        let line = String::from_utf8_lossy(line);
        let mut words = line.split_ascii_whitespace();
        if words.next() == Some("ffmpeg")
            && words.next() == Some("version")
            && let Some(version) = words.next()
            && version.len() <= 200
            && version
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
        {
            return format!("ffmpeg {version}");
        }
    }
    format!("ffmpeg-build-{}", &HexDigest(*digest).to_string()[..16])
}

fn map_failure(failure: &AdapterFailure) -> FfmpegUnavailable {
    let stage = match failure.stage {
        AdapterFailureStage::ExecutableIdentity => FfmpegQualificationStage::ExecutableIdentity,
        AdapterFailureStage::VersionProbe => FfmpegQualificationStage::VersionProbe,
        AdapterFailureStage::EncodeProbe
        | AdapterFailureStage::InputStaging
        | AdapterFailureStage::Spawn
        | AdapterFailureStage::ProcessWait => FfmpegQualificationStage::EncodeProbe,
        AdapterFailureStage::OutputValidation => FfmpegQualificationStage::OutputValidation,
        AdapterFailureStage::ProcessCleanup => FfmpegQualificationStage::ProcessCleanup,
    };
    let reason = match failure.kind {
        AdapterFailureKind::Cancelled => FfmpegUnavailableReason::Cancelled,
        AdapterFailureKind::Deadline => FfmpegUnavailableReason::TimedOut,
        AdapterFailureKind::InvalidCandidate => FfmpegUnavailableReason::InvalidCandidate,
        AdapterFailureKind::ChangedCandidate => FfmpegUnavailableReason::ChangedCandidate,
        AdapterFailureKind::InvalidOutput
        | AdapterFailureKind::OutputOverflow
        | AdapterFailureKind::UnrepresentableTiming => FfmpegUnavailableReason::InvalidOutput,
        AdapterFailureKind::ProcessExit if stage == FfmpegQualificationStage::EncodeProbe => {
            FfmpegUnavailableReason::UnsupportedEncoder
        }
        AdapterFailureKind::Spawn
        | AdapterFailureKind::ProcessExit
        | AdapterFailureKind::ProcessIo
        | AdapterFailureKind::ProcessCleanup
        | AdapterFailureKind::StdoutOverflow
        | AdapterFailureKind::DiagnosticOverflow
        | AdapterFailureKind::Internal => FfmpegUnavailableReason::ProcessFailed,
    };
    FfmpegUnavailable {
        stage,
        reason,
        output_check: failure.output_check,
    }
}

fn unavailable(
    stage: FfmpegQualificationStage,
    reason: FfmpegUnavailableReason,
) -> FfmpegQualification {
    FfmpegQualification::Unavailable(FfmpegUnavailable {
        stage,
        reason,
        output_check: None,
    })
}

struct HexDigest([u8; 32]);

impl std::fmt::Display for HexDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn implementation_label_is_restricted_or_digest_derived() {
        let digest = [0xab; 32];
        assert_eq!(
            implementation_version(b"ffmpeg version 8.0.1 Copyright\n", b"", &digest),
            "ffmpeg 8.0.1"
        );
        let fallback = implementation_version(b"/private/path/ffmpeg version bad\n", b"", &digest);
        assert_eq!(fallback, "ffmpeg-build-abababababababab");
        assert!(!fallback.contains('/') && !fallback.contains('\\'));
    }

    #[test]
    fn build_report_digest_frames_stdout_and_stderr_exactly() {
        assert_ne!(
            hash_build_report(b"a", b"bc"),
            hash_build_report(b"ab", b"c")
        );
        assert_ne!(hash_build_report(b"a", b""), hash_build_report(b"a\n", b""));
    }
}
