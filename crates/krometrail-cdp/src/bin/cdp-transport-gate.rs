#[cfg(feature = "cdp-spike-cdpkit")]
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(feature = "cdp-spike-cdpkit")]
use krometrail_cdp::spike::{
    cdpkit_adapter::CdpkitTransportFactory,
    chrome_harness::{failure_evidence, run_real_chrome_gate},
    evidence::{GateConfiguration, sanitize_evidence, validate_evidence},
    write_json_schema,
};

#[cfg(feature = "cdp-spike-cdpkit")]
#[derive(Debug)]
struct GateCli {
    chrome_binary: PathBuf,
    output: PathBuf,
    minimum_seconds: f64,
    minimum_frames: u64,
    hard_stop_seconds: u64,
}

#[cfg(feature = "cdp-spike-cdpkit")]
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

#[cfg(feature = "cdp-spike-cdpkit")]
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
        "gate" => {
            let cli = parse_gate(remaining)?;
            let configuration = GateConfiguration {
                minimum_seconds: cli.minimum_seconds,
                minimum_frames: cli.minimum_frames,
                saturation_seconds: 10.0,
                saturation_attempts: 100,
            };
            let factory = CdpkitTransportFactory::new();
            let result =
                run_real_chrome_gate(&factory, configuration.clone(), &cli.chrome_binary).await;
            match result {
                Ok(evidence) => {
                    validate_evidence(&evidence).map_err(|error| error.to_string())?;
                    write_json(&cli.output, &evidence)?;
                }
                Err(error) => {
                    // A failed candidate still emits a complete, schema-valid report. The exact
                    // error is retained in every represented gate; no threshold is waived.
                    let evidence = failure_evidence(&factory, configuration, &error);
                    validate_evidence(&evidence).map_err(|validation| validation.to_string())?;
                    write_json(&cli.output, &evidence)?;
                    return Err(error.to_string());
                }
            }
            let _ = cli.hard_stop_seconds;
        }
        other => return Err(format!("unknown command {other}")),
    }
    Ok(())
}

#[cfg(feature = "cdp-spike-cdpkit")]
fn parse_gate(args: Vec<String>) -> Result<GateCli, String> {
    let mut chrome_binary = None;
    let mut output = None;
    let mut minimum_seconds = 60.0;
    let mut minimum_frames = 1000;
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
            "--hard-stop-seconds" => {
                hard_stop_seconds = value
                    .parse()
                    .map_err(|_| "invalid --hard-stop-seconds".to_owned())?
            }
            other => return Err(format!("unknown flag {other}")),
        }
        index += 2;
    }
    Ok(GateCli {
        chrome_binary: chrome_binary.ok_or("--chrome-binary is required")?,
        output: output.ok_or("--output is required")?,
        minimum_seconds,
        minimum_frames,
        hard_stop_seconds,
    })
}

#[cfg(feature = "cdp-spike-cdpkit")]
fn required_path(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<PathBuf, String> {
    while let Some(value) = args.next() {
        if value == flag {
            return args
                .next()
                .map(PathBuf::from)
                .ok_or_else(|| format!("missing value for {flag}"));
        }
    }
    Err(format!("{flag} is required"))
}

#[cfg(feature = "cdp-spike-cdpkit")]
fn write_json(path: &std::path::Path, value: &impl serde::Serialize) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    std::fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(not(feature = "cdp-spike-cdpkit"))]
fn main() -> ExitCode {
    eprintln!("cdp-transport-gate requires the cdp-spike-cdpkit feature");
    ExitCode::FAILURE
}
