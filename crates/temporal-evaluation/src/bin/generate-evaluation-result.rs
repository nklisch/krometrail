use std::{env, fs, path::PathBuf, process};

use temporal_evaluation::{EvaluationResultRecord, sample_evaluation_result};

fn main() {
    let mut args = env::args_os().skip(1);
    let Some(sample_path) = args.next().map(PathBuf::from) else {
        usage_and_exit();
    };
    let Some(schema_path) = args.next().map(PathBuf::from) else {
        usage_and_exit();
    };
    if args.next().is_some() {
        usage_and_exit();
    }

    let result = sample_evaluation_result().unwrap_or_else(|error| fail(error.to_string()));
    let sample_bytes = result
        .canonical_bytes()
        .unwrap_or_else(|error| fail(error.to_string()));
    let mut schema_bytes =
        serde_json::to_vec_pretty(&schemars::schema_for!(EvaluationResultRecord))
            .unwrap_or_else(|error| fail(error.to_string()));
    schema_bytes.push(b'\n');

    write(&sample_path, &sample_bytes);
    write(&schema_path, &schema_bytes);
}

fn write(path: &PathBuf, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| fail(error.to_string()));
    }
    fs::write(path, bytes).unwrap_or_else(|error| fail(error.to_string()));
}

fn usage_and_exit() -> ! {
    eprintln!("usage: generate-evaluation-result <sample.json> <schema.json>");
    process::exit(2);
}

fn fail(message: String) -> ! {
    eprintln!("generate-evaluation-result: {message}");
    process::exit(1);
}
