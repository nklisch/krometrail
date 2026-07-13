#![allow(dead_code)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use krometrail_core::BrowserProduct;

static REAL_BROWSER_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static TEMPORARY_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static WRAPPER_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub fn real_browser_tests_enabled() -> bool {
    env::var("KROMETRAIL_REAL_CHROME_TESTS").as_deref() == Ok("1")
}

pub async fn real_browser_lock() -> tokio::sync::MutexGuard<'static, ()> {
    REAL_BROWSER_LOCK
        .get_or_init(|| tokio::sync::Mutex::new(()))
        .lock()
        .await
}

pub fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/browser/cdp-transport-gate")
}

pub fn fixture_url() -> String {
    let path = fixture_root().join("index.html");
    format!("file://{}", path.display())
}

/// Smoke wrapper flag sets. Both variants force device scale so observations are host-independent;
/// `DefaultDpi` anchors the default band to scale 1, `HighDpi` to scale 2.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromeWrapperVariant {
    DefaultDpi,
    HighDpi,
}

impl ChromeWrapperVariant {
    pub const fn force_device_scale_factor(self) -> f64 {
        match self {
            Self::DefaultDpi => 1.0,
            Self::HighDpi => 2.0,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DefaultDpi => "default_dpi",
            Self::HighDpi => "high_dpi",
        }
    }
}

/// Test-only Chrome launcher wrapper. The existing core `BrowserProduct` is the single product
/// identity used by discovery, runtime compatibility, wrapper selection, and evidence.
/// Linux Chromium is filtered explicitly rather than selected as "first discovered". Production
/// launch is unchanged; this only writes the shell wrapper CI runners require.
pub struct ChromeWrapper {
    pub path: PathBuf,
    pub variant: ChromeWrapperVariant,
    pub executable: PathBuf,
    pub product: BrowserProduct,
}

impl ChromeWrapper {
    /// Select the first discovered installation matching `product`, then write the wrapper.
    /// Returns `None` when no matching installation is discovered (Linux Chromium missing).
    #[cfg(unix)]
    pub fn for_product(product: BrowserProduct, variant: ChromeWrapperVariant) -> Option<Self> {
        let installation = krometrail_cdp::discover_installations(None)
            .into_iter()
            .find(|installation| installation.product == product)?;
        Some(Self::new(installation.executable, product, variant))
    }

    #[cfg(not(unix))]
    pub fn for_product(_product: BrowserProduct, _variant: ChromeWrapperVariant) -> Option<Self> {
        None
    }

    /// Construct from an explicit, already-selected executable. The wrapper script is a pure
    /// function of (executable, variant); discovery is the caller's responsibility.
    #[cfg(unix)]
    pub fn new(
        executable: PathBuf,
        product: BrowserProduct,
        variant: ChromeWrapperVariant,
    ) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let sequence = WRAPPER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "krometrail-real-chrome-wrapper-{}-{sequence}",
            std::process::id()
        ));
        let script = Self::script_bytes(&executable, variant);
        fs::write(&path, script).expect("Chrome wrapper");
        let mut permissions = fs::metadata(&path)
            .expect("Chrome wrapper metadata")
            .permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions).expect("Chrome wrapper permissions");
        Self {
            path,
            variant,
            executable,
            product,
        }
    }

    #[cfg(not(unix))]
    pub fn new(
        _executable: PathBuf,
        _product: BrowserProduct,
        _variant: ChromeWrapperVariant,
    ) -> Self {
        unimplemented!("ChromeWrapper is Unix-only")
    }

    /// Pure function: the wrapper script bytes for (executable, variant), without touching the
    /// filesystem. Used by the deterministic no-Chrome byte test.
    pub fn script_bytes(executable: &Path, variant: ChromeWrapperVariant) -> Vec<u8> {
        let quoted = shell_quote(executable);
        let flags = match variant {
            ChromeWrapperVariant::DefaultDpi => {
                "--headless=new --disable-gpu --no-sandbox --force-device-scale-factor=1"
            }
            ChromeWrapperVariant::HighDpi => {
                "--headless=new --disable-gpu --no-sandbox --high-dpi-support=1 --force-device-scale-factor=2"
            }
        };
        format!("#!/bin/sh\nexec {quoted} {flags} \"$@\"\n").into_bytes()
    }
}

#[cfg(unix)]
fn shell_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

#[cfg(not(unix))]
fn shell_quote(_path: &Path) -> String {
    String::new()
}

impl Drop for ChromeWrapper {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Owns one real-browser test root and removes it only after its users have dropped.
///
/// The guard deliberately removes directories, rather than recursively deleting them. A
/// stale or concurrently reused profile is therefore preserved, and a Chrome command line
/// that still names the root blocks cleanup as an additional ownership check.
pub struct TemporaryRootGuard {
    path: PathBuf,
}

impl TemporaryRootGuard {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryRootGuard {
    fn drop(&mut self) {
        let _ = remove_empty_root_if_unreferenced(&self.path);
    }
}

pub fn temporary_profile_root(name: &str) -> TemporaryRootGuard {
    cleanup_real_browser_roots();
    let sequence = TEMPORARY_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    TemporaryRootGuard {
        path: std::env::temp_dir().join(format!(
            "krometrail-real-{name}-{}-{sequence}",
            std::process::id()
        )),
    }
}

/// Remove only known, empty test roots that no process currently names in its command line.
pub fn cleanup_real_browser_roots() {
    let temporary = std::env::temp_dir();
    let Ok(entries) = fs::read_dir(&temporary) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if (name.starts_with("krometrail-real-managed-")
            || name.starts_with("krometrail-real-targets-")
            || name.starts_with("krometrail-real-reconnect-"))
            && path.is_dir()
        {
            let _ = remove_empty_root_if_unreferenced(&path);
        }
    }
}

fn remove_empty_root_if_unreferenced(path: &Path) -> std::io::Result<bool> {
    if !process_command_references(path).is_empty() {
        return Ok(false);
    }
    remove_empty_directory_tree(path)
}

/// Returns live command lines that still mention a unique test root. Cleanup callers deliberately
/// expose this evidence so a root guard cannot make a leaked browser look like a clean test.
pub fn process_references(path: &Path) -> Vec<String> {
    process_command_references(path)
}

/// Prune empty profile subdirectories without ever deleting a file or following a symlink.
fn remove_empty_directory_tree(path: &Path) -> std::io::Result<bool> {
    let mut empty = true;
    for entry in fs::read_dir(path)?.flatten() {
        let child = entry.path();
        if entry.file_type()?.is_dir() {
            if !remove_empty_directory_tree(&child)? {
                empty = false;
            }
        } else {
            empty = false;
        }
    }
    if empty {
        fs::remove_dir(path).map(|_| true)
    } else {
        Ok(false)
    }
}

#[cfg(target_os = "linux")]
fn process_command_references(path: &Path) -> Vec<String> {
    let needle = path.to_string_lossy();
    let Ok(processes) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    processes
        .flatten()
        .filter_map(|process| {
            let pid = process
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<u32>().ok())?;
            let command = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            let command = String::from_utf8_lossy(&command).replace('\0', " ");
            command
                .contains(needle.as_ref())
                .then_some(format!("pid {pid}: {command}"))
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn process_command_references(path: &Path) -> Vec<String> {
    let needle = path.to_string_lossy();
    let output = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=", "-o", "command="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    parse_ps_command_references(
        String::from_utf8_lossy(&output.stdout).as_ref(),
        needle.as_ref(),
    )
}

#[cfg(target_os = "macos")]
fn parse_ps_command_references(output: &str, needle: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let (pid_part, command) =
                trimmed.split_once(|character: char| character.is_whitespace())?;
            let pid = pid_part.trim().parse::<u32>().ok()?;
            let command = command.trim_start();
            command
                .contains(needle)
                .then_some(format!("pid {pid}: {command}"))
        })
        .collect()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_command_references(_path: &Path) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_removes_only_empty_known_roots() {
        let base = env::temp_dir();
        let suffix = TEMPORARY_ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let empty = base.join(format!("krometrail-real-managed-test-{suffix}"));
        let nonempty = base.join(format!("krometrail-real-targets-test-{suffix}"));
        fs::create_dir(&empty).unwrap();
        fs::create_dir(&nonempty).unwrap();
        fs::write(nonempty.join("owned"), b"keep").unwrap();

        cleanup_real_browser_roots();

        assert!(!empty.exists());
        assert!(nonempty.exists());
        fs::remove_dir_all(nonempty).unwrap();
    }

    #[test]
    fn wrapper_script_bytes_are_pure_and_force_scale() {
        let executable = Path::new("/tmp/sentinel-chrome");
        let default = String::from_utf8(ChromeWrapper::script_bytes(
            executable,
            ChromeWrapperVariant::DefaultDpi,
        ))
        .unwrap();
        assert!(default.contains("--headless=new"));
        assert!(default.contains("--disable-gpu"));
        assert!(default.contains("--no-sandbox"));
        assert!(default.contains("--force-device-scale-factor=1"));
        assert!(!default.contains("--high-dpi-support"));

        let high = String::from_utf8(ChromeWrapper::script_bytes(
            executable,
            ChromeWrapperVariant::HighDpi,
        ))
        .unwrap();
        assert!(high.contains("--headless=new"));
        assert!(high.contains("--disable-gpu"));
        assert!(high.contains("--no-sandbox"));
        assert!(high.contains("--high-dpi-support=1"));
        assert!(high.contains("--force-device-scale-factor=2"));
    }

    #[test]
    fn leak_helper_reports_referenced_root_and_ignores_unreferenced_root() {
        let root = env::temp_dir().join(format!(
            "krometrail-real-reference-test-{}",
            std::process::id()
        ));
        let marker = root.to_string_lossy().to_string();

        // Spawn a shell whose command line contains the marker path. A trivial `sleep 60`
        // would be exec'd and the original script string lost; a small loop keeps `sh` alive
        // with the marker visible in both `/proc/*/cmdline` and `ps` output.
        let mut child = std::process::Command::new("sh")
            .arg("-c")
            .arg(format!("marker={marker}; while true; do sleep 1; done"))
            .spawn()
            .expect("spawn reference process");

        let references = process_references(&root);
        assert!(!references.is_empty(), "referenced root must be reported");
        assert!(references.iter().all(|line| line.contains(&marker)));

        let _ = child.kill();
        let _ = child.wait();

        let unreferenced = env::temp_dir().join(format!(
            "krometrail-real-unreferenced-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(
            process_references(&unreferenced).is_empty(),
            "unreferenced root must not be reported"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_ps_parser_splits_pid_and_command() {
        let output = "  123 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome --user-data-dir=/tmp/krometrail-real-managed-1\n 456 sleep 10\n";
        let references = parse_ps_command_references(output, "/tmp/krometrail-real-managed-1");
        assert_eq!(references.len(), 1);
        assert!(references[0].starts_with("pid 123:"));
        assert!(references[0].contains("Google Chrome"));

        let empty = parse_ps_command_references(output, "/tmp/krometrail-real-managed-2");
        assert!(empty.is_empty());
    }
}
