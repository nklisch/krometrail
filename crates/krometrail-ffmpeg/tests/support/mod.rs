use std::{
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

struct CompiledFixture {
    _directory: tempfile::TempDir,
    executable: PathBuf,
}

static COMPILED_FIXTURE: OnceLock<CompiledFixture> = OnceLock::new();

pub struct FixtureExecutable {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl FixtureExecutable {
    pub fn new(mode: &str) -> Self {
        let compiled = COMPILED_FIXTURE.get_or_init(compile_fixture);
        let directory = tempfile::tempdir().expect("create fake FFmpeg directory");
        let executable = directory.path().join(executable_name());
        std::fs::copy(&compiled.executable, &executable).expect("copy compiled FFmpeg fixture");
        make_executable(&executable);
        std::fs::write(directory.path().join("fixture-mode"), mode)
            .expect("write fake FFmpeg mode");
        Self {
            _directory: directory,
            path: executable,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn clear_observation(&self) {
        for name in ["active-pid", "working-directory"] {
            let _ = std::fs::remove_file(self.path.parent().unwrap().join(name));
        }
    }

    pub fn active_pid(&self) -> Option<u32> {
        std::fs::read_to_string(self.path.parent().unwrap().join("active-pid"))
            .ok()
            .and_then(|value| value.parse().ok())
    }

    pub fn working_directory(&self) -> Option<PathBuf> {
        std::fs::read_to_string(self.path.parent().unwrap().join("working-directory"))
            .ok()
            .map(PathBuf::from)
    }

    pub fn mutate_executable(&self) {
        OpenOptions::new()
            .append(true)
            .open(&self.path)
            .expect("open fake FFmpeg for mutation")
            .write_all(&[0])
            .expect("mutate fake FFmpeg identity");
    }
}

pub fn video_request() -> krometrail_core::VideoEncodeRequest {
    use krometrail_core::{
        DeviceScaleFactor, FrameId, ImageFormat, PresentationRange, PresentationTime, SessionRange,
        SessionTime, VideoEncodeFrame, VideoEncodingProfile, VideoOutputGeometry,
        VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment,
        VideoSegmentSource, VideoTimingBasis, VisualEpoch,
    };

    let first: FrameId = "00000000-0000-0000-0000-000000000001".parse().unwrap();
    let second: FrameId = "00000000-0000-0000-0000-000000000002".parse().unwrap();
    let dimensions = krometrail_core::PixelDimensions::new(2, 2).unwrap();
    let geometry = VideoOutputGeometry::new(dimensions, dimensions, dimensions).unwrap();
    let second_time = SessionTime::from_nanos(100_000_000);
    let range = SessionRange::new(SessionTime::ZERO, second_time).unwrap();
    let first_source = VideoSegmentSource::source_frame(first, SessionTime::ZERO).unwrap();
    let second_source = VideoSegmentSource::source_frame(second, second_time).unwrap();
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
            device_scale_factor: DeviceScaleFactor::new(1.0).unwrap(),
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
    krometrail_core::VideoEncodeRequest::new(
        plan,
        vec![
            VideoEncodeFrame::new(
                0,
                first_source,
                ImageFormat::Png,
                dimensions,
                test_png([0, 0, 0, 255]),
            )
            .unwrap(),
            VideoEncodeFrame::new(
                1,
                second_source,
                ImageFormat::Png,
                dimensions,
                test_png([255, 255, 255, 255]),
            )
            .unwrap(),
        ],
        VideoEncodingProfile::new(geometry, 1_000_000).unwrap(),
    )
    .unwrap()
}

fn test_png(color: [u8; 4]) -> Vec<u8> {
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let mut pixels = [0_u8; 16];
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
        writer.write_image_data(&pixels).unwrap();
    }
    output
}

fn compile_fixture() -> CompiledFixture {
    let directory = tempfile::tempdir().expect("create fixture compiler directory");
    let executable = directory.path().join(executable_name());
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg("--edition=2024")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fixture_main.rs"))
        .arg("-o")
        .arg(&executable)
        .env(
            "KROMETRAIL_FFMPEG_FIXTURE_MANIFEST_DIR",
            env!("CARGO_MANIFEST_DIR"),
        )
        .status()
        .expect("run rustc for compiled FFmpeg fixture");
    assert!(status.success(), "compiled FFmpeg fixture must build");
    CompiledFixture {
        _directory: directory,
        executable,
    }
}

#[cfg(windows)]
fn executable_name() -> &'static str {
    "ffmpeg.exe"
}

#[cfg(not(windows))]
fn executable_name() -> &'static str {
    "ffmpeg"
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .expect("mark FFmpeg fixture executable");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
