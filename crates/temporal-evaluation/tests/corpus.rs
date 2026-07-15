use std::fs;
use std::path::PathBuf;

use temporal_evaluation::{
    AnswerRegion, AnswerTruth, AnswerValidationContext, BENCHMARK_ID, BenchmarkDefinition,
    ConditionId, DURATIONS_MS, DebuggingAnswer, FixtureFile, InterpretationAnswer, Judgment,
    MATRIX_SEED, MotionBehavior, Rect, ScoringDimensionId, StateLabel, UncertaintyReason,
    VIEWPORT_HEIGHT, VIEWPORT_WIDTH, benchmark_definition_schema, parse_interpretation_answer,
    validate_debugging_answer,
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
fn every_case_has_explicit_evaluator_owned_truth_in_corrected_roi_space() {
    let definition = definition();
    let expected = [
        (
            "movement-reversal/basic",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Reversal,
            Judgment::Defective,
            Rect {
                x: 49,
                y: 73,
                width: 480,
                height: 120,
            },
        ),
        (
            "flicker/visibility",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Flicker,
            Judgment::Defective,
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
        ),
        (
            "flicker/color",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Flicker,
            Judgment::Defective,
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
        ),
        (
            "flicker/text",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Flicker,
            Judgment::Defective,
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
        ),
        (
            "layout/width",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::LayoutShift,
            Judgment::Defective,
            Rect {
                x: 49,
                y: 241,
                width: 640,
                height: 160,
            },
        ),
        (
            "layout/content-shift",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::LayoutShift,
            Judgment::Defective,
            Rect {
                x: 49,
                y: 223,
                width: 640,
                height: 202,
            },
        ),
        (
            "layout/scroll-position",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::LayoutShift,
            Judgment::Defective,
            Rect {
                x: 49,
                y: 241,
                width: 320,
                height: 120,
            },
        ),
        (
            "dom-opaque/path-reversal",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Reversal,
            Judgment::Defective,
            Rect {
                x: 401,
                y: 241,
                width: 320,
                height: 160,
            },
        ),
        (
            "dom-opaque/teleport",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Teleport,
            Judgment::Defective,
            Rect {
                x: 401,
                y: 241,
                width: 320,
                height: 160,
            },
        ),
        (
            "dom-opaque/sprite",
            AnswerTruth::Yes,
            vec![StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
            MotionBehavior::Flicker,
            Judgment::Defective,
            Rect {
                x: 401,
                y: 241,
                width: 320,
                height: 160,
            },
        ),
        (
            "stable/smooth-panel",
            AnswerTruth::No,
            vec![StateLabel::IntentionalMotion, StateLabel::Final],
            MotionBehavior::Monotonic,
            Judgment::Intentional,
            Rect {
                x: 49,
                y: 73,
                width: 480,
                height: 120,
            },
        ),
        (
            "stable/loading-indicator",
            AnswerTruth::No,
            vec![StateLabel::IntentionalMotion, StateLabel::Final],
            MotionBehavior::None,
            Judgment::Intentional,
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
        ),
        (
            "stable/caret",
            AnswerTruth::No,
            vec![StateLabel::IntentionalMotion],
            MotionBehavior::None,
            Judgment::Intentional,
            Rect {
                x: 49,
                y: 381,
                width: 300,
                height: 32,
            },
        ),
    ];
    assert_eq!(definition.cases.len(), expected.len());
    for (case, (case_id, temporary_state, state_order, motion_behavior, judgment, region)) in
        definition.cases.iter().zip(expected)
    {
        assert_eq!(case.case_id, case_id);
        assert_eq!(case.ground_truth.temporary_state, temporary_state);
        assert_eq!(case.ground_truth.state_order, state_order);
        assert_eq!(case.ground_truth.motion_behavior, motion_behavior);
        assert_eq!(case.ground_truth.judgment, judgment);
        assert_eq!(case.ground_truth.affected_region, region);
        assert_eq!(case.ground_truth.affected_region, case.affected_region);
    }
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

    let mut invalid = definition();
    invalid.cases[0].affected_region.width = 0;
    assert!(invalid.validate().is_err());

    let mut invalid = definition();
    invalid.cases[0].affected_region.x = VIEWPORT_WIDTH;
    assert!(invalid.validate().is_err());
}

#[test]
fn affected_regions_are_derived_from_viewport_geometry_not_fixture_local_coordinates() {
    let definition = definition();
    let css = String::from_utf8(fs::read(fixture_root().join("benchmark.css")).unwrap()).unwrap();
    let html = String::from_utf8(fs::read(fixture_root().join("index.html")).unwrap()).unwrap();
    let js = String::from_utf8(fs::read(fixture_root().join("benchmark.js")).unwrap()).unwrap();

    assert_eq!(css_value(&css, "body", "margin"), "0");
    assert!(css_block(&css, "*").contains("box-sizing: border-box"));
    let surface_padding = css_padding(&css, ".surface");
    assert_eq!(css_px(&css, ".surface", "width"), VIEWPORT_WIDTH);
    assert_eq!(css_px(&css, ".surface", "height"), VIEWPORT_HEIGHT);
    assert_eq!(surface_padding, (24, 40));

    let stage_border = css_first_px(&css, ".stage", "border");
    let stage = Rect {
        x: 0,
        y: 0,
        width: css_px(&css, ".stage", "width"),
        height: css_px(&css, ".stage", "height"),
    };
    let stage_origin = (
        surface_padding.1 + stage_border,
        surface_padding.0 + stage_border,
    );
    let stage_clip = Rect {
        x: stage_origin.0,
        y: stage_origin.1,
        width: stage.width - stage_border * 2,
        height: stage.height - stage_border * 2,
    };

    let panel = viewport_box(stage_origin, css_box(&css, ".panel"));
    let status = viewport_box(stage_origin, css_box(&css, ".status-card"));
    let content = viewport_box(stage_origin, css_box(&css, ".content-block"));
    let notice = viewport_box(stage_origin, css_box(&css, ".notice"));
    let scroll_box = viewport_box(stage_origin, css_box(&css, ".scroll-box"));
    let canvas = viewport_box(stage_origin, css_box(&css, "#visual-surface"));
    let field = viewport_box(stage_origin, css_box(&css, ".field"));
    let narrow_width = css_px(&css, ".content-block.narrow", "width");

    assert_eq!(html_attribute(&html, "canvas", "width"), 320);
    assert_eq!(html_attribute(&html, "canvas", "height"), 160);
    assert_eq!(canvas.width, 320);
    assert_eq!(canvas.height, 160);
    assert_eq!(narrow_width, 480);

    // These values are the fixture's local animation/drawing inputs. The assertions make their
    // coordinate space explicit and fail if a fixture edit changes the geometry without updating
    // this contract test and the evaluator-owned definition.
    for source_fragment in [
        "let x = 48;",
        "lerp(48, 160",
        "lerp(160, 120",
        "lerp(120, 288",
        "lerp(48, 288",
        "markerX = 80",
        "lerp(80, 320",
        "lerp(320, 240",
        "lerp(240, 320",
        "? 520 : elapsed < 100 ? 80 : 320",
        "markerX = 160",
        "context.fillRect(markerX - 16, 80 - 16, 32, 32)",
    ] {
        assert!(
            js.contains(source_fragment),
            "fixture geometry input drifted: {source_fragment}"
        );
    }
    assert!(js.contains("contentBlock.style.top = active ? \"264px\" : \"216px\""));
    assert!(
        js.contains(
            "scrollBox.scrollTop = elapsed >= activeStart && elapsed < activeEnd ? 160 : 0"
        )
    );

    let panel_path = Rect {
        x: panel.x,
        y: panel.y,
        width: panel.width + (288 - 48),
        height: panel.height,
    };
    let shifted_content = Rect {
        x: content.x,
        y: stage_origin.1 + 264,
        width: content.width,
        height: content.height,
    };
    let content_shift = clip_rect(
        union_rect(union_rect(notice, content), shifted_content),
        stage_clip,
    );
    let expected = [
        ("movement-reversal/basic", panel_path),
        ("flicker/visibility", status),
        ("flicker/color", status),
        ("flicker/text", status),
        ("layout/width", content),
        ("layout/content-shift", content_shift),
        ("layout/scroll-position", scroll_box),
        ("dom-opaque/path-reversal", canvas),
        ("dom-opaque/teleport", canvas),
        ("dom-opaque/sprite", canvas),
        ("stable/smooth-panel", panel_path),
        ("stable/loading-indicator", status),
        ("stable/caret", field),
    ];
    for (case_id, expected_region) in expected {
        let case_definition = definition
            .case(case_id)
            .unwrap_or_else(|| panic!("missing canonical case {case_id}"));
        assert_eq!(
            case_definition.affected_region, expected_region,
            "affected_region for {case_id} is not the fixture's viewport-pixel extent"
        );
        assert!(expected_region.x + expected_region.width <= VIEWPORT_WIDTH);
        assert!(expected_region.y + expected_region.height <= VIEWPORT_HEIGHT);
    }
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

fn css_block<'a>(css: &'a str, selector: &str) -> &'a str {
    let marker = format!("{selector} {{");
    let start = css
        .rfind(&marker)
        .unwrap_or_else(|| panic!("fixture CSS selector is missing: {selector}"))
        + marker.len();
    let end = start
        + css[start..]
            .find('}')
            .unwrap_or_else(|| panic!("fixture CSS block is unterminated: {selector}"));
    &css[start..end]
}

fn css_value<'a>(css: &'a str, selector: &str, property: &str) -> &'a str {
    let prefix = format!("{property}:");
    css_block(css, selector)
        .lines()
        .find_map(|line| {
            let line = line.trim();
            line.strip_prefix(&prefix)
                .map(|value| value.trim().trim_end_matches(';').trim())
        })
        .unwrap_or_else(|| panic!("fixture CSS declaration is missing: {selector} {property}"))
}

fn css_px(css: &str, selector: &str, property: &str) -> u32 {
    css_value(css, selector, property)
        .strip_suffix("px")
        .unwrap_or_else(|| panic!("fixture CSS value is not in pixels: {selector} {property}"))
        .parse()
        .unwrap_or_else(|_| panic!("fixture CSS value is not an integer: {selector} {property}"))
}

fn css_first_px(css: &str, selector: &str, property: &str) -> u32 {
    css_value(css, selector, property)
        .split_whitespace()
        .next()
        .unwrap()
        .strip_suffix("px")
        .unwrap()
        .parse()
        .unwrap()
}

fn css_padding(css: &str, selector: &str) -> (u32, u32) {
    let values = css_value(css, selector, "padding")
        .split_whitespace()
        .map(|value| value.strip_suffix("px").unwrap().parse().unwrap())
        .collect::<Vec<u32>>();
    match values.as_slice() {
        [vertical, horizontal] => (*vertical, *horizontal),
        _ => panic!("fixture surface padding must have vertical and horizontal values"),
    }
}

fn css_box(css: &str, selector: &str) -> Rect {
    Rect {
        x: css_px(css, selector, "left"),
        y: css_px(css, selector, "top"),
        width: css_px(css, selector, "width"),
        height: css_px(css, selector, "height"),
    }
}

fn viewport_box(origin: (u32, u32), local: Rect) -> Rect {
    Rect {
        x: origin.0 + local.x,
        y: origin.1 + local.y,
        width: local.width,
        height: local.height,
    }
}

fn union_rect(first: Rect, second: Rect) -> Rect {
    let right = (first.x + first.width).max(second.x + second.width);
    let bottom = (first.y + first.height).max(second.y + second.height);
    Rect {
        x: first.x.min(second.x),
        y: first.y.min(second.y),
        width: right - first.x.min(second.x),
        height: bottom - first.y.min(second.y),
    }
}

fn clip_rect(rect: Rect, clip: Rect) -> Rect {
    let right = (rect.x + rect.width).min(clip.x + clip.width);
    let bottom = (rect.y + rect.height).min(clip.y + clip.height);
    let x = rect.x.max(clip.x);
    let y = rect.y.max(clip.y);
    Rect {
        x,
        y,
        width: right - x,
        height: bottom - y,
    }
}

fn html_attribute(html: &str, element: &str, attribute: &str) -> u32 {
    let start = html
        .find(&format!("<{element}"))
        .unwrap_or_else(|| panic!("fixture element is missing: {element}"));
    let end = start + html[start..].find('>').unwrap();
    let tag = &html[start..end];
    let marker = format!("{attribute}=\"");
    tag[tag.find(&marker).unwrap() + marker.len()..]
        .split('"')
        .next()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn noncanonical_json_is_not_accepted_as_the_current_definition() {
    let definition = definition();
    let noncanonical = serde_json::to_vec(&definition).unwrap();
    assert_ne!(noncanonical, DEFINITION_BYTES);
    assert!(BenchmarkDefinition::from_canonical_json(&noncanonical).is_err());
}
