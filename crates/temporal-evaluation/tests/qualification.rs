mod support;

use std::{fs, path::PathBuf, process::Command};

use temporal_evaluation::{
    ConditionEvidence, ConditionId, ConditionPackager, EvaluationStatus, EvidenceAvailability,
    EvidenceReferenceKind, NonClaimId, RunFailureCode, ScoreInput, ThesisEligibility,
    UncertaintyReason, aggregate_condition, sample_evaluation_result, score_interpretation,
};

use support::{
    FakeMonotonicClock, bundle, corrupt_change_aware_package, digest, gap_package, hash, interval,
    interval_with_frame_availability, interval_with_orders, movement_trial, packages,
    partial_eviction_package, perfect_answer, synthetic_score, uncertainty_answer,
};

#[test]
fn qualification_packages_all_conditions_over_one_interval_with_fixed_budgets() {
    let packages = packages();
    assert_eq!(
        packages
            .iter()
            .map(|package| package.condition_id)
            .collect::<Vec<_>>(),
        ConditionId::ALL
    );

    for package in &packages {
        package.validate().unwrap();
        assert_eq!(
            package.source_interval_digest,
            packages[0].source_interval_digest
        );
        assert_eq!(package.source_frame_ids, packages[0].source_frame_ids);
        assert_eq!(package.gap_ids, packages[0].gap_ids);
        assert_eq!(package.non_claims, NonClaimId::ALL);
    }

    match &packages[0].evidence {
        ConditionEvidence::FinalScreenshot {
            final_frame_id,
            current_observation,
        } => {
            assert_eq!(final_frame_id, "frame-11");
            assert_eq!(
                current_observation.kind,
                EvidenceReferenceKind::CurrentObservation
            );
        }
        _ => panic!("A must be the final screenshot condition"),
    }
    match &packages[1].evidence {
        ConditionEvidence::UniformStoryboard { slot_frame_ids } => {
            assert_eq!(slot_frame_ids.len(), 8);
            assert!(slot_frame_ids.windows(2).all(|pair| pair[0] != pair[1]));
        }
        _ => panic!("B must use uniform source slots"),
    }
    let uniform_slots = match &packages[1].evidence {
        ConditionEvidence::UniformStoryboard { slot_frame_ids } => slot_frame_ids,
        _ => unreachable!(),
    };
    match &packages[2].evidence {
        ConditionEvidence::ChangeAwareStoryboard { artifacts } => {
            assert_eq!(artifacts.len(), 1);
            assert!(!artifacts[0].selected_frame_ids.is_empty());
            assert!(artifacts[0].selected_frame_ids.len() <= 8);
            assert_ne!(
                artifacts[0].selected_frame_ids.as_slice(),
                uniform_slots.as_slice()
            );
        }
        _ => panic!("C must preserve the existing change-aware authority"),
    }
    let d_bundle = match &packages[3].evidence {
        ConditionEvidence::TemporalBundle(bundle) => bundle,
        _ => panic!("D must preserve the temporal bundle"),
    };
    assert_eq!(d_bundle.before_during_after.len(), 1);
    assert_eq!(d_bundle.storyboards.len(), 1);
    assert_eq!(d_bundle.difference_maps.len(), 1);
    let e = match &packages[4].evidence {
        ConditionEvidence::ProgressiveSource(evidence) => evidence,
        _ => panic!("E must extend D with progressive access"),
    };
    assert_eq!(&e.bundle, d_bundle);
    assert_eq!(e.source_retrievals.len(), 2);
    assert!(
        e.source_retrievals
            .iter()
            .all(|request| request.requested_frame_ids.len() <= 4)
    );
    assert!(e.region_filmstrip.is_some());
}

#[test]
fn fake_clock_wall_clock_filesystem_and_parallel_order_do_not_change_bytes() {
    let canonical_clock = FakeMonotonicClock::new();
    let canonical_order = (0..support::FRAME_COUNT).collect::<Vec<_>>();
    let reverse_order = canonical_order.iter().rev().copied().collect::<Vec<_>>();
    let canonical = interval_with_orders(&canonical_clock, &canonical_order, &reverse_order, 0, 11);
    let reordered_clock = FakeMonotonicClock::new();
    let reordered = interval_with_orders(
        &reordered_clock,
        &reverse_order,
        &canonical_order,
        17,
        u64::MAX,
    );
    assert_eq!(canonical, reordered);
    assert_ne!(canonical_clock.call_count(), reordered_clock.call_count());
    assert_eq!(
        canonical.canonical_bytes().unwrap(),
        reordered.canonical_bytes().unwrap()
    );

    let canonical_packages = packages();
    let parallel_bytes = std::thread::scope(|scope| {
        let mut handles = (0..5)
            .map(|_| {
                scope.spawn(|| {
                    packages()
                        .into_iter()
                        .map(|p| p.canonical_bytes().unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut outputs = Vec::new();
        while let Some(handle) = handles.pop() {
            outputs.push(handle.join().unwrap());
        }
        outputs
    });
    for output in &parallel_bytes {
        assert_eq!(
            output,
            &canonical_packages
                .iter()
                .map(|package| package.canonical_bytes().unwrap())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn scorer_and_result_bytes_are_stable_under_repeated_and_parallel_calls() {
    let package = packages().remove(1);
    let trial = movement_trial(&package);
    let raw = perfect_answer("frame-1");
    let expected = score_interpretation(ScoreInput {
        trial: &trial,
        package: &package,
        truth: &temporal_evaluation::BenchmarkDefinition::canonical()
            .case("movement-reversal/basic")
            .unwrap()
            .ground_truth,
        raw_answer: &raw,
        raw_answer_ref: "qualification-answer-sidecar",
    })
    .unwrap()
    .canonical_bytes()
    .unwrap();

    let score_bytes = std::thread::scope(|scope| {
        let mut handles = (0..6)
            .map(|_| {
                let package = package.clone();
                let trial = trial.clone();
                let raw = raw.clone();
                scope.spawn(move || {
                    score_interpretation(ScoreInput {
                        trial: &trial,
                        package: &package,
                        truth: &temporal_evaluation::BenchmarkDefinition::canonical()
                            .case("movement-reversal/basic")
                            .unwrap()
                            .ground_truth,
                        raw_answer: &raw,
                        raw_answer_ref: "qualification-answer-sidecar",
                    })
                    .unwrap()
                    .canonical_bytes()
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let mut outputs = Vec::new();
        while let Some(handle) = handles.pop() {
            outputs.push(handle.join().unwrap());
        }
        outputs
    });
    assert!(score_bytes.iter().all(|bytes| bytes == &expected));

    let result_bytes = std::thread::scope(|scope| {
        let mut handles = (0..6)
            .map(|_| {
                scope.spawn(|| {
                    sample_evaluation_result()
                        .unwrap()
                        .canonical_bytes()
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let mut outputs = Vec::new();
        while let Some(handle) = handles.pop() {
            outputs.push(handle.join().unwrap());
        }
        outputs
    });
    assert!(result_bytes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn one_field_identity_mutations_never_survive_validation() {
    let mut source = interval();
    source.frames[0].encoded_sha256 = hash(99);
    assert!(source.validate().is_err());

    let mut reordered_source = interval();
    reordered_source.frames.swap(0, 1);
    assert!(reordered_source.validate().is_err());

    let mut retimed_source = interval();
    retimed_source.frames[2].observed_time_ns += 1;
    assert!(retimed_source.validate().is_err());
    let mut source_clock_mutation = interval();
    source_clock_mutation.frames[2].source_time_ns = Some(99_999);
    assert!(source_clock_mutation.validate().is_err());

    let original = packages().remove(2);
    let changed_interval = interval_with_frame_availability(0, EvidenceAvailability::Evicted);
    let changed_package = ConditionPackager::uniform_storyboard(&changed_interval).unwrap();
    assert!(
        temporal_evaluation::require_one_source_interval(&[original.clone(), changed_package])
            .is_err()
    );
    let mut mutations = Vec::new();

    let mut package = original.clone();
    package.source_interval_digest = digest("different-source-interval");
    mutations.push(package);

    let mut package = original.clone();
    package.source_frame_ids.swap(0, 1);
    mutations.push(package);

    let mut package = original.clone();
    package.gap_ids.push("gap-1".into());
    mutations.push(package);

    let mut package = original.clone();
    package.retention = temporal_evaluation::RetentionState::PartiallyRetained;
    mutations.push(package);

    let mut package = original.clone();
    package.digest = digest("wrong-package-digest");
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].output.sha256 = Some(digest("mutated-output"));
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].manifest_sha256 = digest("mutated-manifest");
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].selected_frame_ids.reverse();
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.cache_schema_version = 2;
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.cache_key = digest("mutated-cache-key");
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.source_fingerprint = digest("mutated-sources");
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.parameter_hash = digest("mutated-parameters");
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.visual_epoch_hash = digest("mutated-dimensions-or-scale");
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].resolved_range.start_ns = 1;
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.adapter_version.version = "2".into();
    mutations.push(package);

    let mut package = original.clone();
    let ConditionEvidence::ChangeAwareStoryboard { artifacts } = &mut package.evidence else {
        unreachable!()
    };
    artifacts[0].cache.generator.version = "2.0.0".into();
    mutations.push(package);

    assert!(mutations.iter().all(|package| package.validate().is_err()));

    let result = sample_evaluation_result().unwrap();
    let mut result_mutation = result.clone();
    result_mutation.trials[0].source_interval_digest = digest("mutated-result-interval");
    assert!(result_mutation.validate().is_err());
}

#[test]
fn gaps_eviction_corruption_partial_and_unavailable_retrievals_are_not_passes() {
    let truth = temporal_evaluation::BenchmarkDefinition::canonical()
        .case("movement-reversal/basic")
        .unwrap()
        .ground_truth
        .clone();

    let gap = gap_package();
    let gap_score = score_interpretation(ScoreInput {
        trial: &movement_trial(&gap),
        package: &gap,
        truth: &truth,
        raw_answer: &uncertainty_answer("frame-0", UncertaintyReason::CaptureGap),
        raw_answer_ref: "gap-sidecar",
    })
    .unwrap();
    assert_eq!(gap_score.status, EvaluationStatus::Inconclusive);
    assert_eq!(
        gap_score.failure.as_ref().unwrap().code,
        RunFailureCode::CaptureGap
    );

    let evicted = partial_eviction_package();
    let evicted_score = score_interpretation(ScoreInput {
        trial: &movement_trial(&evicted),
        package: &evicted,
        truth: &truth,
        raw_answer: &uncertainty_answer("frame-1", UncertaintyReason::MissingSource),
        raw_answer_ref: "evicted-sidecar",
    })
    .unwrap();
    assert_eq!(evicted_score.status, EvaluationStatus::Inconclusive);
    assert_eq!(
        evicted_score.failure.as_ref().unwrap().code,
        RunFailureCode::Retention
    );

    let corrupt = corrupt_change_aware_package();
    let corrupt_score = score_interpretation(ScoreInput {
        trial: &movement_trial(&corrupt),
        package: &corrupt,
        truth: &truth,
        raw_answer: &uncertainty_answer("frame-0", UncertaintyReason::MissingSource),
        raw_answer_ref: "corrupt-sidecar",
    })
    .unwrap();
    assert_eq!(corrupt_score.status, EvaluationStatus::Inconclusive);
    assert_eq!(
        corrupt_score.failure.as_ref().unwrap().code,
        RunFailureCode::CorruptSource
    );

    let source = interval();
    let mut partial_bundle = bundle(&source);
    partial_bundle.storyboards[0].output.availability = EvidenceAvailability::NotCollected;
    let partial = ConditionPackager::temporal_bundle(&source, partial_bundle).unwrap();
    let partial_score = score_interpretation(ScoreInput {
        trial: &movement_trial(&partial),
        package: &partial,
        truth: &truth,
        raw_answer: &uncertainty_answer("frame-0", UncertaintyReason::MissingSource),
        raw_answer_ref: "partial-bundle-sidecar",
    })
    .unwrap();
    assert_eq!(partial_score.status, EvaluationStatus::Inconclusive);

    let unavailable = support::unavailable_retrieval_package();
    let unavailable_score = score_interpretation(ScoreInput {
        trial: &movement_trial(&unavailable),
        package: &unavailable,
        truth: &truth,
        raw_answer: &uncertainty_answer("frame-2", UncertaintyReason::MissingSource),
        raw_answer_ref: "unavailable-retrieval-sidecar",
    })
    .unwrap();
    assert_eq!(unavailable_score.status, EvaluationStatus::Inconclusive);
    assert!(unavailable_score.failure.is_some());
}

#[test]
fn skipped_rows_are_closed_only_when_every_row_is_optional_and_skipped() {
    let package = packages().remove(0);
    let pass = synthetic_score(
        &package,
        temporal_evaluation::CaseFamily::MovementReversal,
        0,
        EvaluationStatus::Pass,
    );
    let skipped = synthetic_score(
        &package,
        temporal_evaluation::CaseFamily::MovementReversal,
        1,
        EvaluationStatus::Skipped,
    );
    assert!(
        aggregate_condition(
            package.condition_id,
            &[pass, skipped.clone()],
            &support::threshold_profile(),
        )
        .unwrap_err()
        .to_string()
        .contains("condition aggregate rejects mixed skipped trial rows")
    );
    let aggregate = aggregate_condition(
        package.condition_id,
        &[skipped],
        &support::threshold_profile(),
    )
    .unwrap();
    assert_eq!(aggregate.status, EvaluationStatus::Skipped);
    assert_eq!(
        aggregate.failure.as_ref().unwrap().code,
        RunFailureCode::OptionalUnavailable
    );

    let blocked = aggregate_condition(
        ConditionId::AFinalScreenshot,
        &[],
        &support::threshold_profile(),
    )
    .unwrap();
    assert_eq!(blocked.status, EvaluationStatus::Blocked);
}

#[test]
fn synthetic_ci_result_is_always_not_eligible_and_keeps_every_non_claim() {
    let result = sample_evaluation_result().unwrap();
    assert_eq!(
        result.evidence_layer,
        temporal_evaluation::EvidenceLayer::DeterministicCi
    );
    assert_eq!(result.thesis_eligibility, ThesisEligibility::NotEligible);
    assert_eq!(result.non_claims, NonClaimId::ALL.to_vec());
    assert_eq!(
        result.canonical_bytes().unwrap(),
        result.canonical_bytes().unwrap()
    );

    let mut forged = result;
    forged.thesis_eligibility = ThesisEligibility::Eligible;
    assert!(forged.validate().is_err());
}

#[test]
fn clean_generation_reproduces_every_committed_contract_artifact_without_docs_or_run_output() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!(
        "krometrail-temporal-evaluation-generation-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();

    let definition = directory.join("benchmark-definition.json");
    let definition_schema = directory.join("benchmark-definition.schema.json");
    run_generator(
        "generate-benchmark-definition",
        &[&definition, &definition_schema],
    );
    assert_eq!(
        fs::read(&definition).unwrap(),
        include_bytes!("../../../docs/evidence/temporal-evaluation/v1/benchmark-definition.json")
    );
    assert_eq!(
        fs::read(&definition_schema).unwrap(),
        include_bytes!(
            "../../../docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json"
        )
    );

    let manifest = directory.join("sample-manifest.json");
    let manifest_schema = directory.join("run-manifest.schema.json");
    run_generator("generate-run-manifest", &[&manifest, &manifest_schema]);
    assert_eq!(
        fs::read(&manifest).unwrap(),
        include_bytes!("../../../docs/evidence/temporal-evaluation/v1/sample-manifest.json")
    );
    assert_eq!(
        fs::read(&manifest_schema).unwrap(),
        include_bytes!("../../../docs/evidence/temporal-evaluation/v1/run-manifest.schema.json")
    );

    let result = directory.join("sample-evaluation-result.json");
    let result_schema = directory.join("evaluation-result.schema.json");
    run_generator("generate-evaluation-result", &[&result, &result_schema]);
    assert_eq!(
        fs::read(&result).unwrap(),
        include_bytes!(
            "../../../docs/evidence/temporal-evaluation/v1/sample-evaluation-result.json"
        )
    );
    assert_eq!(
        fs::read(&result_schema).unwrap(),
        include_bytes!(
            "../../../docs/evidence/temporal-evaluation/v1/evaluation-result.schema.json"
        )
    );

    let docs_status = Command::new("git")
        .current_dir(&root)
        .args(["diff", "--quiet", "--", "docs/public/llms-full.txt"])
        .status()
        .unwrap();
    assert!(
        docs_status.success(),
        "generation must not modify VitePress docs"
    );
    let tracked_run_output = Command::new("git")
        .current_dir(&root)
        .args(["ls-files", "target/temporal-evaluation"])
        .output()
        .unwrap();
    assert!(tracked_run_output.status.success());
    assert!(tracked_run_output.stdout.is_empty());

    fs::remove_dir_all(directory).unwrap();
}

fn run_generator(name: &str, paths: &[&PathBuf]) {
    let binary = std::env::var_os(format!("CARGO_BIN_EXE_{name}"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let test_binary = std::env::current_exe().expect("qualification test executable path");
            test_binary
                .parent()
                .and_then(|deps| deps.parent())
                .expect("qualification test executable directory")
                .join(name)
        });
    let status = Command::new(binary)
        .args(paths.iter().map(|path| path.as_os_str()))
        .status()
        .unwrap();
    assert!(status.success(), "{name} failed");
}

#[test]
fn support_does_not_smuggle_truth_or_payloads_into_package_bytes() {
    for package in packages() {
        let text = String::from_utf8(package.canonical_bytes().unwrap()).unwrap();
        for forbidden in [
            "ground truth",
            "model answer",
            "page text",
            "data:image",
            "base64",
            "target/temporal-evaluation",
            "/tmp/",
        ] {
            assert!(
                !text.to_ascii_lowercase().contains(forbidden),
                "{forbidden}"
            );
        }
    }
}
