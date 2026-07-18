use std::{ffi::OsString, path::Path, time::Duration};

use krometrail_core::{ImageFormat, VideoEncodeRequest};
use tokio::io::AsyncWriteExt;

use crate::{
    error::{AdapterFailure, AdapterFailureKind, AdapterFailureStage},
    mp4::ExpectedMp4,
    policy::{CONCAT_FILE_NAME, OUTPUT_FILE_NAME, encode_arguments},
};

pub(crate) struct PreparedEncodeJob {
    workspace: tempfile::TempDir,
    arguments: Vec<OsString>,
    expected: ExpectedMp4,
    output_limit: u64,
}

impl PreparedEncodeJob {
    pub(crate) async fn from_request(request: &VideoEncodeRequest) -> Result<Self, AdapterFailure> {
        let workspace = tempfile::Builder::new()
            .prefix("krometrail-video-")
            .tempdir()
            .map_err(|_| staging_failure())?;
        restrict_workspace(workspace.path()).map_err(|_| staging_failure())?;

        let mut names = Vec::with_capacity(request.frames().len());
        for (index, frame) in request.frames().iter().enumerate() {
            let extension = match frame.format() {
                ImageFormat::Jpeg => "jpg",
                ImageFormat::Png => "png",
            };
            let name = format!("frame-{index:06}.{extension}");
            let path = workspace.path().join(&name);
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .await
                .map_err(|_| staging_failure())?;
            file.write_all(frame.bytes())
                .await
                .map_err(|_| staging_failure())?;
            file.flush().await.map_err(|_| staging_failure())?;
            names.push(name);
        }

        let (concat, duration_micros) = concat_document(request, &names)?;
        let mut concat_file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(workspace.path().join(CONCAT_FILE_NAME))
            .await
            .map_err(|_| staging_failure())?;
        concat_file
            .write_all(concat.as_bytes())
            .await
            .map_err(|_| staging_failure())?;
        concat_file.flush().await.map_err(|_| staging_failure())?;

        let output_limit = request.profile().max_encoded_bytes();
        let geometry = request.profile().geometry();
        Ok(Self {
            arguments: encode_arguments(geometry, duration_micros, output_limit),
            expected: ExpectedMp4 {
                canvas: geometry.canvas(),
                presentation_duration_micros: duration_micros,
                max_bytes: output_limit,
            },
            output_limit,
            workspace,
        })
    }

    pub(crate) fn workspace(&self) -> &Path {
        self.workspace.path()
    }

    pub(crate) fn arguments(&self) -> &[OsString] {
        &self.arguments
    }

    pub(crate) const fn expected(&self) -> ExpectedMp4 {
        self.expected
    }

    pub(crate) const fn output_limit(&self) -> u64 {
        self.output_limit
    }

    pub(crate) fn output_path(&self) -> std::path::PathBuf {
        self.workspace.path().join(OUTPUT_FILE_NAME)
    }
}

fn concat_document(
    request: &VideoEncodeRequest,
    names: &[String],
) -> Result<(String, u64), AdapterFailure> {
    if names.len() != request.plan().segments().len() || names.is_empty() {
        return Err(staging_failure());
    }
    let mut document = String::from("ffconcat version 1.0\n");
    let mut previous_endpoint = 0_u64;
    for (segment, name) in request.plan().segments().iter().zip(names) {
        let endpoint = round_nanos_to_micros(segment.presentation().end().as_nanos())?;
        let duration = endpoint
            .checked_sub(previous_endpoint)
            .filter(|duration| *duration > 0)
            .ok_or_else(staging_failure)?;
        document.push_str("file ");
        document.push_str(name);
        document.push('\n');
        document.push_str("duration ");
        document.push_str(&format_duration(duration));
        document.push('\n');
        previous_endpoint = endpoint;
    }
    document.push_str("file ");
    document.push_str(names.last().expect("validated names are non-empty"));
    document.push('\n');
    Ok((document, previous_endpoint))
}

fn round_nanos_to_micros(nanos: u64) -> Result<u64, AdapterFailure> {
    nanos
        .checked_add(500)
        .map(|rounded| rounded / 1_000)
        .ok_or_else(staging_failure)
}

fn format_duration(micros: u64) -> String {
    let duration = Duration::from_micros(micros);
    format!("{}.{:06}", duration.as_secs(), duration.subsec_micros())
}

fn staging_failure() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::InputStaging,
        AdapterFailureKind::Internal,
    )
}

#[cfg(unix)]
fn restrict_workspace(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_workspace(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        DeviceScaleFactor, FrameId, PresentationRange, PresentationTime, SessionRange, SessionTime,
        VideoEncodeFrame, VideoEncodingProfile, VideoOutputGeometry, VideoPresentationPlan,
        VideoPresentationPolicy, VideoPresentationSegment, VideoSegmentSource, VideoTimingBasis,
        VisualEpoch,
    };

    fn test_request() -> VideoEncodeRequest {
        let first: FrameId = "00000000-0000-0000-0000-000000000001".parse().unwrap();
        let second: FrameId = "00000000-0000-0000-0000-000000000002".parse().unwrap();
        let dimensions = krometrail_core::PixelDimensions::new(2, 2).unwrap();
        let geometry = VideoOutputGeometry::new(dimensions, dimensions, dimensions).unwrap();
        let first_source = VideoSegmentSource::source_frame(first, SessionTime::ZERO).unwrap();
        let second_source =
            VideoSegmentSource::source_frame(second, SessionTime::from_nanos(100_000_000)).unwrap();
        let plan = VideoPresentationPlan::new(
            VideoPresentationPolicy::RealTime,
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100_000_000)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100_000_000)).unwrap(),
            SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(100_000_000)).unwrap(),
            VisualEpoch {
                index: 0,
                frame_ids: vec![first, second],
                image: dimensions,
                viewport: dimensions,
                device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
            },
            vec![first, second],
            vec![SessionTime::ZERO, SessionTime::from_nanos(100_000_000)],
            vec![],
            vec![
                VideoPresentationSegment::new(
                    0,
                    first_source.clone(),
                    PresentationRange::new(
                        PresentationTime::ZERO,
                        PresentationTime::from_nanos(100_000_000).unwrap(),
                    )
                    .unwrap(),
                    VideoTimingBasis::RecordedDelta,
                )
                .unwrap(),
                VideoPresentationSegment::new(
                    1,
                    second_source.clone(),
                    PresentationRange::new(
                        PresentationTime::from_nanos(100_000_000).unwrap(),
                        PresentationTime::from_nanos(350_000_000).unwrap(),
                    )
                    .unwrap(),
                    VideoTimingBasis::TerminalHold,
                )
                .unwrap(),
            ],
            geometry,
        )
        .unwrap();
        VideoEncodeRequest::new(
            plan,
            vec![
                VideoEncodeFrame::new(0, first_source, ImageFormat::Png, dimensions, [1_u8])
                    .unwrap(),
                VideoEncodeFrame::new(1, second_source, ImageFormat::Png, dimensions, [2_u8])
                    .unwrap(),
            ],
            VideoEncodingProfile::new(geometry, 1_000_000).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn cumulative_endpoint_rounding_does_not_accumulate_per_segment_error() {
        let endpoints = [1_499_u64, 2_501, 4_499];
        let rounded: Vec<_> = endpoints
            .into_iter()
            .map(|value| round_nanos_to_micros(value).unwrap())
            .collect();
        assert_eq!(rounded, [1, 3, 4]);
        assert_eq!(
            [rounded[0], rounded[1] - rounded[0], rounded[2] - rounded[1]],
            [1, 2, 1]
        );
    }

    #[test]
    fn duration_format_is_fixed_ascii_microseconds() {
        assert_eq!(format_duration(1), "0.000001");
        assert_eq!(format_duration(1_250_001), "1.250001");
        assert!(format_duration(12).is_ascii());
    }

    #[test]
    fn generated_names_cannot_carry_path_syntax() {
        for index in [0, 1, 511] {
            for extension in ["png", "jpg"] {
                let name = format!("frame-{index:06}.{extension}");
                assert!(!name.contains('/') && !name.contains('\\'));
            }
        }
    }

    #[test]
    fn expected_mp4_uses_the_core_canvas_contract() {
        let dimensions = krometrail_core::PixelDimensions::new(2, 2).unwrap();
        let expected = ExpectedMp4 {
            canvas: dimensions,
            presentation_duration_micros: 250_000,
            max_bytes: 1_024,
        };
        assert_eq!(expected.canvas, dimensions);
    }

    #[tokio::test]
    async fn staging_writes_only_generated_names_and_exact_cumulative_durations() {
        let job = PreparedEncodeJob::from_request(&test_request())
            .await
            .unwrap();
        let document = tokio::fs::read_to_string(job.workspace().join(CONCAT_FILE_NAME))
            .await
            .unwrap();
        assert_eq!(
            document,
            concat!(
                "ffconcat version 1.0\n",
                "file frame-000000.png\n",
                "duration 0.100000\n",
                "file frame-000001.png\n",
                "duration 0.250000\n",
                "file frame-000001.png\n"
            )
        );
        assert_eq!(job.expected().presentation_duration_micros, 350_000);
        assert_eq!(job.output_limit(), 1_000_000);
        assert_eq!(job.output_path(), job.workspace().join(OUTPUT_FILE_NAME));
        let workspace = job.workspace().to_owned();
        drop(job);
        assert!(!workspace.exists());
    }
}
