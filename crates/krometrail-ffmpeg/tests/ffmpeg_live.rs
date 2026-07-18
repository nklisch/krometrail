#![cfg(feature = "qualification-support")]

#[allow(dead_code)]
mod support;

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use krometrail_core::{CancellationSignal, PortFuture, TemporalVideoEncoder, VideoEncodingContext};
use krometrail_ffmpeg::{FfmpegDiscoveryOptions, FfmpegQualification, qualify_ffmpeg};

struct NeverCancelled;

impl CancellationSignal for NeverCancelled {
    fn is_cancelled(&self) -> bool {
        false
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test]
#[ignore = "requires an explicit user-installed FFmpeg with the supported libx264 policy"]
async fn selected_real_ffmpeg_produces_the_playable_fixed_policy_clip() {
    let selected = std::env::var_os("KROMETRAIL_FFMPEG_PATH")
        .map(PathBuf::from)
        .expect("set KROMETRAIL_FFMPEG_PATH to the exact user-installed FFmpeg executable");
    let deadline = Instant::now() + Duration::from_secs(10);
    let qualification = qualify_ffmpeg(
        FfmpegDiscoveryOptions::with_explicit_executable(selected),
        Arc::new(NeverCancelled),
        deadline,
    )
    .await;
    let FfmpegQualification::Qualified(encoder) = qualification else {
        let FfmpegQualification::Unavailable(unavailable) = qualification else {
            unreachable!()
        };
        panic!(
            "the selected executable did not satisfy the fixed MP4/H.264 qualification policy: stage={:?} reason={:?}",
            unavailable.stage, unavailable.reason
        );
    };
    let clip = encoder
        .encode(
            support::video_request(),
            VideoEncodingContext {
                deadline: Instant::now() + Duration::from_secs(10),
                cancellation: Arc::new(NeverCancelled),
            },
        )
        .await
        .expect("qualified real FFmpeg must encode the same fixed-policy request");
    println!(
        "qualified {} encoder={} adapter={} policy={} output_sha256={} bytes={}",
        encoder.identity().implementation_version(),
        encoder.identity().encoder_name(),
        encoder.identity().adapter_version(),
        encoder.identity().argument_policy_version(),
        clip.output_hash(),
        clip.encoded_bytes().len(),
    );
    if let Some(output) = std::env::var_os("KROMETRAIL_FFMPEG_FIXTURE_OUTPUT") {
        std::fs::write(PathBuf::from(output), clip.encoded_bytes())
            .expect("explicit fixture output path must be writable");
    }
    assert!(
        clip.encoded_bytes()
            .windows(4)
            .any(|window| window == b"ftyp")
    );
}
