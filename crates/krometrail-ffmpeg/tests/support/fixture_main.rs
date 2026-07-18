use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    process::Command,
    thread,
    time::Duration,
};

const VALID_MP4: &[u8] = include_bytes!(concat!(
    env!("KROMETRAIL_FFMPEG_FIXTURE_MANIFEST_DIR"),
    "/tests/fixtures/video/valid-h264.mp4"
));

fn main() {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    if arguments.first().is_some_and(|value| value == "--fixture-child") {
        thread::sleep(Duration::from_secs(60));
        return;
    }
    let executable = std::env::current_exe().expect("fixture executable path");
    let directory = executable.parent().expect("fixture executable directory");
    let mode = fs::read_to_string(directory.join("fixture-mode"))
        .unwrap_or_else(|_| "valid".to_owned());

    if arguments.iter().any(|value| value == "-version") {
        if mode.trim() == "version-overflow" {
            io::stdout().write_all(&vec![b'v'; 70 * 1024]).unwrap();
        } else if mode.trim() == "valid-version2" {
            println!("ffmpeg version fixture-2");
        } else {
            println!("ffmpeg version fixture-1");
        }
        return;
    }

    let encode_number = increment_encode_count(directory);
    let effective_mode = if mode.trim() == "valid-version2"
        || encode_number == 1 && mode.trim().ends_with("_after_qualification")
    {
        "valid"
    } else {
        mode.trim().trim_end_matches("_after_qualification")
    };
    if !has_exact_policy_shape(&arguments) {
        std::process::exit(90);
    }
    let output = PathBuf::from(arguments.last().expect("fixed output argument"));
    match effective_mode {
        "valid" => fs::write(output, VALID_MP4).unwrap(),
        "invalid" => fs::write(output, b"not an mp4").unwrap(),
        "exit" => std::process::exit(17),
        "stderr-overflow" => {
            io::stderr().write_all(&vec![b'e'; 70 * 1024]).unwrap();
            thread::sleep(Duration::from_secs(60));
        }
        "output-overflow" => {
            let file = fs::File::create(output).unwrap();
            file.set_len(2 * 1024 * 1024).unwrap();
            thread::sleep(Duration::from_secs(60));
        }
        "hang" => thread::sleep(Duration::from_secs(60)),
        "descendant" => {
            let _child = Command::new(executable)
                .arg("--fixture-child")
                .spawn()
                .expect("spawn compiled descendant");
            thread::sleep(Duration::from_secs(60));
        }
        _ => std::process::exit(91),
    }
}

fn increment_encode_count(directory: &std::path::Path) -> u64 {
    let path = directory.join("encode-count");
    let current = fs::read_to_string(&path)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    let next = current + 1;
    fs::write(path, next.to_string()).unwrap();
    next
}

fn has_exact_policy_shape(arguments: &[std::ffi::OsString]) -> bool {
    let values: Vec<_> = arguments
        .iter()
        .map(|value| value.to_string_lossy())
        .collect();
    values.first().is_some_and(|value| value == "-nostdin")
        && values.last().is_some_and(|value| value == "output.partial.mp4")
        && values.windows(2).any(|pair| pair == ["-c:v", "libx264"])
        && values.windows(2).any(|pair| pair == ["-safe", "1"])
        && values.windows(2).any(|pair| pair == ["-an", "-sn"])
        && !values.iter().any(|value| value == "sh" || value == "cmd.exe")
}
