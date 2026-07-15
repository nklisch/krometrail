use temporal_evaluation::{
    AnswerRegion, AnswerTruth, ArtifactCacheIdentity, ArtifactEvidenceReference, ArtifactKind,
    CaseFamily, ConditionId, ConditionPackager, DimensionOutcome, DimensionScore,
    EvidenceAvailability, EvidenceReference, EvidenceReferenceKind, ExactRate, FailureRecord,
    GapEvidence, InterpretationAnswer, Judgment, MotionBehavior, NamedVersion,
    ProgressiveConditionEvidence, ProgressiveRetrievalRecord, RetentionState, RunFailureCode,
    ScopeIdentity, ScoringDimensionId, SourceFrameEvidence, SourceInterval, StateLabel,
    TemporalBundleEvidence, ThresholdProfile, TimeRangeNs, TrialScore, aggregate_condition,
    assess_thresholds,
};

fn hash(value: u8) -> String {
    format!("sha256:{value:0>64}")
}

fn interval() -> SourceInterval {
    SourceInterval::new(
        "threshold-interval",
        ScopeIdentity::new("threshold-session", "threshold-target").unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        TimeRangeNs::new(0, 11_000).unwrap(),
        5_000,
        (0..12)
            .map(|index| SourceFrameEvidence {
                id: format!("frame-{index}"),
                capture_ordinal: index + 1,
                source_time_ns: Some(index * 1_000),
                observed_time_ns: index * 1_000 + 10_000,
                session_time_ns: index * 1_000,
                encoded_sha256: hash(index as u8 + 1),
                availability: EvidenceAvailability::Retained,
            })
            .collect(),
        Vec::new(),
        RetentionState::Retained,
    )
    .unwrap()
}

fn reference(id: &str, kind: EvidenceReferenceKind) -> EvidenceReference {
    EvidenceReference::new(id, kind, Some(hash(200)), EvidenceAvailability::Retained).unwrap()
}

fn artifact(
    interval: &SourceInterval,
    id: &str,
    kind: ArtifactKind,
    algorithm: &str,
    version: &str,
) -> ArtifactEvidenceReference {
    ArtifactEvidenceReference {
        output: reference(id, EvidenceReferenceKind::Artifact(kind)),
        resolved_range: interval.resolved_range,
        manifest_sha256: hash(201),
        source_frame_ids: interval.frame_ids(),
        selected_frame_ids: (0..8).map(|index| format!("frame-{index}")).collect(),
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

fn bundle(interval: &SourceInterval, suffix: &str) -> TemporalBundleEvidence {
    TemporalBundleEvidence {
        bundle: reference(
            &format!("bundle-{suffix}"),
            EvidenceReferenceKind::Artifact(ArtifactKind::TemporalDebugBundle),
        ),
        before_during_after: vec![artifact(
            interval,
            &format!("bda-{suffix}"),
            ArtifactKind::BeforeDuringAfter,
            "temporal-storyboard",
            "1.1.0",
        )],
        storyboards: vec![artifact(
            interval,
            &format!("storyboard-{suffix}"),
            ArtifactKind::ChangeAwareStoryboard,
            "temporal-storyboard",
            "1.1.0",
        )],
        difference_maps: vec![artifact(
            interval,
            &format!("diff-{suffix}"),
            ArtifactKind::DifferenceMap,
            "temporal-difference-map",
            "v1",
        )],
        capture_summary: reference(
            &format!("capture-{suffix}"),
            EvidenceReferenceKind::CaptureSummary,
        ),
        context_summary: reference(
            &format!("context-{suffix}"),
            EvidenceReferenceKind::ContextSummary,
        ),
        evidence_references: vec![],
    }
}

fn packages() -> Vec<temporal_evaluation::ConditionPackage> {
    let interval = interval();
    let a = ConditionPackager::final_screenshot(
        &interval,
        "frame-11",
        reference("observation-a", EvidenceReferenceKind::CurrentObservation),
    )
    .unwrap();
    let b = ConditionPackager::uniform_storyboard(&interval).unwrap();
    let c = ConditionPackager::change_aware_storyboard(
        &interval,
        vec![artifact(
            &interval,
            "storyboard-c",
            ArtifactKind::ChangeAwareStoryboard,
            "temporal-storyboard",
            "1.1.0",
        )],
    )
    .unwrap();
    let d = ConditionPackager::temporal_bundle(&interval, bundle(&interval, "d")).unwrap();
    let e = ConditionPackager::progressive_source(
        &interval,
        ProgressiveConditionEvidence {
            bundle: bundle(&interval, "e"),
            source_retrievals: vec![ProgressiveRetrievalRecord {
                request_id: "request-e".into(),
                requested_frame_ids: vec!["frame-1".into()],
                returned_frames: vec![
                    EvidenceReference::new(
                        "frame-1",
                        EvidenceReferenceKind::SourceFrame,
                        Some(interval.frame("frame-1").unwrap().encoded_sha256.clone()),
                        EvidenceAvailability::Retained,
                    )
                    .unwrap(),
                ],
                unavailable_frame_ids: vec![],
            }],
            region_filmstrip: Some(artifact(
                &interval,
                "filmstrip-e",
                ArtifactKind::RegionFilmstrip,
                "region-filmstrip",
                "1.0.0",
            )),
        },
    )
    .unwrap();
    vec![a, b, c, d, e]
}

fn synthetic_score(
    condition: ConditionId,
    index: u16,
    family: CaseFamily,
    source_interval_digest: &str,
    tile_count: u16,
    defect_outcome: DimensionOutcome,
    stable_outcome: DimensionOutcome,
) -> TrialScore {
    let case_id = match family {
        CaseFamily::MovementReversal => "movement-reversal/basic",
        CaseFamily::Flicker => "flicker/visibility",
        CaseFamily::TransientLayout => "layout/width",
        CaseFamily::DomOpaqueMotion => "dom-opaque/path-reversal",
        CaseFamily::StableControl => "stable/smooth-panel",
    };
    let mut dimensions = vec![
        dimension(
            ScoringDimensionId::TransientDefectIdentification,
            if family == CaseFamily::StableControl {
                DimensionOutcome::NotApplicable
            } else {
                defect_outcome
            },
        ),
        dimension(ScoringDimensionId::StateOrder, DimensionOutcome::Correct),
        dimension(
            ScoringDimensionId::AffectedRegion,
            DimensionOutcome::Correct,
        ),
        dimension(
            ScoringDimensionId::MotionBehavior,
            DimensionOutcome::Correct,
        ),
        dimension(
            ScoringDimensionId::GapUncertainty,
            DimensionOutcome::NotApplicable,
        ),
        dimension(
            ScoringDimensionId::StableControlFalsePositive,
            if family == CaseFamily::StableControl {
                stable_outcome
            } else {
                DimensionOutcome::NotApplicable
            },
        ),
    ];
    let earned_points = dimensions
        .iter()
        .filter(|dimension| dimension.outcome == DimensionOutcome::Correct)
        .count() as u16;
    let possible_points = dimensions
        .iter()
        .filter(|dimension| {
            matches!(
                dimension.outcome,
                DimensionOutcome::Correct | DimensionOutcome::Incorrect
            )
        })
        .count() as u16;
    let failed = dimensions
        .iter()
        .any(|dimension| dimension.outcome == DimensionOutcome::Incorrect);
    let status = if failed {
        temporal_evaluation::EvaluationStatus::Fail
    } else {
        temporal_evaluation::EvaluationStatus::Pass
    };
    TrialScore {
        trial_id: format!("synthetic/{family:?}/{index}"),
        condition_id: condition,
        package_digest: hash(250),
        source_interval_digest: source_interval_digest.into(),
        source_frame_tile_count: tile_count,
        case_id: case_id.into(),
        answer: InterpretationAnswer {
            temporary_state: if family == CaseFamily::StableControl {
                AnswerTruth::No
            } else {
                AnswerTruth::Yes
            },
            state_order: if family == CaseFamily::StableControl {
                vec![StateLabel::IntentionalMotion, StateLabel::Final]
            } else {
                vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final]
            },
            affected_region: AnswerRegion::Unknown,
            motion_behavior: MotionBehavior::Uncertain,
            judgment: if family == CaseFamily::StableControl {
                Judgment::Intentional
            } else {
                Judgment::Defective
            },
            uncertainty_reasons: vec![],
            evidence_refs: vec![],
        },
        answer_digest: hash(index as u8 + 20),
        raw_answer_ref: format!("sidecar-{index}"),
        dimensions: std::mem::take(&mut dimensions),
        accepted_claims: vec![],
        earned_points,
        possible_points,
        status,
        failure: failed.then(|| FailureRecord {
            code: RunFailureCode::Threshold,
            phase: "synthetic".into(),
            reason: "synthetic complete answer is incorrect".into(),
            recovery: "replace the synthetic answer".into(),
            retryable: false,
        }),
    }
}

fn dimension(id: ScoringDimensionId, outcome: DimensionOutcome) -> DimensionScore {
    DimensionScore {
        dimension_id: id,
        outcome,
        observed_value: "synthetic".into(),
        expected_value: "synthetic".into(),
        evidence_ids: vec![],
        rationale_code: "synthetic".into(),
    }
}

fn scores(
    condition: ConditionId,
    interval_digest: &str,
    defect_outcome: DimensionOutcome,
    tile_count: u16,
) -> Vec<TrialScore> {
    [
        (CaseFamily::MovementReversal, 0),
        (CaseFamily::Flicker, 10),
        (CaseFamily::TransientLayout, 20),
        (CaseFamily::StableControl, 30),
    ]
    .into_iter()
    .flat_map(|(family, start)| {
        (0..10).map(move |offset| {
            synthetic_score(
                condition,
                start + offset,
                family,
                interval_digest,
                tile_count,
                defect_outcome,
                DimensionOutcome::Correct,
            )
        })
    })
    .collect()
}

fn aggregates() -> Vec<temporal_evaluation::ConditionAggregate> {
    let digest = interval().digest().unwrap();
    [
        (
            ConditionId::AFinalScreenshot,
            DimensionOutcome::Incorrect,
            1,
        ),
        (
            ConditionId::BUniformStoryboard,
            DimensionOutcome::Correct,
            8,
        ),
        (
            ConditionId::CChangeAwareStoryboard,
            DimensionOutcome::Correct,
            8,
        ),
        (ConditionId::DTemporalBundle, DimensionOutcome::Correct, 8),
        (
            ConditionId::EProgressiveSource,
            DimensionOutcome::Correct,
            8,
        ),
    ]
    .into_iter()
    .map(|(condition, outcome, tiles)| {
        aggregate_condition(
            condition,
            &scores(condition, &digest, outcome, tiles),
            &ThresholdProfile::canonical(),
        )
        .unwrap()
    })
    .collect()
}

#[test]
fn exact_rate_rejects_invalid_counts_and_respects_exact_percentage_points() {
    assert!(ExactRate::new(0, 0).is_err());
    assert!(ExactRate::new(11, 10).is_err());
    let rate = ExactRate::new(u32::MAX, u32::MAX).unwrap();
    assert_eq!(rate.percentage_points(), 100);
    assert!(
        ExactRate::new(3, 10)
            .unwrap()
            .at_least(ExactRate::new(1, 10).unwrap(), 20)
    );
    assert!(
        !ExactRate::new(29, 100)
            .unwrap()
            .at_least(ExactRate::new(0, 1).unwrap(), 30)
    );
}

#[test]
fn aggregation_is_fixed_order_and_preserves_family_and_dimension_rates() {
    let digest = interval().digest().unwrap();
    let scores = scores(
        ConditionId::BUniformStoryboard,
        &digest,
        DimensionOutcome::Correct,
        8,
    );
    let aggregate = aggregate_condition(
        ConditionId::BUniformStoryboard,
        &scores,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    assert_eq!(
        aggregate
            .dimensions
            .iter()
            .map(|dimension| dimension.dimension_id)
            .collect::<Vec<_>>(),
        ScoringDimensionId::ALL
    );
    assert_eq!(aggregate.trial_count, 40);
    assert_eq!(aggregate.decisive_trial_count, 40);
    assert_eq!(
        aggregate.source_frame_tile_count,
        ExactRate::new(8, temporal_evaluation::UNIFORM_SOURCE_FRAME_SLOTS as u32).unwrap()
    );
    assert_eq!(
        aggregate
            .family_defect_rates
            .iter()
            .map(|(family, _)| *family)
            .collect::<Vec<_>>(),
        vec![
            CaseFamily::MovementReversal,
            CaseFamily::Flicker,
            CaseFamily::TransientLayout,
        ]
    );
    let reversed = scores.into_iter().rev().collect::<Vec<_>>();
    let reversed_aggregate = aggregate_condition(
        ConditionId::BUniformStoryboard,
        &reversed,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    assert_eq!(aggregate, reversed_aggregate);
}

#[test]
fn threshold_assessment_enforces_all_v1_gates_and_reports_e_without_using_it() {
    let aggregate_values = aggregates();
    let assessment = assess_thresholds(
        &aggregate_values,
        &packages(),
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Pass
    );
    assert!(assessment.final_vs_bundle.passed);
    assert!(
        assessment
            .required_family_improvements
            .iter()
            .all(|check| check.passed)
    );
    assert!(assessment.bundle_vs_uniform.passed);
    assert!(assessment.stable_false_positive_delta.passed);
    assert!(assessment.progressive_report.passed);
    let mut reordered = aggregate_values.clone();
    reordered.reverse();
    assert_eq!(
        assessment,
        assess_thresholds(&reordered, &packages(), &ThresholdProfile::canonical()).unwrap()
    );

    let without_d = aggregate_values
        .iter()
        .filter(|aggregate| aggregate.condition_id != ConditionId::DTemporalBundle)
        .cloned()
        .collect::<Vec<_>>();
    let no_d = assess_thresholds(&without_d, &packages(), &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        no_d.progressive_report.status,
        temporal_evaluation::EvaluationStatus::Pass
    );
    assert_eq!(
        no_d.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
}

#[test]
fn complete_below_threshold_evidence_is_fail_not_inconclusive() {
    let digest = interval().digest().unwrap();
    let failed_bundle = aggregate_condition(
        ConditionId::DTemporalBundle,
        &scores(
            ConditionId::DTemporalBundle,
            &digest,
            DimensionOutcome::Incorrect,
            8,
        ),
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    let mut all = aggregates();
    all.retain(|aggregate| aggregate.condition_id != ConditionId::DTemporalBundle);
    all.push(failed_bundle);
    let assessment = assess_thresholds(&all, &packages(), &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Fail
    );
    assert_eq!(
        assessment.failure.as_ref().unwrap().code,
        RunFailureCode::Threshold
    );
    assert_eq!(
        assessment.final_vs_bundle.status,
        temporal_evaluation::EvaluationStatus::Fail
    );
}

#[test]
fn missing_coverage_pair_mismatch_and_bad_traceability_never_pass() {
    let digest = interval().digest().unwrap();
    let short = scores(
        ConditionId::AFinalScreenshot,
        &digest,
        DimensionOutcome::Incorrect,
        1,
    )
    .into_iter()
    .take(9)
    .collect::<Vec<_>>();
    let short_aggregate = aggregate_condition(
        ConditionId::AFinalScreenshot,
        &short,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    let mut all = aggregates();
    all.retain(|aggregate| aggregate.condition_id != ConditionId::AFinalScreenshot);
    all.push(short_aggregate);
    let assessment = assess_thresholds(&all, &packages(), &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );

    let mut mismatched_scores = scores(
        ConditionId::DTemporalBundle,
        &digest,
        DimensionOutcome::Correct,
        8,
    );
    mismatched_scores[0].trial_id = "synthetic/different-trial".into();
    let mismatched = aggregate_condition(
        ConditionId::DTemporalBundle,
        &mismatched_scores,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    let mut pair_mismatch = aggregates();
    pair_mismatch.retain(|aggregate| aggregate.condition_id != ConditionId::DTemporalBundle);
    pair_mismatch.push(mismatched);
    let assessment =
        assess_thresholds(&pair_mismatch, &packages(), &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );

    let no_packages =
        assess_thresholds(&aggregates(), &[], &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        no_packages.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
}

#[test]
fn gaps_retention_loss_and_corrupt_outputs_remain_inconclusive() {
    let mut gap_source = interval();
    let gap_frames = gap_source
        .frames
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, mut frame)| {
            if index == 11 {
                frame.availability = EvidenceAvailability::Gap;
            }
            frame
        })
        .collect();
    gap_source = SourceInterval::new(
        gap_source.interval_id,
        gap_source.session_scope,
        gap_source.requested_range,
        gap_source.resolved_range,
        gap_source.anchor_session_time_ns,
        gap_frames,
        vec![GapEvidence::new("gap-1", 11_000, 11_000, "capture gap", None).unwrap()],
        RetentionState::PartiallyRetained,
    )
    .unwrap();
    let gap_scores = scores(
        ConditionId::BUniformStoryboard,
        &gap_source.digest().unwrap(),
        DimensionOutcome::Correct,
        8,
    );
    let mut gap_scores = gap_scores;
    gap_scores[0].status = temporal_evaluation::EvaluationStatus::Inconclusive;
    gap_scores[0].failure = Some(FailureRecord {
        code: RunFailureCode::CaptureGap,
        phase: "synthetic".into(),
        reason: "declared gap".into(),
        recovery: "recapture".into(),
        retryable: true,
    });
    let gap_aggregate = aggregate_condition(
        ConditionId::BUniformStoryboard,
        &gap_scores,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    let mut all = aggregates();
    all.retain(|aggregate| aggregate.condition_id != ConditionId::BUniformStoryboard);
    all.push(gap_aggregate);
    let mut gap_packages = packages();
    gap_packages.retain(|package| package.condition_id != ConditionId::BUniformStoryboard);
    gap_packages.push(ConditionPackager::uniform_storyboard(&gap_source).unwrap());
    let assessment =
        assess_thresholds(&all, &gap_packages, &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
    assert_eq!(
        assessment.failure.as_ref().unwrap().code,
        RunFailureCode::CaptureGap
    );

    let mut retained_source = interval();
    let retained_frames = retained_source
        .frames
        .iter()
        .cloned()
        .enumerate()
        .map(|(index, mut frame)| {
            if index == 11 {
                frame.availability = EvidenceAvailability::Evicted;
            }
            frame
        })
        .collect();
    retained_source = SourceInterval::new(
        retained_source.interval_id,
        retained_source.session_scope,
        retained_source.requested_range,
        retained_source.resolved_range,
        retained_source.anchor_session_time_ns,
        retained_frames,
        Vec::new(),
        RetentionState::PartiallyRetained,
    )
    .unwrap();
    let retained_scores = scores(
        ConditionId::BUniformStoryboard,
        &retained_source.digest().unwrap(),
        DimensionOutcome::Correct,
        8,
    );
    let retained_aggregate = aggregate_condition(
        ConditionId::BUniformStoryboard,
        &retained_scores,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    let mut all = aggregates();
    all.retain(|aggregate| aggregate.condition_id != ConditionId::BUniformStoryboard);
    all.push(retained_aggregate);
    let mut retained_packages = packages();
    retained_packages.retain(|package| package.condition_id != ConditionId::BUniformStoryboard);
    retained_packages.push(ConditionPackager::uniform_storyboard(&retained_source).unwrap());
    let assessment =
        assess_thresholds(&all, &retained_packages, &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
    assert_eq!(
        assessment.failure.as_ref().unwrap().code,
        RunFailureCode::Retention
    );

    let source = interval();
    let mut corrupt = artifact(
        &source,
        "storyboard-corrupt",
        ArtifactKind::ChangeAwareStoryboard,
        "temporal-storyboard",
        "1.1.0",
    );
    corrupt.output.availability = EvidenceAvailability::Corrupt;
    let corrupt_package =
        ConditionPackager::change_aware_storyboard(&source, vec![corrupt]).unwrap();
    let mut corrupt_packages = packages();
    corrupt_packages.retain(|package| package.condition_id != ConditionId::CChangeAwareStoryboard);
    corrupt_packages.push(corrupt_package);
    let assessment = assess_thresholds(
        &aggregates(),
        &corrupt_packages,
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
}

#[test]
fn status_precedence_and_mixed_skipped_are_explicit() {
    let blocked = aggregate_condition(
        ConditionId::AFinalScreenshot,
        &[],
        &ThresholdProfile::canonical(),
    )
    .unwrap();
    assert_eq!(
        blocked.status,
        temporal_evaluation::EvaluationStatus::Blocked
    );

    let mut all = aggregates();
    all.retain(|aggregate| aggregate.condition_id != ConditionId::AFinalScreenshot);
    all.push(blocked);
    let assessment = assess_thresholds(&all, &packages(), &ThresholdProfile::canonical()).unwrap();
    assert_eq!(
        assessment.status,
        temporal_evaluation::EvaluationStatus::Blocked
    );

    let mut skipped = aggregates();
    let aggregate = skipped
        .iter_mut()
        .find(|aggregate| aggregate.condition_id == ConditionId::AFinalScreenshot)
        .unwrap();
    aggregate.status = temporal_evaluation::EvaluationStatus::Skipped;
    aggregate.failure = Some(FailureRecord {
        code: RunFailureCode::OptionalUnavailable,
        phase: "synthetic".into(),
        reason: "optional unavailable".into(),
        recovery: "retry later".into(),
        retryable: true,
    });
    assert!(assess_thresholds(&skipped, &packages(), &ThresholdProfile::canonical()).is_err());
}
