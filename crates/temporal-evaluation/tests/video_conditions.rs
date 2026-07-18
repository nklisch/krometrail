use temporal_evaluation::{
    Architecture, ConditionId, Platform, VideoConditionEvidence, VideoConditionId,
    VideoEncoderEvidence, VideoHostModelIdentity, VideoPresentationPolicy, VideoResourceEvidence,
    optional_video_conditions,
};

fn hash(byte: char) -> String {
    format!("sha256:{}", byte.to_string().repeat(64))
}

fn evidence(
    condition_id: VideoConditionId,
    presentation_policy: VideoPresentationPolicy,
) -> VideoConditionEvidence {
    let artifact_id = "00000000-0000-0000-0000-000000000050";
    VideoConditionEvidence {
        condition_id,
        required: false,
        host_model: VideoHostModelIdentity {
            host: "reference-host-1".into(),
            platform: Platform::Linux,
            architecture: Architecture::X86_64,
            provider: "fixture-provider".into(),
            model_id: "fixture-model".into(),
            model_version_or_dated_alias: "2026-07-18".into(),
            video_input_declared: true,
        },
        encoder: VideoEncoderEvidence {
            implementation_version: "ffmpeg-7.1".into(),
            build_sha256: hash('a'),
            encoder_name: "libx264".into(),
            adapter_version: "1.0.7".into(),
            argument_policy_version: "fixed-mp4-h264-v1".into(),
        },
        presentation_policy,
        resource: VideoResourceEvidence {
            source_interval_sha256: hash('b'),
            gap_ids: vec!["gap-1".into()],
            artifact_id: artifact_id.into(),
            output_sha256: hash('c'),
            video_uri: format!(
                "krometrail://evidence/00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002/videos/{artifact_id}"
            ),
            manifest_uri: format!(
                "krometrail://evidence/00000000-0000-0000-0000-000000000001/00000000-0000-0000-0000-000000000002/video-manifests/{artifact_id}"
            ),
            manifest_sha256: hash('d'),
        },
    }
}

#[test]
fn a_through_e_remain_required_while_f_and_g_are_separate_optional_conditions() {
    assert_eq!(ConditionId::ALL.len(), 5);
    assert_eq!(optional_video_conditions().len(), 2);
    assert!(
        optional_video_conditions()
            .iter()
            .all(|condition| !condition.required)
    );
    assert_eq!(
        optional_video_conditions().map(|condition| condition.condition_id),
        VideoConditionId::ALL
    );
}

#[test]
fn optional_video_evidence_binds_exact_host_model_encoder_policy_and_local_resources() {
    let real_time = evidence(
        VideoConditionId::FRealTimeVideo,
        VideoPresentationPolicy::RealTime,
    );
    let optimized = evidence(
        VideoConditionId::GModelOptimizedVideo,
        VideoPresentationPolicy::ModelOptimized,
    );
    real_time.validate().unwrap();
    optimized.validate().unwrap();
    assert_ne!(real_time.presentation_policy, optimized.presentation_policy);

    let mut unsupported = real_time.clone();
    unsupported.host_model.video_input_declared = false;
    assert!(unsupported.validate().is_err());
    let mut wrong_policy = real_time.clone();
    wrong_policy.presentation_policy = VideoPresentationPolicy::ModelOptimized;
    assert!(wrong_policy.validate().is_err());
    let mut mismatched_manifest = real_time;
    mismatched_manifest.resource.manifest_uri = mismatched_manifest
        .resource
        .manifest_uri
        .replace("000000000050", "000000000051");
    assert!(mismatched_manifest.validate().is_err());
}
