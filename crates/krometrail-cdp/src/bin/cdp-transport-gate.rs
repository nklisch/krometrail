#[cfg(feature = "cdp-spike")]
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(feature = "cdp-spike-cdpkit")]
use krometrail_cdp::spike::{
    cdpkit_adapter::CdpkitTransportFactory,
    chrome_harness::{failure_evidence, run_real_chrome_gate},
};

#[cfg(feature = "cdp-spike")]
use krometrail_cdp::spike::{
    decide_from_files,
    evidence::{sanitize_evidence, validate_decisive_report},
    write_json_schema,
};

#[cfg(feature = "cdp-spike-cdpkit")]
use krometrail_cdp::spike::evidence::{GateConfiguration, validate_evidence};

#[cfg(feature = "cdp-spike-cdpkit")]
#[derive(Debug)]
struct GateCli {
    chrome_binary: PathBuf,
    output: PathBuf,
    expected_git_revision: String,
    minimum_seconds: f64,
    minimum_frames: u64,
    saturation_seconds: f64,
    saturation_attempts: u64,
    hard_stop_seconds: u64,
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
            let mut args = remaining.into_iter();
            let output = required_path(&mut args, "--output")?;
            write_json_schema(&output).map_err(|error| error.to_string())?;
        }
        "validate-and-normalize" => {
            let mut args = remaining.into_iter();
            let input = required_path(&mut args, "--input")?;
            let output = required_path(&mut args, "--output")?;
            let bytes = std::fs::read(&input).map_err(|error| error.to_string())?;
            let evidence = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            let evidence = sanitize_evidence(evidence).map_err(|error| error.to_string())?;
            write_json(&output, &evidence)?;
        }
        "decide" => {
            let mut args = remaining.into_iter();
            let linux = required_path(&mut args, "--linux-report")?;
            let macos = required_path(&mut args, "--macos-report")?;
            let output = required_path(&mut args, "--output")?;
            let decision = decide_from_files(&linux, &macos).map_err(|error| error.to_string())?;
            write_json(&output, &decision)?;
        }
        "validate-decisive" => {
            let mut args = remaining.into_iter();
            let input = required_path(&mut args, "--input")?;
            let platform = required_value(&mut args, "--platform")?;
            let expected_revision = required_value(&mut args, "--expected-git-revision")?;
            let bytes = std::fs::read(&input).map_err(|error| error.to_string())?;
            let evidence = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            validate_decisive_report(&evidence, &platform).map_err(|error| error.to_string())?;
            if evidence.source.git_revision != expected_revision {
                return Err(format!(
                    "report uses {}, expected {}",
                    evidence.source.git_revision, expected_revision
                ));
            }
        }
        "gate" => {
            #[cfg(feature = "cdp-spike-cdpkit")]
            {
                let cli = parse_gate(remaining)?;
                let configuration = GateConfiguration {
                    minimum_seconds: cli.minimum_seconds,
                    minimum_frames: cli.minimum_frames,
                    saturation_seconds: cli.saturation_seconds,
                    saturation_attempts: cli.saturation_attempts,
                    hard_stop_seconds: cli.hard_stop_seconds,
                };
                let factory = CdpkitTransportFactory::new();
                let result =
                    run_real_chrome_gate(&factory, configuration.clone(), &cli.chrome_binary).await;
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
                        let evidence = failure_evidence(&factory, configuration, &error);
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
    Ok(GateCli {
        chrome_binary: chrome_binary.ok_or("--chrome-binary is required")?,
        output: output.ok_or("--output is required")?,
        expected_git_revision: expected_git_revision
            .ok_or("--expected-git-revision is required")?,
        minimum_seconds,
        minimum_frames,
        saturation_seconds,
        saturation_attempts,
        hard_stop_seconds,
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

        assert_eq!(gate.minimum_seconds, 60.0);
        assert_eq!(gate.minimum_frames, 1_000);
        assert_eq!(gate.saturation_seconds, 10.0);
        assert_eq!(gate.saturation_attempts, 100);
        assert_eq!(gate.hard_stop_seconds, 120);
        assert_eq!(gate.expected_git_revision.len(), 40);
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
