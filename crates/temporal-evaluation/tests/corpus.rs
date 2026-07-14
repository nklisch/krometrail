use std::fs;
use std::path::PathBuf;

use temporal_evaluation::{
    BENCHMARK_ID, BenchmarkDefinition, DURATIONS_MS, FixtureFile, benchmark_definition_schema,
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
