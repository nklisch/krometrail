use temporal_evaluation::{
    ArtifactCacheIdentity, ArtifactEvidenceReference, ArtifactKind, ConditionEvidence, ConditionId,
    ConditionPackager, EvidenceAvailability, EvidenceReference, EvidenceReferenceKind, GapEvidence,
    NamedVersion, NonClaimId, ProgressiveConditionEvidence, ProgressiveRetrievalRecord,
    RetentionState, ScopeIdentity, SourceFrameEvidence, SourceInterval, TemporalBundleEvidence,
    TimeRangeNs, UNIFORM_SOURCE_FRAME_SLOTS, require_one_source_interval,
};

fn hash(value: u8) -> String {
    format!("sha256:{:0>64}", value)
}

fn interval() -> SourceInterval {
    let frames = (0..12)
        .map(|index| SourceFrameEvidence {
            id: format!("frame-{index}"),
            capture_ordinal: index + 1,
            source_time_ns: Some(index * 1_000),
            observed_time_ns: index * 1_000 + 10_000,
            session_time_ns: index * 1_000,
            encoded_sha256: hash(index as u8 + 1),
            availability: EvidenceAvailability::Retained,
        })
        .collect();
    SourceInterval::new(
        "interval-1",
        ScopeIdentity::new("session-1", "target-1").unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        5_000,
        frames,
        Vec::new(),
        RetentionState::Retained,
    )
    .unwrap()
}

fn reference(
    id: &str,
    kind: EvidenceReferenceKind,
    availability: EvidenceAvailability,
) -> EvidenceReference {
    EvidenceReference::new(
        id,
        kind,
        (availability == EvidenceAvailability::Retained).then(|| hash(200)),
        availability,
    )
    .unwrap()
}

fn artifact(
    interval: &SourceInterval,
    id: &str,
    kind: ArtifactKind,
    algorithm: &str,
    version: &str,
    selected: Vec<String>,
) -> ArtifactEvidenceReference {
    ArtifactEvidenceReference {
        output: reference(
            id,
            EvidenceReferenceKind::Artifact(kind),
            EvidenceAvailability::Retained,
        ),
        resolved_range: interval.resolved_range,
        manifest_sha256: hash(201),
        source_frame_ids: interval.frame_ids(),
        selected_frame_ids: selected,
        gap_ids: interval.gap_ids(),
        algorithm_versions: vec![NamedVersion {
            name: algorithm.into(),
            version: version.into(),
        }],
        cache: ArtifactCacheIdentity {
            cache_schema_version: 1,
            cache_key: hash(202),
            source_fingerprint: hash(203),
            parameter_hash: hash(204),
            visual_epoch_hash: hash(205),
            adapter_version: NamedVersion {
                name: "adapter-v1".into(),
                version: "1".into(),
            },
            generator: NamedVersion {
                name: algorithm.into(),
                version: version.into(),
            },
        },
    }
}

fn bundle(interval: &SourceInterval) -> TemporalBundleEvidence {
    let selected: Vec<String> = (0..8).map(|index| format!("frame-{index}")).collect();
    TemporalBundleEvidence {
        bundle: reference(
            "bundle-1",
            EvidenceReferenceKind::Artifact(ArtifactKind::TemporalDebugBundle),
            EvidenceAvailability::Retained,
        ),
        before_during_after: vec![artifact(
            interval,
            "bda-1",
            ArtifactKind::BeforeDuringAfter,
            "temporal-storyboard",
            "1.1.0",
            selected.clone(),
        )],
        storyboards: vec![artifact(
            interval,
            "storyboard-1",
            ArtifactKind::ChangeAwareStoryboard,
            "temporal-storyboard",
            "1.1.0",
            selected.clone(),
        )],
        difference_maps: vec![artifact(
            interval,
            "diff-1",
            ArtifactKind::DifferenceMap,
            "temporal-difference-map",
            "v1",
            selected,
        )],
        capture_summary: reference(
            "capture-1",
            EvidenceReferenceKind::CaptureSummary,
            EvidenceAvailability::Retained,
        ),
        context_summary: reference(
            "context-1",
            EvidenceReferenceKind::ContextSummary,
            EvidenceAvailability::Retained,
        ),
        evidence_references: vec![],
    }
}

#[test]
fn all_five_conditions_keep_one_interval_and_fixed_non_claims() {
    let interval = interval();
    let a = ConditionPackager::final_screenshot(
        &interval,
        "frame-11",
        reference(
            "observation-1",
            EvidenceReferenceKind::CurrentObservation,
            EvidenceAvailability::Retained,
        ),
    )
    .unwrap();
    let b = ConditionPackager::uniform_storyboard(&interval).unwrap();
    let c = ConditionPackager::change_aware_storyboard(
        &interval,
        vec![artifact(
            &interval,
            "storyboard-2",
            ArtifactKind::ChangeAwareStoryboard,
            "temporal-storyboard",
            "1.1.0",
            (0..8).map(|index| format!("frame-{index}")).collect(),
        )],
    )
    .unwrap();
    let d = ConditionPackager::temporal_bundle(&interval, bundle(&interval)).unwrap();
    let e = ConditionPackager::progressive_source(
        &interval,
        ProgressiveConditionEvidence {
            bundle: bundle(&interval),
            source_retrievals: vec![ProgressiveRetrievalRecord {
                request_id: "request-1".into(),
                requested_frame_ids: vec!["frame-1".into(), "frame-3".into()],
                returned_frames: vec![
                    EvidenceReference::new(
                        "frame-1",
                        EvidenceReferenceKind::SourceFrame,
                        Some(interval.frame("frame-1").unwrap().encoded_sha256.clone()),
                        EvidenceAvailability::Retained,
                    )
                    .unwrap(),
                    EvidenceReference::new(
                        "frame-3",
                        EvidenceReferenceKind::SourceFrame,
                        Some(interval.frame("frame-3").unwrap().encoded_sha256.clone()),
                        EvidenceAvailability::Retained,
                    )
                    .unwrap(),
                ],
                unavailable_frame_ids: vec![],
            }],
            region_filmstrip: Some(artifact(
                &interval,
                "filmstrip-1",
                ArtifactKind::RegionFilmstrip,
                "region-filmstrip",
                "1.0.0",
                (0..8).map(|index| format!("frame-{index}")).collect(),
            )),
        },
    )
    .unwrap();

    let packages = vec![a, b, c, d, e];
    assert_eq!(packages.len(), ConditionId::ALL.len());
    assert_eq!(
        require_one_source_interval(&packages).unwrap(),
        interval.digest().unwrap()
    );
    for package in &packages {
        assert_eq!(package.non_claims, NonClaimId::ALL);
        assert_eq!(package.source_frame_ids, interval.frame_ids());
        assert_eq!(package.gap_ids, interval.gap_ids());
        assert_eq!(package.retention, RetentionState::Retained);
        let bytes = package.canonical_bytes().unwrap();
        assert_eq!(
            serde_json::from_slice::<temporal_evaluation::ConditionPackage>(&bytes).unwrap(),
            *package
        );
        let text = String::from_utf8(bytes).unwrap();
        for forbidden in [
            "data:image",
            "base64",
            "ground truth",
            "raw answer",
            "/tmp/",
        ] {
            assert!(!text.to_ascii_lowercase().contains(forbidden));
        }
    }
    assert!(matches!(
        packages[1].evidence,
        ConditionEvidence::UniformStoryboard { .. }
    ));
    assert_eq!(UNIFORM_SOURCE_FRAME_SLOTS, 8);
}

#[test]
fn b_is_integer_uniform_and_rejects_insufficient_retention() {
    let interval = interval();
    let package = ConditionPackager::uniform_storyboard(&interval).unwrap();
    let slots = match package.evidence {
        ConditionEvidence::UniformStoryboard { slot_frame_ids } => slot_frame_ids,
        _ => panic!("wrong evidence variant"),
    };
    assert_eq!(
        slots,
        vec![
            "frame-0", "frame-1", "frame-3", "frame-4", "frame-6", "frame-7", "frame-9", "frame-11"
        ]
    );
    assert!(slots.windows(2).all(|pair| pair[0] != pair[1]));

    let mut unavailable = interval;
    for frame in unavailable.frames.iter_mut().take(5) {
        frame.availability = EvidenceAvailability::Evicted;
    }
    unavailable.retention = RetentionState::PartiallyRetained;
    // The interval digest is intentionally recomputed through the constructor instead of
    // repairing a mutable identity in-place.
    let unavailable = SourceInterval::new(
        unavailable.interval_id,
        unavailable.session_scope,
        unavailable.requested_range,
        unavailable.resolved_range,
        unavailable.anchor_session_time_ns,
        unavailable.frames,
        unavailable.gaps,
        unavailable.retention,
    )
    .unwrap();
    assert!(ConditionPackager::uniform_storyboard(&unavailable).is_err());
}

#[test]
fn gap_unavailability_is_explicit_and_wrong_authority_is_rejected() {
    let mut source = interval();
    let frames = source
        .frames
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, mut frame)| {
            if index == 4 {
                frame.availability = EvidenceAvailability::Gap;
            }
            frame
        })
        .collect();
    source = SourceInterval::new(
        source.interval_id,
        source.session_scope,
        source.requested_range,
        source.resolved_range,
        source.anchor_session_time_ns,
        frames,
        vec![GapEvidence::new("gap-1", 4_000, 4_000, "queue saturated", None).unwrap()],
        RetentionState::PartiallyRetained,
    )
    .unwrap();
    assert!(source.has_unresolved_gap());
    let package = ConditionPackager::uniform_storyboard(&source).unwrap();
    assert_eq!(package.gap_ids, vec!["gap-1"]);

    let interval = interval();
    let mut wrong = artifact(
        &interval,
        "storyboard-1",
        ArtifactKind::ChangeAwareStoryboard,
        "temporal-storyboard",
        "1.0.0",
        (0..8).map(|index| format!("frame-{index}")).collect(),
    );
    assert!(ConditionPackager::change_aware_storyboard(&interval, vec![wrong.clone()]).is_err());
    wrong.cache.cache_schema_version = 0;
    assert!(ConditionPackager::change_aware_storyboard(&interval, vec![wrong]).is_err());
}
