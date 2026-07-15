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

fn discover_installations() -> Vec<krometrail_core::BrowserInstallation> {
    #[cfg(feature = "qualification-support")]
    {
        crate::discover_installations(None)
    }
    #[cfg(not(feature = "qualification-support"))]
    {
        krometrail_cdp::discover_installations(None)
    }
}

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

pub fn page_observation_fixture_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/browser/page-observation/index.html");
    format!("file://{}", path.display())
}

pub fn verified_interactions_fixture_url() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/browser/verified-interactions/index.html");
    format!("file://{}", path.display())
}

pub fn page_lifecycle_fixture_url(page: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/browser/page-lifecycle")
        .join(page);
    format!("file://{}", path.display())
}

pub fn waits_and_batches_fixture_url(page: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/browser/waits-and-batches")
        .join(page);
    format!("file://{}", path.display())
}

/// Fixed dimensions used by the live qualification profile. CSS pixels and device scale are
/// kept together so a wrapper cannot accidentally request the right size at the wrong scale.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ChromeViewport {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor_milli: u16,
}

impl ChromeViewport {
    pub const LIVE: Self = Self {
        width: 800,
        height: 450,
        device_scale_factor_milli: 1_000,
    };

    pub const fn scale_factor(self) -> f64 {
        self.device_scale_factor_milli as f64 / 1_000.0
    }
}

/// Existing smoke wrapper variants. Their old constructors and flag sets remain unchanged;
/// viewport-aware construction is an additive qualification-only path.
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

/// Test-only Chrome launcher wrapper. It owns only the generated shell wrapper; the browser
/// process and managed profile remain owned by the existing launcher/session authorities.
pub struct ChromeWrapper {
    pub path: PathBuf,
    pub variant: ChromeWrapperVariant,
    pub executable: PathBuf,
    pub product: BrowserProduct,
    pub viewport: Option<ChromeViewport>,
}

impl ChromeWrapper {
    #[cfg(unix)]
    pub fn for_product(product: BrowserProduct, variant: ChromeWrapperVariant) -> Option<Self> {
        let installation = discover_installations()
            .into_iter()
            .find(|installation| installation.product == product)?;
        Some(Self::new(installation.executable, product, variant))
    }

    #[cfg(not(unix))]
    pub fn for_product(_product: BrowserProduct, _variant: ChromeWrapperVariant) -> Option<Self> {
        None
    }

    #[cfg(unix)]
    pub fn for_product_with_viewport(
        product: BrowserProduct,
        viewport: ChromeViewport,
    ) -> Option<Self> {
        let installation = discover_installations()
            .into_iter()
            .find(|installation| installation.product == product)?;
        Some(Self::new_with_viewport(
            installation.executable,
            product,
            ChromeWrapperVariant::DefaultDpi,
            viewport,
        ))
    }

    #[cfg(not(unix))]
    pub fn for_product_with_viewport(
        _product: BrowserProduct,
        _viewport: ChromeViewport,
    ) -> Option<Self> {
        None
    }

    #[cfg(unix)]
    pub fn new(
        executable: PathBuf,
        product: BrowserProduct,
        variant: ChromeWrapperVariant,
    ) -> Self {
        Self::write(executable, product, variant, None)
    }

    #[cfg(not(unix))]
    pub fn new(
        _executable: PathBuf,
        _product: BrowserProduct,
        _variant: ChromeWrapperVariant,
    ) -> Self {
        panic!("ChromeWrapper is Unix-only")
    }

    #[cfg(unix)]
    pub fn new_with_viewport(
        executable: PathBuf,
        product: BrowserProduct,
        variant: ChromeWrapperVariant,
        viewport: ChromeViewport,
    ) -> Self {
        Self::write(executable, product, variant, Some(viewport))
    }

    #[cfg(not(unix))]
    pub fn new_with_viewport(
        _executable: PathBuf,
        _product: BrowserProduct,
        _variant: ChromeWrapperVariant,
        _viewport: ChromeViewport,
    ) -> Self {
        panic!("ChromeWrapper is Unix-only")
    }

    #[cfg(unix)]
    fn write(
        executable: PathBuf,
        product: BrowserProduct,
        variant: ChromeWrapperVariant,
        viewport: Option<ChromeViewport>,
    ) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let sequence = WRAPPER_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = env::temp_dir().join(format!(
            "krometrail-real-chrome-wrapper-{}-{sequence}",
            std::process::id()
        ));
        let script = match viewport {
            Some(viewport) => Self::script_bytes_with_viewport(&executable, variant, viewport),
            None => Self::script_bytes(&executable, variant),
        };
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
            viewport,
        }
    }

    /// Pure function used by existing smoke tests. Keep its bytes stable.
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

    /// Pure qualification wrapper construction. The requested viewport is explicit and does not
    /// alter the legacy smoke wrapper's flag set.
    pub fn script_bytes_with_viewport(
        executable: &Path,
        variant: ChromeWrapperVariant,
        viewport: ChromeViewport,
    ) -> Vec<u8> {
        let mut script = Self::script_bytes(executable, variant);
        let text = String::from_utf8(script).expect("wrapper script is UTF-8");
        let flags = format!("--window-size={},{}", viewport.width, viewport.height);
        let text = text.replacen(" \"$@\"", &format!(" {flags} \"$@\""), 1);
        script = text.into_bytes();
        script
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
        path: env::temp_dir().join(format!(
            "krometrail-real-{name}-{}-{sequence}",
            std::process::id()
        )),
    }
}

pub fn cleanup_real_browser_roots() {
    let Ok(entries) = fs::read_dir(env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let known = [
            "krometrail-real-managed-",
            "krometrail-real-targets-",
            "krometrail-real-reconnect-",
            "krometrail-real-page-observation-",
            "krometrail-real-page-lifecycle-",
            "krometrail-real-verified-interactions-",
            "krometrail-real-waits-and-batches-",
        ];
        if known.iter().any(|prefix| name.starts_with(prefix)) && path.is_dir() {
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

pub fn process_references(path: &Path) -> Vec<String> {
    process_command_references(path)
}

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
    let output = std::process::Command::new("ps")
        .args(["-ax", "-o", "pid=", "-o", "command="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    let needle = path.to_string_lossy();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| line.contains(needle.as_ref()))
        .map(str::to_owned)
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
    fn qualification_wrapper_adds_only_the_requested_viewport() {
        let old = String::from_utf8(ChromeWrapper::script_bytes(
            Path::new("/tmp/chrome"),
            ChromeWrapperVariant::DefaultDpi,
        ))
        .unwrap();
        let live = String::from_utf8(ChromeWrapper::script_bytes_with_viewport(
            Path::new("/tmp/chrome"),
            ChromeWrapperVariant::DefaultDpi,
            ChromeViewport::LIVE,
        ))
        .unwrap();
        assert!(!old.contains("--window-size"));
        assert!(old.contains("--force-device-scale-factor=1"));
        assert!(live.contains("--window-size=800,450"));
        assert!(live.contains("--force-device-scale-factor=1"));
    }

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
}
