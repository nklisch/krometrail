#[cfg(feature = "cdp-spike")]
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[cfg(feature = "cdp-spike")]
use serde::Deserialize;

#[cfg(feature = "cdp-spike-cdpkit")]
use krometrail_cdp::spike::{
    cdpkit_adapter::CdpkitTransportFactory,
    chrome_harness::{failure_evidence, run_real_chrome_gate},
};

#[cfg(feature = "cdp-spike")]
use krometrail_cdp::spike::{
    decide_from_files_at,
    evidence::{
        attest_relevant_source_at, is_git_revision, resolve_repository_root, sanitize_evidence,
        validate_decisive_report_at,
    },
    write_json_schema,
};

#[cfg(feature = "cdp-spike-cdpkit")]
use krometrail_cdp::spike::evidence::{GateConfiguration, validate_evidence};

#[cfg(feature = "cdp-spike-cdpkit")]
#[derive(Debug)]
struct GateCli {
    chrome_binary: PathBuf,
    output: PathBuf,
    repo_root: Option<PathBuf>,
    expected_git_revision: String,
    configuration: GateConfiguration,
}

#[cfg(feature = "cdp-spike")]
#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cdp transport gate: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(feature = "cdp-spike")]
async fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    let (command, remaining) = match first {
        Some(value) if value.starts_with('-') => (
            "gate".to_owned(),
            std::iter::once(value).chain(args).collect(),
        ),
        Some(value) => (value, args.collect()),
        None => ("gate".to_owned(), Vec::new()),
    };
    match command.as_str() {
        "schema" => {
            let mut args = remaining.iter().cloned();
            let output = required_path(&mut args, "--output")?;
            write_json_schema(&output).map_err(|error| error.to_string())?;
        }
        "canonical-config" => {
            let mut args = remaining.iter().cloned();
            let output = required_path(&mut args, "--output")?;
            write_json(&output, &canonical_config_document())?;
        }
        "verify-canonical-config" => {
            let mut args = remaining.iter().cloned();
            let input = required_path(&mut args, "--input")?;
            verify_canonical_config(&input)?;
        }
        "validate-and-normalize" => {
            let mut args = remaining.iter().cloned();
            let input = required_path(&mut args, "--input")?;
            let output = required_path(&mut args, "--output")?;
            let bytes = std::fs::read(&input).map_err(|error| error.to_string())?;
            let evidence = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            let evidence = sanitize_evidence(evidence).map_err(|error| error.to_string())?;
            write_json(&output, &evidence)?;
        }
        "decide" => {
            let mut args = remaining.iter().cloned();
            let linux = required_path(&mut args, "--linux-report")?;
            let macos = required_path(&mut args, "--macos-report")?;
            let output = required_path(&mut args, "--output")?;
            let repo_root = optional_path(&remaining, "--repo-root")?;
            let repo_root =
                resolve_repository_root(repo_root.as_deref()).map_err(|error| error.to_string())?;
            let decision = decide_from_files_at(&linux, &macos, &repo_root)
                .map_err(|error| error.to_string())?;
            write_json(&output, &decision)?;
        }
        "validate-decisive" => {
            let mut args = remaining.iter().cloned();
            let input = required_path(&mut args, "--input")?;
            let platform = required_value(&mut args, "--platform")?;
            let expected_revision = required_value(&mut args, "--expected-git-revision")?;
            let repo_root = optional_path(&remaining, "--repo-root")?;
            let repo_root =
                resolve_repository_root(repo_root.as_deref()).map_err(|error| error.to_string())?;
            let bytes = std::fs::read(&input).map_err(|error| error.to_string())?;
            let evidence = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            if !is_git_revision(&expected_revision) {
                return Err(
                    "--expected-git-revision must be exactly 40 lowercase hexadecimal characters"
                        .into(),
                );
            }
            validate_decisive_report_at(&evidence, &platform, &repo_root)
                .map_err(|error| error.to_string())?;
            if evidence.source.git_revision != expected_revision {
                return Err(format!(
                    "report uses {}, expected {}",
                    evidence.source.git_revision, expected_revision
                ));
            }
        }
        "attest" => {
            let mut args = remaining.iter().cloned();
            let expected_revision = required_value(&mut args, "--expected-git-revision")?;
            if !is_git_revision(&expected_revision) {
                return Err(
                    "--expected-git-revision must be exactly 40 lowercase hexadecimal characters"
                        .into(),
                );
            }
            let repo_root = optional_path(&remaining, "--repo-root")?;
            let repo_root =
                resolve_repository_root(repo_root.as_deref()).map_err(|error| error.to_string())?;
            let output = required_path(&mut args, "--output")?;
            let attestation = attest_relevant_source_at(&repo_root, &expected_revision)
                .map_err(|error| error.to_string())?;
            write_json(&output, &attestation)?;
        }
        "gate" => {
            #[cfg(feature = "cdp-spike-cdpkit")]
            {
                let cli = parse_gate(remaining)?;
                let repo_root = resolve_repository_root(cli.repo_root.as_deref())
                    .map_err(|error| error.to_string())?;
                let configuration = cli.configuration.clone();
                let factory = CdpkitTransportFactory::new();
                let result = run_real_chrome_gate(
                    &factory,
                    configuration.clone(),
                    &cli.chrome_binary,
                    &cli.expected_git_revision,
                    &repo_root,
                )
                .await;
                match result {
                    Ok(evidence) => {
                        validate_evidence(&evidence).map_err(|error| error.to_string())?;
                        if evidence.source.git_revision != cli.expected_git_revision {
                            return Err(format!(
                                "gate ran at {}, expected {}",
                                evidence.source.git_revision, cli.expected_git_revision
                            ));
                        }
                        write_json(&cli.output, &evidence)?;
                    }
                    Err(error) => {
                        // A failed candidate still emits a complete, schema-valid report. The exact
                        // error is retained in every represented gate; no threshold is waived.
                        let evidence = failure_evidence(
                            &factory,
                            configuration,
                            &cli.expected_git_revision,
                            &repo_root,
                            &error,
                        );
                        validate_evidence(&evidence)
                            .map_err(|validation| validation.to_string())?;
                        write_json(&cli.output, &evidence)?;
                        return Err(error.to_string());
                    }
                }
            }
            #[cfg(not(feature = "cdp-spike-cdpkit"))]
            {
                return Err("gate requires the cdp-spike-cdpkit feature".into());
            }
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}

#[cfg(feature = "cdp-spike-cdpkit")]
fn parse_gate(args: Vec<String>) -> Result<GateCli, String> {
    let mut chrome_binary = None;
    let mut output = None;
    let mut repo_root = None;
    let mut expected_git_revision = None;
    let mut minimum_seconds = 60.0;
    let mut minimum_frames = 1000;
    let mut saturation_seconds = 10.0;
    let mut saturation_attempts = 100;
    let mut hard_stop_seconds = 120;
    let mut index = 0;
    while index < args.len() {
        let flag = &args[index];
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--chrome-binary" => chrome_binary = Some(PathBuf::from(value)),
            "--output" => output = Some(PathBuf::from(value)),
            "--repo-root" => repo_root = Some(PathBuf::from(value)),
            "--expected-git-revision" => expected_git_revision = Some(value.to_owned()),
            "--minimum-seconds" => {
                minimum_seconds = value
                    .parse()
                    .map_err(|_| "invalid --minimum-seconds".to_owned())?
            }
            "--minimum-frames" => {
                minimum_frames = value
                    .parse()
                    .map_err(|_| "invalid --minimum-frames".to_owned())?
            }
            "--saturation-seconds" => {
                saturation_seconds = value
                    .parse()
                    .map_err(|_| "invalid --saturation-seconds".to_owned())?
            }
            "--saturation-attempts" => {
                saturation_attempts = value
                    .parse()
                    .map_err(|_| "invalid --saturation-attempts".to_owned())?
            }
            "--hard-stop-seconds" => {
                hard_stop_seconds = value
                    .parse()
                    .map_err(|_| "invalid --hard-stop-seconds".to_owned())?;
                if hard_stop_seconds == 0 {
                    return Err("--hard-stop-seconds must be positive".into());
                }
            }
            other => return Err(format!("unknown flag {other}")),
        }
        index += 2;
    }
    let expected_git_revision =
        expected_git_revision.ok_or("--expected-git-revision is required")?;
    if !is_git_revision(&expected_git_revision) {
        return Err(
            "--expected-git-revision must be exactly 40 lowercase hexadecimal characters".into(),
        );
    }
    let configuration = GateConfiguration {
        minimum_seconds,
        minimum_frames,
        saturation_seconds,
        saturation_attempts,
        hard_stop_seconds,
    };
    if configuration != krometrail_cdp::spike::canonical_decisive_configuration() {
        return Err(
            "gate configuration must exactly match the canonical decisive 60/1000/10/100/120 contract"
                .into(),
        );
    }
    Ok(GateCli {
        chrome_binary: chrome_binary.ok_or("--chrome-binary is required")?,
        output: output.ok_or("--output is required")?,
        repo_root,
        expected_git_revision,
        configuration,
    })
}

#[cfg(all(test, feature = "cdp-spike-cdpkit"))]
mod tests {
    use super::parse_gate;

    #[test]
    fn parses_the_complete_shared_gate_configuration() {
        let gate = parse_gate(
            [
                "--chrome-binary",
                "/chrome",
                "--output",
                "report.json",
                "--expected-git-revision",
                "0123456789abcdef0123456789abcdef01234567",
                "--minimum-seconds",
                "60",
                "--minimum-frames",
                "1000",
                "--saturation-seconds",
                "10",
                "--saturation-attempts",
                "100",
                "--hard-stop-seconds",
                "120",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        )
        .expect("complete gate configuration");

        assert_eq!(gate.configuration.minimum_seconds, 60.0);
        assert_eq!(gate.configuration.minimum_frames, 1_000);
        assert_eq!(gate.configuration.saturation_seconds, 10.0);
        assert_eq!(gate.configuration.saturation_attempts, 100);
        assert_eq!(gate.configuration.hard_stop_seconds, 120);
        assert_eq!(gate.expected_git_revision.len(), 40);
    }

    #[test]
    fn rejects_a_noncanonical_hard_stop_even_when_other_values_match() {
        let mut arguments = vec![
            "--chrome-binary",
            "/chrome",
            "--output",
            "report.json",
            "--expected-git-revision",
            "0123456789abcdef0123456789abcdef01234567",
            "--hard-stop-seconds",
            "999999",
        ];
        let error = parse_gate(arguments.drain(..).map(str::to_owned).collect())
            .expect_err("noncanonical hard stop must be rejected");
        assert!(error.contains("canonical decisive"));
    }

    #[test]
    fn requires_the_expected_revision_for_exact_sha_runs() {
        let error = parse_gate(
            ["--chrome-binary", "/chrome", "--output", "report.json"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
        .expect_err("exact gate runs must bind an expected revision");
        assert!(error.contains("--expected-git-revision"));
    }

    fn temporary_json(contents: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "krometrail-cdp-canonical-config-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock is after unix epoch")
                .as_nanos()
        ));
        std::fs::write(&path, contents).expect("write canonical config fixture");
        path
    }

    #[test]
    fn canonical_config_verification_ignores_pretty_key_order() {
        let generated = serde_json::to_value(super::canonical_config_document()).unwrap();
        let configuration = &generated["configuration"];
        let digest = serde_json::to_string(&generated["configuration_sha256"]).unwrap();
        let reordered = format!(
            "{{\"configuration\":{{\"hard_stop_seconds\":{},\"saturation_attempts\":{},\"saturation_seconds\":{},\"minimum_frames\":{},\"minimum_seconds\":{}}},\"configuration_sha256\":{}}}",
            configuration["hard_stop_seconds"],
            configuration["saturation_attempts"],
            configuration["saturation_seconds"],
            configuration["minimum_frames"],
            configuration["minimum_seconds"],
            digest,
        );
        let path = temporary_json(&reordered);
        let result = super::verify_canonical_config(&path);
        std::fs::remove_file(path).expect("remove canonical config fixture");
        result.expect("pretty JSON key reordering must not change the digest");
    }

    #[test]
    fn canonical_config_verification_rejects_a_real_configuration_mutation() {
        let mut generated = serde_json::to_value(super::canonical_config_document()).unwrap();
        generated["configuration"]["hard_stop_seconds"] = serde_json::json!(121);
        let path = temporary_json(&serde_json::to_string_pretty(&generated).unwrap());
        let error = super::verify_canonical_config(&path)
            .expect_err("a mutated canonical configuration must fail closed");
        std::fs::remove_file(path).expect("remove canonical config fixture");
        assert!(error.contains("canonical"));
    }
}

#[cfg(feature = "cdp-spike")]
fn canonical_config_document() -> serde_json::Value {
    serde_json::json!({
        "configuration": krometrail_cdp::spike::canonical_decisive_configuration(),
        "configuration_sha256": krometrail_cdp::spike::canonical_decisive_configuration_digest(),
    })
}

#[cfg(feature = "cdp-spike")]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CanonicalConfigDocument {
    configuration: krometrail_cdp::spike::GateConfiguration,
    configuration_sha256: String,
}

#[cfg(feature = "cdp-spike")]
fn verify_canonical_config(path: &Path) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    let document: CanonicalConfigDocument =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    let canonical = krometrail_cdp::spike::canonical_decisive_configuration();
    if document.configuration != canonical {
        return Err("canonical configuration values do not match Rust contract".into());
    }
    let computed = krometrail_cdp::spike::configuration_digest(&document.configuration);
    let expected = krometrail_cdp::spike::canonical_decisive_configuration_digest();
    if document.configuration_sha256 != expected || computed != expected {
        return Err("canonical configuration digest does not match Rust contract".into());
    }
    Ok(())
}

#[cfg(feature = "cdp-spike")]
fn required_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    while let Some(value) = args.next() {
        if value == flag {
            return args
                .next()
                .ok_or_else(|| format!("missing value for {flag}"));
        }
    }
    Err(format!("{flag} is required"))
}

#[cfg(feature = "cdp-spike")]
fn required_path(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<PathBuf, String> {
    required_value(args, flag).map(PathBuf::from)
}

#[cfg(feature = "cdp-spike")]
fn optional_path(args: &[String], flag: &str) -> Result<Option<PathBuf>, String> {
    let Some(index) = args.iter().position(|value| value == flag) else {
        return Ok(None);
    };
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("missing value for {flag}"))?;
    Ok(Some(PathBuf::from(value)))
}

#[cfg(feature = "cdp-spike")]
fn write_json(path: &std::path::Path, value: &impl serde::Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(not(feature = "cdp-spike"))]
fn main() -> ExitCode {
    eprintln!("cdp-transport-gate requires the cdp-spike feature");
    ExitCode::FAILURE
}
