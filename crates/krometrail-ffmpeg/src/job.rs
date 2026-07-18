use std::{
    ffi::OsString,
    fs::OpenOptions,
    io::{Read, Write},
    path::Path,
    sync::Arc,
    time::Duration,
};

use krometrail_core::{ImageFormat, VideoEncodeRequest};

use crate::{
    control::OperationControl,
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
    pub(crate) async fn from_request(
        request: &VideoEncodeRequest,
        control: &OperationControl,
    ) -> Result<Self, AdapterFailure> {
        let request = request.clone();
        control
            .run_blocking(AdapterFailureStage::InputStaging, move |control| {
                Self::from_request_blocking(&request, &control)
            })
            .await
    }

    fn from_request_blocking(
        request: &VideoEncodeRequest,
        control: &OperationControl,
    ) -> Result<Self, AdapterFailure> {
        control.check(AdapterFailureStage::InputStaging)?;
        let workspace = tempfile::Builder::new()
            .prefix("krometrail-video-")
            .tempdir()
            .map_err(|_| staging_failure())?;
        restrict_workspace(workspace.path()).map_err(|_| staging_failure())?;

        let mut names = Vec::with_capacity(request.frames().len());
        for (index, frame) in request.frames().iter().enumerate() {
            control.check(AdapterFailureStage::InputStaging)?;
            let extension = match frame.format() {
                ImageFormat::Jpeg => "jpg",
                ImageFormat::Png => "png",
            };
            let name = format!("frame-{index:06}.{extension}");
            let path = workspace.path().join(&name);
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .map_err(|_| staging_failure())?;
            for chunk in frame.bytes().chunks(64 * 1024) {
                control.check(AdapterFailureStage::InputStaging)?;
                file.write_all(chunk).map_err(|_| staging_failure())?;
            }
            file.flush().map_err(|_| staging_failure())?;
            names.push(name);
        }

        let timeline = quantize_timeline(request)?;
        let concat = concat_document(&names, &timeline)?;
        let mut concat_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(workspace.path().join(CONCAT_FILE_NAME))
            .map_err(|_| staging_failure())?;
        concat_file
            .write_all(concat.as_bytes())
            .map_err(|_| staging_failure())?;
        concat_file.flush().map_err(|_| staging_failure())?;

        let output_limit = request.profile().max_encoded_bytes();
        let geometry = request.profile().geometry();
        Ok(Self {
            arguments: encode_arguments(
                geometry,
                &timeline.presentation_pts_micros,
                timeline.duration_micros,
                output_limit,
            ),
            expected: ExpectedMp4 {
                canvas: geometry.canvas(),
                presentation_duration_micros: timeline.duration_micros,
                sample_durations_micros: timeline.sample_durations_micros,
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

    pub(crate) fn expected(&self) -> ExpectedMp4 {
        self.expected.clone()
    }

    pub(crate) const fn output_limit(&self) -> u64 {
        self.output_limit
    }

    pub(crate) fn output_path(&self) -> std::path::PathBuf {
        self.workspace.path().join(OUTPUT_FILE_NAME)
    }

    pub(crate) fn read_output_blocking(
        &self,
        control: &OperationControl,
    ) -> Result<Arc<[u8]>, AdapterFailure> {
        control.check(AdapterFailureStage::OutputValidation)?;
        let path = self.output_path();
        let metadata = std::fs::metadata(&path)
            .map_err(|_| output_failure(AdapterFailureKind::InvalidOutput))?;
        if metadata.len() == 0 || metadata.len() > self.output_limit {
            return Err(output_failure(if metadata.len() > self.output_limit {
                AdapterFailureKind::OutputOverflow
            } else {
                AdapterFailureKind::InvalidOutput
            })
            .with_observed_bytes(metadata.len()));
        }
        let mut file = std::fs::File::open(path)
            .map_err(|_| output_failure(AdapterFailureKind::InvalidOutput))?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        let mut chunk = [0_u8; 64 * 1024];
        loop {
            control.check(AdapterFailureStage::OutputValidation)?;
            let read = file
                .read(&mut chunk)
                .map_err(|_| output_failure(AdapterFailureKind::InvalidOutput))?;
            if read == 0 {
                break;
            }
            if (bytes.len() as u64).saturating_add(read as u64) > self.output_limit {
                return Err(output_failure(AdapterFailureKind::OutputOverflow)
                    .with_observed_bytes((bytes.len() + read) as u64));
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        if bytes.len() as u64 != metadata.len() || bytes.len() as u64 > self.output_limit {
            return Err(output_failure(AdapterFailureKind::OutputOverflow)
                .with_observed_bytes(bytes.len() as u64));
        }
        Ok(bytes.into())
    }
}

#[derive(Debug, Eq, PartialEq)]
struct QuantizedTimeline {
    segment_durations_micros: Vec<u64>,
    presentation_pts_micros: Vec<u64>,
    sample_durations_micros: Arc<[u64]>,
    duration_micros: u64,
}

fn quantize_timeline(request: &VideoEncodeRequest) -> Result<QuantizedTimeline, AdapterFailure> {
    quantize_boundaries(request.plan().segments().iter().map(|segment| {
        (
            segment.presentation().start().as_nanos(),
            segment.presentation().end().as_nanos(),
        )
    }))
}

fn quantize_boundaries(
    boundaries: impl IntoIterator<Item = (u64, u64)>,
) -> Result<QuantizedTimeline, AdapterFailure> {
    let boundaries: Vec<_> = boundaries.into_iter().collect();
    if boundaries.is_empty() {
        return Err(unrepresentable_timing());
    }
    let mut segment_durations_micros = Vec::with_capacity(boundaries.len());
    let mut presentation_pts_micros = Vec::with_capacity(boundaries.len() + 1);
    let mut previous_endpoint = 0_u64;
    for (start_nanos, end_nanos) in boundaries {
        let start = round_nanos_to_micros(start_nanos)?;
        let endpoint = round_nanos_to_micros(end_nanos)?;
        if start != previous_endpoint || endpoint <= start {
            return Err(unrepresentable_timing());
        }
        presentation_pts_micros.push(start);
        segment_durations_micros.push(endpoint - start);
        previous_endpoint = endpoint;
    }
    let terminal = previous_endpoint
        .checked_sub(1)
        .filter(|terminal| *terminal > *presentation_pts_micros.last().unwrap())
        .ok_or_else(unrepresentable_timing)?;
    presentation_pts_micros.push(terminal);
    let mut sample_durations_micros = Vec::with_capacity(presentation_pts_micros.len());
    for pair in presentation_pts_micros.windows(2) {
        sample_durations_micros.push(pair[1] - pair[0]);
    }
    sample_durations_micros.push(1);
    Ok(QuantizedTimeline {
        segment_durations_micros,
        presentation_pts_micros,
        sample_durations_micros: sample_durations_micros.into(),
        duration_micros: previous_endpoint,
    })
}

fn concat_document(
    names: &[String],
    timeline: &QuantizedTimeline,
) -> Result<String, AdapterFailure> {
    if names.len() != timeline.segment_durations_micros.len() || names.is_empty() {
        return Err(staging_failure());
    }
    let mut document = String::from("ffconcat version 1.0\n");
    for (duration, name) in timeline.segment_durations_micros.iter().zip(names) {
        document.push_str("file ");
        document.push_str(name);
        document.push('\n');
        document.push_str("duration ");
        document.push_str(&format_duration(*duration));
        document.push('\n');
    }
    document.push_str("file ");
    document.push_str(names.last().expect("validated names are non-empty"));
    document.push('\n');
    Ok(document)
}

fn round_nanos_to_micros(nanos: u64) -> Result<u64, AdapterFailure> {
    nanos
        .checked_add(500)
        .map(|rounded| rounded / 1_000)
        .ok_or_else(unrepresentable_timing)
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

fn unrepresentable_timing() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::InputStaging,
        AdapterFailureKind::UnrepresentableTiming,
    )
}

fn output_failure(kind: AdapterFailureKind) -> AdapterFailure {
    AdapterFailure::new(AdapterFailureStage::OutputValidation, kind)
}

#[cfg(unix)]
fn restrict_workspace(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn restrict_workspace(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "private FFmpeg staging is unsupported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{
        CancellationSignal, DeviceScaleFactor, FrameId, PortFuture, PresentationRange,
        PresentationTime, SessionRange, SessionTime, VideoEncodeFrame, VideoEncodingProfile,
        VideoOutputGeometry, VideoPresentationPlan, VideoPresentationPolicy,
        VideoPresentationSegment, VideoSegmentSource, VideoTimingBasis, VisualEpoch,
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

    fn control() -> OperationControl {
        OperationControl::new(
            Arc::new(NeverCancelled),
            std::time::Instant::now() + Duration::from_secs(5),
        )
    }

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
    fn quantized_timeline_is_total_only_for_strictly_increasing_pts() {
        let timeline = quantize_boundaries([(0, 1_499), (1_499, 3_500)]).unwrap();
        assert_eq!(timeline.segment_durations_micros, [1, 3]);
        assert_eq!(timeline.presentation_pts_micros, [0, 1, 3]);
        assert_eq!(&*timeline.sample_durations_micros, [1, 2, 1]);
        assert_eq!(timeline.duration_micros, 4);

        for boundaries in [
            vec![(0, 499)],
            vec![(0, 600), (600, 1_000)],
            vec![(0, 1_000)],
            vec![(0, u64::MAX)],
        ] {
            assert_eq!(
                quantize_boundaries(boundaries).unwrap_err().kind,
                AdapterFailureKind::UnrepresentableTiming
            );
        }
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
            sample_durations_micros: Arc::from([250_000_u64]),
            max_bytes: 1_024,
        };
        assert_eq!(expected.canvas, dimensions);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn staging_writes_only_generated_names_and_exact_cumulative_durations() {
        let job = PreparedEncodeJob::from_request(&test_request(), &control())
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
        assert_eq!(
            &*job.expected().sample_durations_micros,
            [100_000, 249_999, 1]
        );
        assert_eq!(job.output_limit(), 1_000_000);
        assert_eq!(job.output_path(), job.workspace().join(OUTPUT_FILE_NAME));
        let workspace = job.workspace().to_owned();
        drop(job);
        assert!(!workspace.exists());
    }
}
