use std::fs;
use std::path::PathBuf;

use temporal_evaluation::{
    AnswerRegion, AnswerTruth, AnswerValidationContext, BENCHMARK_ID, BenchmarkDefinition,
    ConditionId, DURATIONS_MS, DebuggingAnswer, FixtureFile, InterpretationAnswer, Judgment,
    MATRIX_SEED, MotionBehavior, ScoringDimensionId, StateLabel, UncertaintyReason,
    benchmark_definition_schema, parse_interpretation_answer, validate_debugging_answer,
};

const DEFINITION_BYTES: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/benchmark-definition.json");
const SCHEMA_PATH: &str =
    "../../docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json";
const FIXTURE_ROOT: &str = "../../tests/fixtures/browser/temporal-benchmark";

fn definition() -> BenchmarkDefinition {
    BenchmarkDefinition::from_canonical_json(DEFINITION_BYTES)
        .expect("committed benchmark definition must be canonical and valid")
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(FIXTURE_ROOT)
}

#[test]
fn committed_definition_loads_from_the_current_single_contract() {
    let definition = definition();
    assert_eq!(definition.benchmark_id, BENCHMARK_ID);
    assert_eq!(definition.duration_ms, DURATIONS_MS);
    assert_eq!(definition.cases.len(), 13);
    assert_eq!(definition.cases[0].case_id, "movement-reversal/basic");
    assert_eq!(definition.cases.last().unwrap().case_id, "stable/caret");
    assert_eq!(definition.canonical_bytes().unwrap(), DEFINITION_BYTES);
}

#[test]
fn generated_schema_matches_the_committed_schema() {
    let mut expected = serde_json::to_vec_pretty(&benchmark_definition_schema()).unwrap();
    expected.push(b'\n');
    let committed = fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_PATH))
        .expect("generated benchmark schema must be committed");
    assert_eq!(committed, expected);
}

#[test]
fn fixture_file_identities_are_sha256_of_the_ordered_committed_files() {
    let definition = definition();
    let mut previous = None;
    for file in &definition.fixture.files {
        if let Some(previous) = previous {
            assert!(
                previous < file.path.as_str(),
                "fixture file order is not canonical"
            );
        }
        previous = Some(file.path.as_str());
        let path = fixture_root().join(&file.path);
        let bytes = fs::read(&path).unwrap_or_else(|error| {
            panic!("fixture file {} must be readable: {error}", path.display())
        });
        let actual = FixtureFile::from_bytes(file.path.clone(), &bytes).unwrap();
        assert_eq!(actual.sha256, file.sha256, "fixture hash drifted: {path:?}");
    }
}

#[test]
fn canonical_case_registry_has_exact_phase_duration_and_final_state_contracts() {
    let definition = definition();
    let expected_ids = [
        "movement-reversal/basic",
        "flicker/visibility",
        "flicker/color",
        "flicker/text",
        "layout/width",
        "layout/content-shift",
        "layout/scroll-position",
        "dom-opaque/path-reversal",
        "dom-opaque/teleport",
        "dom-opaque/sprite",
        "stable/smooth-panel",
        "stable/loading-indicator",
        "stable/caret",
    ];
    let actual_ids = definition
        .cases
        .iter()
        .map(|case| case.case_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(actual_ids, expected_ids);

    for case in &definition.cases {
        assert_eq!(case.anchor_id, "run");
        assert!(!case.phases.is_empty());
        assert!(
            case.phases
                .iter()
                .any(|phase| phase.state_id == case.final_state_id)
        );
        assert_eq!(
            case.phases.last().unwrap().end,
            temporal_evaluation::PhaseBoundary::End
        );
        for duration in DURATIONS_MS {
            assert!(definition.supports_duration(duration));
            assert!(
                case.phases
                    .windows(2)
                    .all(|phases| phases[0].end.resolve_for_duration(duration)
                        == phases[1].start.resolve_for_duration(duration))
            );
        }
    }

    assert!(
        definition
            .cases
            .iter()
            .filter(|case| case.intent == temporal_evaluation::CaseIntent::Defect)
            .all(|case| case.defect_interval.is_some())
    );
    assert!(
        definition
            .cases
            .iter()
            .filter(|case| case.intent == temporal_evaluation::CaseIntent::Intentional)
            .all(|case| case.defect_interval.is_none())
    );
}

#[test]
fn deterministic_capture_and_interpretation_matrices_are_platform_independent() {
    let definition = definition();
    let capture = definition
        .matrix
        .capture_trials(&definition.cases, &definition.duration_ms)
        .unwrap();
    let capture_again = definition
        .matrix
        .capture_trials(&definition.cases, &definition.duration_ms)
        .unwrap();
    assert_eq!(capture, capture_again);
    assert_eq!(capture.len(), 13 * 5 * 30);
    assert_eq!(
        capture.first().unwrap().trial_id,
        "capture:movement-reversal/basic/16/0"
    );
    assert_eq!(
        capture[29].trial_id,
        "capture:movement-reversal/basic/16/29"
    );
    assert_eq!(capture[30].trial_id, "capture:movement-reversal/basic/33/0");
    assert_eq!(
        capture.last().unwrap().trial_id,
        "capture:stable/smooth-panel/200/29"
    );

    let conditions = ConditionId::ALL.to_vec();
    let interpretation = definition
        .matrix
        .interpretation_trials(&definition.cases, &definition.duration_ms, &conditions)
        .unwrap();
    let interpretation_again = definition
        .matrix
        .interpretation_trials(&definition.cases, &definition.duration_ms, &conditions)
        .unwrap();
    assert_eq!(interpretation, interpretation_again);
    assert_eq!(interpretation.len(), 13 * 5 * 5 * 10);
    assert!(
        interpretation
            .iter()
            .all(|trial| conditions.contains(&trial.condition_id))
    );
    assert_eq!(definition.matrix.seed, MATRIX_SEED);
}

#[test]
fn matrix_coverage_uses_explicit_non_passing_statuses() {
    let matrix = &definition().matrix;
    assert_eq!(
        matrix.coverage_status(true, 0, 10),
        temporal_evaluation::EvaluationStatus::Blocked
    );
    assert_eq!(
        matrix.coverage_status(true, 9, 10),
        temporal_evaluation::EvaluationStatus::Inconclusive
    );
    assert_eq!(
        matrix.coverage_status(true, 10, 10),
        temporal_evaluation::EvaluationStatus::Pass
    );
    assert_eq!(
        matrix.coverage_status(false, 0, 10),
        temporal_evaluation::EvaluationStatus::Skipped
    );
    assert_eq!(
        matrix.coverage_status(false, 10, 10),
        temporal_evaluation::EvaluationStatus::Pass
    );
}

#[test]
fn conditions_and_scoring_vocabulary_are_one_exact_registry() {
    let definition = definition();
    assert_eq!(
        definition
            .conditions
            .iter()
            .map(|condition| condition.condition_id)
            .collect::<Vec<_>>(),
        ConditionId::ALL
    );
    assert_eq!(
        definition
            .scoring
            .dimensions
            .iter()
            .map(|dimension| dimension.id)
            .collect::<Vec<_>>(),
        ScoringDimensionId::ALL
    );
    for condition in &definition.conditions {
        assert_eq!(condition.scoring_dimension_ids, ScoringDimensionId::ALL);
        assert_eq!(
            condition.source_interval_policy,
            temporal_evaluation::SourceIntervalPolicy::SameCapturedSourceInterval
        );
    }
    let serialized = serde_json::to_string(&definition.conditions).unwrap();
    for forbidden in [
        "movement-reversal",
        "flicker",
        "transient-layout",
        "dom-opaque",
        "stable-control",
        "case_id",
        "variant",
        "ground truth",
    ] {
        assert!(
            !serialized.contains(forbidden),
            "condition leaks {forbidden}"
        );
    }
}

#[test]
fn prompts_have_exact_hashes_bounded_answers_and_no_fixture_metadata() {
    let definition = definition();
    definition.prompts.validate().unwrap();
    for template in &definition.prompts.templates {
        assert_eq!(template.sha256, template.computed_sha256().unwrap());
        let text = format!("{} {}", template.system_prompt, template.task_prompt).to_lowercase();
        for forbidden in [
            "movement-reversal",
            "flicker",
            "transient layout",
            "dom-opaque",
            "stable control",
            "case id",
            "variant",
            "ground truth",
        ] {
            assert!(!text.contains(forbidden), "prompt leaks {forbidden}");
        }
    }

    let answer = InterpretationAnswer {
        temporary_state: AnswerTruth::Uncertain,
        state_order: vec![StateLabel::Baseline, StateLabel::Unknown],
        affected_region: AnswerRegion::Unknown,
        motion_behavior: MotionBehavior::Uncertain,
        judgment: Judgment::Uncertain,
        uncertainty_reasons: vec![UncertaintyReason::CaptureGap],
        evidence_refs: vec!["frame_1".into()],
    };
    let bytes = serde_json::to_vec(&answer).unwrap();
    parse_interpretation_answer(
        &bytes,
        AnswerValidationContext {
            unresolved_capture_gap: true,
            missing_source: false,
        },
    )
    .unwrap();

    let mut invalid = serde_json::to_value(answer).unwrap();
    invalid["unexpected"] = serde_json::json!(true);
    assert!(
        parse_interpretation_answer(
            serde_json::to_string(&invalid).unwrap().as_bytes(),
            AnswerValidationContext {
                unresolved_capture_gap: false,
                missing_source: false,
            },
        )
        .is_err()
    );

    let debugging = DebuggingAnswer {
        reproduced: AnswerTruth::Yes,
        diagnosis: "supported diagnosis".into(),
        patch_applied: AnswerTruth::Yes,
        final_state_verified: AnswerTruth::Yes,
        temporal_behavior_verified: AnswerTruth::Yes,
        evidence_refs: vec!["artifact_1".into()],
    };
    validate_debugging_answer(&debugging).unwrap();
    let mut invalid_debugging = debugging;
    invalid_debugging.diagnosis = "x".repeat(513);
    assert!(validate_debugging_answer(&invalid_debugging).is_err());
}

#[test]
fn input_identities_change_when_a_canonical_definition_input_changes() {
    let definition = definition();
    let mut changed = definition.clone();
    changed.matrix.seed = definition.matrix.seed.wrapping_add(1);
    assert!(changed.validate().is_err());
    assert_ne!(
        definition.input_identities.matrix_sha256,
        temporal_evaluation::sha256_prefixed(
            &temporal_evaluation::canonical_json(&changed.matrix).unwrap()
        )
    );
}

#[test]
fn invalid_case_duration_phase_and_final_state_edits_are_rejected() {
    let mut invalid = definition();
    invalid.duration_ms[0] = 17;
    assert!(invalid.validate().is_err());

    let mut invalid = definition();
    invalid.cases[0].final_state_id = "movement.not-a-state".into();
    assert!(invalid.validate().is_err());

    let mut invalid = definition();
    invalid.cases[0].phases[0].end = temporal_evaluation::PhaseBoundary::OffsetMs { value: 99 };
    assert!(invalid.validate().is_err());

    let mut invalid = definition();
    invalid.cases[0].defect_interval = None;
    assert!(invalid.validate().is_err());
}

#[test]
fn target_source_is_local_static_and_does_not_use_clock_random_or_network_apis() {
    let root = fixture_root();
    let source = ["README.md", "benchmark.css", "benchmark.js", "index.html"]
        .into_iter()
        .map(|file| String::from_utf8(fs::read(root.join(file)).unwrap()).unwrap())
        .collect::<Vec<_>>()
        .join("\n");

    for forbidden in [
        "http://",
        "https://",
        "fetch(",
        "XMLHttpRequest",
        "WebSocket",
        "EventSource",
        "sendBeacon",
        "new Date",
        "Date(",
        "setTimeout",
        "setInterval",
        "Math.random",
        "crypto.getRandomValues",
    ] {
        assert!(!source.contains(forbidden), "fixture contains {forbidden}");
    }
    assert!(source.contains("performance.now()"));
    assert!(source.contains("requestAnimationFrame"));
}

#[test]
fn every_run_resets_the_same_visual_baseline_before_animation() {
    let script =
        String::from_utf8(fs::read(fixture_root().join("benchmark.js")).expect("benchmark script"))
            .unwrap();
    let reset = script
        .find("function resetVisuals()")
        .expect("reset function");
    let run = script.find("function runScenario()").expect("run function");
    assert!(reset < run);
    assert!(script[run..].contains("resetVisuals();"));
    assert!(script.ends_with("  resetVisuals();\n})();\n"));
    for baseline in [
        "panel.style.transform = \"translateX(0px)\"",
        "statusCard.hidden = false",
        "statusText.textContent = \"Ready\"",
        "contentBlock.style.top = \"216px\"",
        "scrollBox.scrollTop = 0",
        "caret.classList.remove(\"off\")",
        "drawSurface(\"baseline\", 0)",
    ] {
        assert!(
            script[reset..run].contains(baseline),
            "reset misses {baseline}"
        );
    }
}

#[test]
fn agent_facing_markup_does_not_render_ground_truth_labels_or_case_identity() {
    let html = String::from_utf8(fs::read(fixture_root().join("index.html")).unwrap()).unwrap();
    for hidden_label in [
        "movement-reversal",
        "flicker",
        "transient",
        "dom-opaque",
        "stable-control",
        "ground-truth",
        "phase-id",
        "defect",
    ] {
        assert!(!html.contains(hidden_label), "markup leaks {hidden_label}");
    }
    assert!(!html.contains("data-"));
}

#[test]
fn noncanonical_json_is_not_accepted_as_the_current_definition() {
    let definition = definition();
    let noncanonical = serde_json::to_vec(&definition).unwrap();
    assert_ne!(noncanonical, DEFINITION_BYTES);
    assert!(BenchmarkDefinition::from_canonical_json(&noncanonical).is_err());
}
