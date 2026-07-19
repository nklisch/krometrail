//! Deterministic Chrome installation discovery.

use krometrail_core::{
    BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
};
use std::{
    env,
    fs::{self, Metadata},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

/// A candidate with its policy source. Keeping this separate from the public installation lets
/// tests inspect ordering without manufacturing an invalid version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryCandidate {
    pub executable: PathBuf,
    pub source: BrowserInstallationSource,
}

/// Inputs to the shared discovery policy. The production helper supplies platform defaults and
/// PATH names; deterministic tests can supply all of them explicitly.
#[derive(Clone, Debug, Default)]
pub struct DiscoveryInputs {
    pub explicit: Option<PathBuf>,
    pub environment_override: Option<PathBuf>,
    pub platform_defaults: Vec<PathBuf>,
    pub path_names: Vec<String>,
    pub path: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug)]
struct VersionProbePolicy {
    cold_candidate_timeout: Duration,
    ordinary_candidate_timeout: Duration,
    output_limit: u64,
}

impl Default for VersionProbePolicy {
    fn default() -> Self {
        Self {
            cold_candidate_timeout: Duration::from_secs(10),
            ordinary_candidate_timeout: Duration::from_secs(2),
            output_limit: 4096,
        }
    }
}

#[derive(Debug)]
enum VersionProbeOutcome {
    Found(BrowserProduct, BrowserProductVersion),
    SpawnFailed,
    TimedOut,
    Rejected,
}

impl VersionProbeOutcome {
    const fn as_str(&self) -> &'static str {
        match self {
            Self::Found(..) => "found",
            Self::SpawnFailed => "spawn_failed",
            Self::TimedOut => "timed_out",
            Self::Rejected => "rejected",
        }
    }
}

const TRANSIENT_SPAWN_RETRIES: usize = 4;

fn spawn_version_probe(path: &Path) -> io::Result<Child> {
    let mut retry = 0;
    loop {
        let child = Command::new(path)
            .arg("--version")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        match child {
            Ok(child) => return Ok(child),
            Err(error)
                if error.kind() == io::ErrorKind::ExecutableFileBusy
                    && retry < TRANSIENT_SPAWN_RETRIES =>
            {
                std::thread::sleep(Duration::from_millis(1 << retry));
                retry += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

pub fn discover_installations(explicit: Option<&Path>) -> Vec<BrowserInstallation> {
    discover_installations_with(DiscoveryInputs {
        explicit: explicit.map(Path::to_path_buf),
        environment_override: environment_override(),
        platform_defaults: platform_defaults(),
        path_names: path_names(),
        path: env::var_os("PATH").map(PathBuf::from),
    })
}

pub fn discover_installations_with(inputs: DiscoveryInputs) -> Vec<BrowserInstallation> {
    discover_installations_with_policy(inputs, VersionProbePolicy::default())
}

fn discover_installations_with_policy(
    inputs: DiscoveryInputs,
    policy: VersionProbePolicy,
) -> Vec<BrowserInstallation> {
    let mut candidates = Vec::new();
    if let Some(path) = inputs.explicit {
        candidates.push(DiscoveryCandidate {
            executable: path,
            source: BrowserInstallationSource::ExplicitRequest,
        });
    }
    if let Some(path) = inputs.environment_override {
        candidates.push(DiscoveryCandidate {
            executable: path,
            source: BrowserInstallationSource::EnvironmentOverride,
        });
    }
    candidates.extend(
        inputs
            .platform_defaults
            .into_iter()
            .map(|executable| DiscoveryCandidate {
                executable,
                source: BrowserInstallationSource::PlatformDefault,
            }),
    );
    if let Some(path) = inputs.path {
        let raw = path.to_string_lossy();
        let separator = if cfg!(windows) { ';' } else { ':' };
        let roots = if raw.contains(separator) {
            raw.split(separator).map(PathBuf::from).collect::<Vec<_>>()
        } else {
            vec![path]
        };
        for root in roots {
            for name in &inputs.path_names {
                candidates.push(DiscoveryCandidate {
                    executable: root.join(name),
                    source: BrowserInstallationSource::PathLookup,
                });
            }
        }
    }

    let mut seen = Vec::new();
    let mut installations = Vec::new();
    let mut attempted = 0_u64;
    for candidate in candidates {
        let Ok(canonical) = canonical_executable(&candidate.executable) else {
            continue;
        };
        if seen.iter().any(|path: &PathBuf| path == &canonical) {
            continue;
        }
        // A failing executable can be reachable through several source classes. Record it before
        // probing so the highest-precedence occurrence owns the single bounded attempt.
        seen.push(canonical.clone());
        attempted = attempted.saturating_add(1);
        let timeout = if candidate.source == BrowserInstallationSource::PathLookup {
            policy.ordinary_candidate_timeout
        } else {
            policy.cold_candidate_timeout
        };
        let started = std::time::Instant::now();
        let outcome = probe_version(&canonical, timeout, policy.output_limit);
        tracing::info!(
            event = "browser.discovery.candidate_probed",
            candidate_ordinal = attempted,
            candidate_source = candidate.source.as_str(),
            probe_outcome = outcome.as_str(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "browser.discovery.candidate_probed"
        );
        let VersionProbeOutcome::Found(product, version) = outcome else {
            continue;
        };
        // Electron is an explicit renderer endpoint, not a platform-discovered managed browser.
        if product == BrowserProduct::ElectronRenderer
            && candidate.source != BrowserInstallationSource::ExplicitRequest
        {
            continue;
        }
        let Ok(installation) =
            BrowserInstallation::new(canonical.clone(), candidate.source, product, version)
        else {
            continue;
        };
        installations.push(installation);
    }
    tracing::info!(
        event = "browser.discovery.completed",
        attempted_count = attempted,
        discovered_count = installations.len(),
        selected_installation_kind = installations
            .first()
            .map(|item| item.product.as_str())
            .unwrap_or("none"),
    );
    installations
}

pub fn platform_defaults() -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        vec![
            PathBuf::from("/usr/bin/google-chrome-stable"),
            PathBuf::from("/usr/bin/google-chrome"),
            PathBuf::from("/usr/bin/chromium"),
            PathBuf::from("/usr/bin/chromium-browser"),
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            PathBuf::from("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            PathBuf::from("/Applications/Chromium.app/Contents/MacOS/Chromium"),
            PathBuf::from(
                "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
            ),
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

pub fn path_names() -> Vec<String> {
    vec![
        "google-chrome-stable".into(),
        "google-chrome".into(),
        "chromium".into(),
        "chromium-browser".into(),
    ]
}

fn environment_override() -> Option<PathBuf> {
    ["KROMETRAIL_CHROME", "CHROME_BIN"]
        .into_iter()
        .find_map(|key| env::var_os(key).map(PathBuf::from))
}

fn canonical_executable(path: &Path) -> Result<PathBuf, ()> {
    let metadata = fs::metadata(path).map_err(|_| ())?;
    if !is_regular_executable(&metadata) {
        return Err(());
    }
    fs::canonicalize(path).map_err(|_| ())
}

fn is_regular_executable(metadata: &Metadata) -> bool {
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn probe_version(path: &Path, timeout: Duration, output_limit: u64) -> VersionProbeOutcome {
    let mut child = match spawn_version_probe(path) {
        Ok(child) => child,
        Err(_) => return VersionProbeOutcome::SpawnFailed,
    };
    // A version probe is an untrusted executable boundary. Read at most 4096 bytes on a helper
    // thread while the caller enforces a hard child deadline. Unlike a temporary capture file,
    // this keeps doctor discovery free of filesystem mutation; a noisy candidate is killed after
    // the bounded wait and cannot grow an in-memory result beyond the cap.
    let stdout = match child.stdout.take() {
        Some(stdout) => stdout,
        None => return VersionProbeOutcome::Rejected,
    };
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(output_limit)
            .read_to_end(&mut bytes)
            .ok()
            .map(|_| bytes)
    });
    let deadline = std::time::Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = reader.join();
                return VersionProbeOutcome::TimedOut;
            }
        }
    };
    let bytes = match reader.join().ok().flatten() {
        Some(bytes) => bytes,
        None => return VersionProbeOutcome::Rejected,
    };
    if !status.success() {
        return VersionProbeOutcome::Rejected;
    }
    let text = String::from_utf8_lossy(&bytes);
    let Some(text) = text.lines().find(|line| !line.trim().is_empty()) else {
        return VersionProbeOutcome::Rejected;
    };
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    let product = if lower.contains("electron") {
        BrowserProduct::ElectronRenderer
    } else if lower.contains("chromium") {
        BrowserProduct::Chromium
    } else if lower.contains("chrome") {
        BrowserProduct::Chrome
    } else {
        BrowserProduct::OtherChromium
    };
    let version = text
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .unwrap_or(text);
    let version = match BrowserProductVersion::new(version.to_owned()) {
        Ok(version) => version,
        Err(_) => {
            return VersionProbeOutcome::Rejected;
        }
    };
    VersionProbeOutcome::Found(product, version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, io::Write};

    fn fixture(root: &Path, name: &str, version: &str) -> PathBuf {
        script_fixture(root, name, &format!("echo '{version}'"))
    }

    fn script_fixture(root: &Path, name: &str, script: &str) -> PathBuf {
        let path = root.join(name);
        let mut file = tempfile::Builder::new()
            .prefix(".krometrail-fixture-")
            .tempfile_in(root)
            .unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "{script}").unwrap();
        file.as_file().sync_all().unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = file.as_file().metadata().unwrap().permissions();
            permissions.set_mode(0o755);
            file.as_file().set_permissions(permissions).unwrap();
        }
        let (staging_file, staging_path) = file.keep().unwrap();
        drop(staging_file);
        fs::rename(staging_path, &path).unwrap();
        path
    }

    #[test]
    fn precedence_deduplicates_canonical_paths_and_classifies_versions() {
        let root = tempfile_root();
        let chrome = fixture(root.path(), "chrome", "Google Chrome 123.4.5");
        let chromium = fixture(root.path(), "chromium", "Chromium 123.4.5");
        let result = discover_installations_with(DiscoveryInputs {
            explicit: Some(chrome.clone()),
            environment_override: Some(chrome.clone()),
            platform_defaults: vec![chromium.clone()],
            path_names: vec!["chrome".into()],
            path: Some(root.path().to_owned()),
        });
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source, BrowserInstallationSource::ExplicitRequest);
        assert_eq!(result[1].product, BrowserProduct::Chromium);
    }

    #[test]
    fn platform_defaults_use_cold_probe_budget_while_path_stays_short() {
        let root = tempfile_root();
        let delayed = script_fixture(
            root.path(),
            "delayed",
            "sleep 0.08\necho 'Google Chrome 123.4.5'",
        );
        let policy = VersionProbePolicy {
            cold_candidate_timeout: Duration::from_secs(1),
            ordinary_candidate_timeout: Duration::from_millis(20),
            output_limit: 4096,
        };
        let platform = discover_installations_with_policy(
            DiscoveryInputs {
                platform_defaults: vec![delayed.clone()],
                ..DiscoveryInputs::default()
            },
            policy,
        );
        assert_eq!(platform.len(), 1);
        let path = discover_installations_with_policy(
            DiscoveryInputs {
                path_names: vec!["delayed".into()],
                path: Some(root.path().to_owned()),
                ..DiscoveryInputs::default()
            },
            policy,
        );
        assert!(path.is_empty());
    }

    #[test]
    fn failing_canonical_candidate_is_probed_once_at_highest_precedence() {
        let root = tempfile_root();
        let counter = root.path().join("counter");
        let candidate = script_fixture(
            root.path(),
            "failing",
            &format!("echo x >> '{}'\nexit 1", counter.display()),
        );
        let result = discover_installations_with_policy(
            DiscoveryInputs {
                explicit: Some(candidate.clone()),
                environment_override: Some(candidate),
                ..DiscoveryInputs::default()
            },
            VersionProbePolicy::default(),
        );
        assert!(result.is_empty());
        assert_eq!(fs::read_to_string(counter).unwrap().lines().count(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn probe_retries_transient_executable_busy() {
        let root = tempfile_root();
        let candidate = fixture(root.path(), "busy", "Google Chrome 123.4.5");
        let writer = OpenOptions::new().write(true).open(&candidate).unwrap();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(5));
            drop(writer);
        });

        let outcome = probe_version(&candidate, Duration::from_secs(1), 4096);

        release.join().unwrap();
        assert!(matches!(
            outcome,
            VersionProbeOutcome::Found(BrowserProduct::Chrome, _)
        ));
    }

    fn tempfile_root() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("krometrail-discovery-")
            .tempdir()
            .unwrap()
    }
}
