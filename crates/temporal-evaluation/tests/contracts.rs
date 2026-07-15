use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use temporal_evaluation::{
    BENCHMARK_ID, BenchmarkDefinition, CaseFamily, ConditionId, DURATIONS_MS, EvaluationStatus,
    FixtureFile, MATRIX_SEED, MatrixOrder, PromptId, RunManifest, ScoringDimensionId,
    benchmark_definition_schema, canonical_json, run_manifest_schema, sample_manifest,
    sha256_prefixed,
};

const DEFINITION_BYTES: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/benchmark-definition.json");
const DEFINITION_SCHEMA_BYTES: &[u8] = include_bytes!(
    "../../../docs/evidence/temporal-evaluation/v1/benchmark-definition.schema.json"
);
const MANIFEST_BYTES: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/sample-manifest.json");
const MANIFEST_SCHEMA_BYTES: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/run-manifest.schema.json");
const README_BYTES: &[u8] =
    include_bytes!("../../../docs/evidence/temporal-evaluation/v1/README.md");

const DEFINITION_DIGEST: &str =
    "sha256:ed3525d78a072c357c6a97fd5ba006dd23dae973fa8d80b6d309d121d0b7cb1d";
const DEFINITION_SCHEMA_DIGEST: &str =
    "sha256:2068c69ee0b9c6a3edc7072d1cfa6f63fe98094dcb7906288ab82cf3f6fbc36a";
const MANIFEST_DIGEST: &str =
    "sha256:7881c4b05db700757f52a6f5be854bb0c7c8e403bf66970fcfaa3922ac74c134";
const MANIFEST_SCHEMA_DIGEST: &str =
    "sha256:80069d18b74888eb005a18a81a3374368d6e061542ea3877d2b076938a6f9db3";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn generated_schema<T: serde::Serialize>(schema: &T) -> Vec<u8> {
    let mut bytes = serde_json::to_vec_pretty(schema).expect("schema serialization");
    bytes.push(b'\n');
    bytes
}

fn git_status(args: &[&str]) -> std::process::ExitStatus {
    Command::new("git")
        .current_dir(workspace_root())
        .args(args)
        .status()
        .unwrap_or_else(|error| panic!("git {args:?}: {error}"))
}

#[test]
fn committed_contract_artifacts_are_canonical_schema_backed_and_digestable() {
    let definition = BenchmarkDefinition::from_canonical_json(DEFINITION_BYTES)
        .expect("definition must be canonical and schema-backed");
    let manifest = RunManifest::from_canonical_json(MANIFEST_BYTES)
        .expect("sample manifest must be canonical and schema-backed");

    assert_eq!(definition.benchmark_id, BENCHMARK_ID);
    assert_eq!(definition.definition_digest().unwrap(), DEFINITION_DIGEST);
    assert_eq!(sha256_prefixed(DEFINITION_BYTES), DEFINITION_DIGEST);
    assert_eq!(
        sha256_prefixed(DEFINITION_SCHEMA_BYTES),
        DEFINITION_SCHEMA_DIGEST
    );
    assert_eq!(sha256_prefixed(MANIFEST_BYTES), MANIFEST_DIGEST);
    assert_eq!(
        sha256_prefixed(MANIFEST_SCHEMA_BYTES),
        MANIFEST_SCHEMA_DIGEST
    );
    assert_eq!(manifest.digest().unwrap(), MANIFEST_DIGEST);
    assert_eq!(manifest.canonical_bytes().unwrap(), MANIFEST_BYTES);
    assert_eq!(definition.canonical_bytes().unwrap(), DEFINITION_BYTES);

    assert_eq!(
        generated_schema(&benchmark_definition_schema()),
        DEFINITION_SCHEMA_BYTES
    );
    assert_eq!(
        generated_schema(&run_manifest_schema()),
        MANIFEST_SCHEMA_BYTES
    );
    assert_eq!(manifest, sample_manifest());
}

#[test]
fn fixture_order_hashes_and_registry_matrix_are_one_contract() {
    let definition = BenchmarkDefinition::from_canonical_json(DEFINITION_BYTES).unwrap();
    let fixture_root = workspace_root().join("tests/fixtures/browser/temporal-benchmark");
    let mut previous_path = None;
    for fixture in &definition.fixture.files {
        if let Some(previous) = previous_path {
            assert!(previous < fixture.path.as_str());
        }
        previous_path = Some(fixture.path.as_str());
        let bytes = fs::read(fixture_root.join(&fixture.path)).unwrap();
        assert_eq!(
            FixtureFile::from_bytes(fixture.path.clone(), &bytes)
                .unwrap()
                .sha256,
            fixture.sha256
        );
    }

    assert_eq!(definition.cases.len(), 13);
    assert_eq!(
        definition
            .cases
            .iter()
            .map(|case| case.family)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        CaseFamily::ALL.len()
    );
    assert_eq!(definition.duration_ms, DURATIONS_MS);
    assert_eq!(definition.matrix.seed, MATRIX_SEED);
    assert_eq!(
        definition.matrix.capture_order,
        MatrixOrder::FamilyCaseDurationRepetition
    );
    assert_eq!(
        definition.matrix.interpretation_order,
        MatrixOrder::SeededFisherYates
    );

    let capture = definition
        .matrix
        .capture_trials(&definition.cases, &definition.duration_ms)
        .unwrap();
    assert_eq!(capture.len(), 13 * DURATIONS_MS.len() * 30);
    assert_eq!(
        capture.first().unwrap().trial_id,
        "capture:movement-reversal/basic/16/0"
    );
    assert_eq!(
        capture.last().unwrap().trial_id,
        "capture:stable/smooth-panel/200/29"
    );

    let interpretation = definition
        .matrix
        .interpretation_trials(
            &definition.cases,
            &definition.duration_ms,
            &ConditionId::ALL,
        )
        .unwrap();
    assert_eq!(
        interpretation.len(),
        13 * DURATIONS_MS.len() * ConditionId::ALL.len() * 10
    );
    assert!(
        interpretation
            .iter()
            .all(|trial| ConditionId::ALL.contains(&trial.condition_id))
    );
    assert_eq!(definition.conditions.len(), ConditionId::ALL.len());
    assert_eq!(definition.prompts.templates.len(), PromptId::ALL.len());
    assert_eq!(
        definition.scoring.dimensions.len(),
        ScoringDimensionId::ALL.len()
    );
    definition.validate().unwrap();
    definition.prompts.validate().unwrap();
    for condition in &definition.conditions {
        condition.validate().unwrap();
    }
}

#[test]
fn status_prompt_privacy_and_non_live_boundaries_are_explicit() {
    let definition = BenchmarkDefinition::from_canonical_json(DEFINITION_BYTES).unwrap();
    for status in EvaluationStatus::ALL {
        assert!(serde_json::to_value(status).unwrap().is_string());
    }
    for prompt in &definition.prompts.templates {
        let text = format!("{} {}", prompt.system_prompt, prompt.task_prompt).to_ascii_lowercase();
        assert!(!text.contains("ground truth"));
        assert!(!text.contains("movement-reversal"));
    }

    let mut unsafe_manifest = sample_manifest();
    unsafe_manifest.fixture.root_relative_path = "/home/operator/private-fixture".into();
    assert!(unsafe_manifest.sanitize().is_err());
    unsafe_manifest = sample_manifest();
    unsafe_manifest.non_claims[0] = "https://127.0.0.1:9222/page".into();
    assert!(unsafe_manifest.sanitize().is_err());

    let root_manifest =
        fs::read_to_string(workspace_root().join("crates/temporal-evaluation/Cargo.toml")).unwrap();
    for forbidden in [
        "krometrail-cdp",
        "tokio",
        "reqwest",
        "openai",
        "anthropic",
        "mcp",
    ] {
        assert!(
            !root_manifest.contains(forbidden),
            "contract crate gained {forbidden}"
        );
    }
    let sample_text = String::from_utf8(MANIFEST_BYTES.to_vec()).unwrap();
    for forbidden in ["/home/", "http://", "https://", "password", "websocket"] {
        assert!(!sample_text.contains(forbidden));
    }
}

#[test]
fn ignored_output_boundary_is_real_and_generated_docs_are_not_an_output() {
    let relative = format!(
        "target/temporal-evaluation/contract-test-{}/run-manifest.json",
        std::process::id()
    );
    let output = workspace_root().join(&relative);
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&output, MANIFEST_BYTES).unwrap();
    assert!(git_status(&["check-ignore", "--quiet", "--", &relative]).success());
    fs::remove_file(&output).unwrap();

    let docs_status = git_status(&["diff", "--quiet", "--", "docs/public/llms-full.txt"]);
    assert!(
        docs_status.success(),
        "benchmark tests must not edit generated docs"
    );

    let readme = String::from_utf8(README_BYTES.to_vec()).unwrap();
    for required in [
        "contract and reproducibility claim only",
        "does not start Chrome",
        "does not claim that Chrome captured any",
        "blocked",
        "inconclusive",
        "unavailable",
        "optional Linux Chromium",
        "target/temporal-evaluation/",
    ] {
        assert!(readme.contains(required), "README omits {required}");
    }
}

#[test]
fn canonical_bytes_never_include_the_output_directory() {
    let sample = sample_manifest();
    let bytes = canonical_json(&sample).unwrap();
    assert!(
        !String::from_utf8(bytes)
            .unwrap()
            .contains("target/temporal-evaluation")
    );
    assert!(Path::new("target/temporal-evaluation").is_relative());
}
