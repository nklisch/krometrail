use std::{env, fs, path::PathBuf, process};

use temporal_evaluation::{run_manifest_schema, sample_manifest};

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

    let manifest = sample_manifest();
    let sample_bytes = manifest
        .canonical_bytes()
        .unwrap_or_else(|error| fail(error.to_string()));
    let mut schema_bytes = serde_json::to_vec_pretty(&run_manifest_schema())
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
    eprintln!("usage: generate-run-manifest <sample.json> <schema.json>");
    process::exit(2);
}

fn fail(message: String) -> ! {
    eprintln!("generate-run-manifest: {message}");
    process::exit(1);
}
