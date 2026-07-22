#![cfg(feature = "qualification-support")]

use std::{
    io::{BufRead as _, BufReader, Read as _, Write as _},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
    time::Duration,
};

use image::ImageEncoder as _;
use krometrail_core::{
    CaptureOrdinal, CapturedFrame, DeviceScaleFactor, EncodedFrame, FrameId, ImageFormat,
    ObservedTime, OutputLimitsRequest, PixelDimensions, RangeResolutionOptions, RecordingSink,
    ResolvedRange, SessionId, SessionRange, SessionTime, TargetId, TemporalRangeAnchorKind,
    TemporalVideoGenerationRequest, VideoPresentationPolicy,
};
use krometrail_store::{
    IndexStoreConfig, RecordingStore, RotationConfig, SegmentStoreConfig, SegmentWriter,
    SqliteIndex,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn open_store(root: &std::path::Path) -> Arc<RecordingStore> {
    let segments = root.join("segments");
    let index = Arc::new(
        SqliteIndex::open(IndexStoreConfig {
            database_path: root.join("index.sqlite3"),
            segments_directory: segments.clone(),
            busy_timeout: Duration::from_secs(1),
        })
        .unwrap(),
    );
    let writer = Arc::new(
        SegmentWriter::open(SegmentStoreConfig {
            directory: segments,
            rotation: RotationConfig::suggested(),
        })
        .unwrap(),
    );
    Arc::new(RecordingStore::new(writer, index, store_test_clock()).unwrap())
}

fn png(color: [u8; 4]) -> Vec<u8> {
    let pixels = color.repeat(16);
    let mut encoded = Vec::new();
    image::codecs::png::PngEncoder::new(&mut encoded)
        .write_image(&pixels, 4, 4, image::ExtendedColorType::Rgba8)
        .unwrap();
    encoded
}

fn frame(
    session: SessionId,
    target: TargetId,
    id: u128,
    ordinal: u64,
    time: u64,
    color: [u8; 4],
) -> EncodedFrame {
    let dimensions = PixelDimensions::new(4, 4).unwrap();
    let metadata = CapturedFrame::new(
        FrameId::from_uuid(Uuid::from_u128(id)),
        session,
        target,
        CaptureOrdinal::new(ordinal).unwrap(),
        None,
        ObservedTime::from_nanos(time),
        SessionTime::from_nanos(time),
        ImageFormat::Png,
        dimensions,
        dimensions,
        DeviceScaleFactor::new(1.0).unwrap(),
        vec![],
    )
    .unwrap();
    EncodedFrame::new(metadata, png(color)).unwrap()
}

fn send(stdin: &mut ChildStdin, request: Value) {
    writeln!(stdin, "{request}").unwrap();
    stdin.flush().unwrap();
}

fn receive(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    stdout.read_line(&mut line).unwrap();
    assert!(!line.is_empty(), "MCP server closed before responding");
    serde_json::from_str(line.trim()).unwrap()
}

fn start_mcp(
    data: &std::path::Path,
    ffmpeg: PathBuf,
) -> (Child, ChildStdin, BufReader<ChildStdout>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_krometrail"))
        .arg("mcp")
        .env("KROMETRAIL_DATA_DIR", data)
        .env("KROMETRAIL_FFMPEG_PATH", ffmpeg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Krometrail MCP binary should start");
    let stdin = child.stdin.take().unwrap();
    let stdout = BufReader::new(child.stdout.take().unwrap());
    (child, stdin, stdout)
}

#[tokio::test]
#[ignore = "requires KROMETRAIL_FFMPEG_PATH to select a user-installed supported FFmpeg"]
async fn selected_real_ffmpeg_generates_both_policies_through_store_mcp_and_local_resources() {
    let ffmpeg = std::env::var_os("KROMETRAIL_FFMPEG_PATH")
        .map(PathBuf::from)
        .expect("set KROMETRAIL_FFMPEG_PATH to the exact user-installed FFmpeg executable");
    assert!(ffmpeg.is_file(), "selected FFmpeg path must be a file");

    let directory =
        std::env::temp_dir().join(format!("krometrail-video-mcp-live-{}", Uuid::new_v4()));
    let store = open_store(&directory);
    let session = SessionId::from_uuid(Uuid::from_u128(1));
    let target = TargetId::from_uuid(Uuid::from_u128(2));
    let frames = [
        frame(session, target, 3, 1, 0, [20, 30, 40, 255]),
        frame(session, target, 4, 2, 100_000_000, [220, 210, 200, 255]),
    ];
    for frame in &frames {
        store.append_frame(frame.clone()).await.unwrap();
    }
    store.flush(session).await.unwrap();
    drop(store);

    let interval =
        SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(350_000_000)).unwrap();
    let range = ResolvedRange::new(
        session,
        target,
        TemporalRangeAnchorKind::SessionTime,
        interval,
        interval,
        frames.iter().map(|frame| frame.metadata().id()).collect(),
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        RangeResolutionOptions::DEFAULT,
    )
    .unwrap();

    let (mut child, mut stdin, mut stdout) = start_mcp(&directory, ffmpeg);
    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-06-18","capabilities":{},
                "clientInfo":{"name":"video-mcp-live","version":"1"}
            }
        }),
    );
    assert_eq!(receive(&mut stdout)["id"], 1);
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut stdin,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
    );
    let listed = receive(&mut stdout);
    assert!(
        listed["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["name"] == "generate_temporal_video")
    );
    send(
        &mut stdin,
        json!({
            "jsonrpc":"2.0","id":3,"method":"resources/templates/list","params":{}
        }),
    );
    let templates = receive(&mut stdout);
    let names = templates["result"]["resourceTemplates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|template| template["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert!(names.contains(&"temporal-video"));
    assert!(names.contains(&"temporal-video-manifest"));

    for (offset, policy) in [
        VideoPresentationPolicy::RealTime,
        VideoPresentationPolicy::ModelOptimized,
    ]
    .into_iter()
    .enumerate()
    {
        let request = TemporalVideoGenerationRequest::new(
            range.clone(),
            policy,
            OutputLimitsRequest::new(4, 4, 4 * 1024 * 1024).unwrap(),
        )
        .unwrap();
        send(
            &mut stdin,
            json!({
                "jsonrpc":"2.0","id":10 + offset,"method":"tools/call","params":{
                    "name":"generate_temporal_video",
                    "arguments":serde_json::to_value(request).unwrap()
                }
            }),
        );
        let generated = receive(&mut stdout);
        assert_eq!(generated["result"]["isError"], false, "{generated}");
        let clip = &generated["result"]["structuredContent"]["result"]["clips"][0];
        assert_eq!(clip["presentation_policy"], policy.as_str());
        let video_uri = clip["video_uri"].as_str().unwrap();
        let manifest_uri = clip["manifest_uri"].as_str().unwrap();

        send(
            &mut stdin,
            json!({
                "jsonrpc":"2.0","id":20 + offset,"method":"resources/read",
                "params":{"uri":video_uri}
            }),
        );
        let video = receive(&mut stdout);
        assert_eq!(video["result"]["contents"][0]["mimeType"], "video/mp4");
        assert!(
            video["result"]["contents"][0]["blob"]
                .as_str()
                .is_some_and(|blob| !blob.is_empty())
        );

        send(
            &mut stdin,
            json!({
                "jsonrpc":"2.0","id":30 + offset,"method":"resources/read",
                "params":{"uri":manifest_uri}
            }),
        );
        let manifest_resource = receive(&mut stdout);
        assert_eq!(
            manifest_resource["result"]["contents"][0]["mimeType"],
            "application/json"
        );
        let manifest: Value = serde_json::from_str(
            manifest_resource["result"]["contents"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["plan"]["policy"], policy.as_str());
        assert_eq!(manifest["output_hash"], clip["output_hash"]);
        assert_eq!(manifest["artifact_id"], clip["artifact_id"]);
    }

    drop(stdin);
    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(status.success(), "stderr: {stderr}");
    assert!(stderr.is_empty(), "stderr: {stderr}");
    std::fs::remove_dir_all(directory).unwrap();
}

fn store_test_clock() -> std::sync::Arc<dyn krometrail_core::MonotonicClock> {
    struct Fixed;
    impl krometrail_core::MonotonicClock for Fixed {
        fn now(&self) -> krometrail_core::ObservedTime {
            krometrail_core::ObservedTime::from_nanos(0)
        }
    }
    std::sync::Arc::new(Fixed)
}
