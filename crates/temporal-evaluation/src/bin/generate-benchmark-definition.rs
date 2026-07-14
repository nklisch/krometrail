use std::{env, fs, path::PathBuf, process};

use temporal_evaluation::{BenchmarkDefinition, benchmark_definition_schema};

fn main() {
    let mut args = env::args_os().skip(1);
    let Some(definition_path) = args.next().map(PathBuf::from) else {
        usage_and_exit();
    };
    let Some(schema_path) = args.next().map(PathBuf::from) else {
        usage_and_exit();
    };
    if args.next().is_some() {
        usage_and_exit();
    }

    let definition = BenchmarkDefinition::canonical();
    let definition_bytes = definition
        .canonical_bytes()
        .unwrap_or_else(|error| fail(error.to_string()));
    let schema = serde_json::to_vec_pretty(&benchmark_definition_schema())
        .unwrap_or_else(|error| fail(error.to_string()));

    write(&definition_path, &definition_bytes);
    let mut schema_bytes = schema;
    schema_bytes.push(b'\n');
    write(&schema_path, &schema_bytes);
}

fn write(path: &PathBuf, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap_or_else(|error| fail(error.to_string()));
    }
    fs::write(path, bytes).unwrap_or_else(|error| fail(error.to_string()));
}

fn usage_and_exit() -> ! {
    eprintln!("usage: generate-benchmark-definition <definition.json> <schema.json>");
    process::exit(2);
}

fn fail(message: String) -> ! {
    eprintln!("generate-benchmark-definition: {message}");
    process::exit(1);
}
