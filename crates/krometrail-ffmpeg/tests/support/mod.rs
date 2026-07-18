use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

struct CompiledFixture {
    _directory: tempfile::TempDir,
    executable: PathBuf,
}

static COMPILED_FIXTURE: OnceLock<CompiledFixture> = OnceLock::new();

pub struct FixtureExecutable {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl FixtureExecutable {
    pub fn new(mode: &str) -> Self {
        let compiled = COMPILED_FIXTURE.get_or_init(compile_fixture);
        let directory = tempfile::tempdir().expect("create fake FFmpeg directory");
        let executable = directory.path().join(executable_name());
        std::fs::copy(&compiled.executable, &executable).expect("copy compiled FFmpeg fixture");
        make_executable(&executable);
        std::fs::write(directory.path().join("fixture-mode"), mode)
            .expect("write fake FFmpeg mode");
        Self {
            _directory: directory,
            path: executable,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn compile_fixture() -> CompiledFixture {
    let directory = tempfile::tempdir().expect("create fixture compiler directory");
    let executable = directory.path().join(executable_name());
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let status = Command::new(rustc)
        .arg("--edition=2024")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/fixture_main.rs"))
        .arg("-o")
        .arg(&executable)
        .env(
            "KROMETRAIL_FFMPEG_FIXTURE_MANIFEST_DIR",
            env!("CARGO_MANIFEST_DIR"),
        )
        .status()
        .expect("run rustc for compiled FFmpeg fixture");
    assert!(status.success(), "compiled FFmpeg fixture must build");
    CompiledFixture {
        _directory: directory,
        executable,
    }
}

#[cfg(windows)]
fn executable_name() -> &'static str {
    "ffmpeg.exe"
}

#[cfg(not(windows))]
fn executable_name() -> &'static str {
    "ffmpeg"
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .expect("mark FFmpeg fixture executable");
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}
