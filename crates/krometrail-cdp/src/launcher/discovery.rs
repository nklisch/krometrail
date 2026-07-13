//! Deterministic Chrome installation discovery.

use krometrail_core::{
    BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
};
use std::{
    env,
    fs::{self, Metadata},
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
    for candidate in candidates {
        let Ok(canonical) = canonical_executable(&candidate.executable) else {
            continue;
        };
        if seen.iter().any(|path: &PathBuf| path == &canonical) {
            continue;
        }
        let Some((product, version)) = probe_version(&canonical) else {
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
        seen.push(canonical);
        installations.push(installation);
    }
    tracing::info!(
        event = "browser.discovery.completed",
        candidate_count = installations.len(),
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

fn probe_version(path: &Path) -> Option<(BrowserProduct, BrowserProductVersion)> {
    let mut child = match Command::new(path)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return None,
    };
    // A version probe is an untrusted executable boundary. Read at most 4096 bytes on a helper
    // thread while the caller enforces a hard child deadline. Unlike a temporary capture file,
    // this keeps doctor discovery free of filesystem mutation; a noisy candidate is killed after
    // the bounded wait and cannot grow an in-memory result beyond the cap.
    let stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(4096)
            .read_to_end(&mut bytes)
            .ok()
            .map(|_| bytes)
    });
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
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
                return None;
            }
        }
    };
    let bytes = reader.join().ok().flatten()?;
    if !status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes);
    let text = text.lines().find(|line| !line.trim().is_empty())?.trim();
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
    Some((
        product,
        BrowserProductVersion::new(version.to_owned()).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, io::Write};

    fn fixture(root: &Path, name: &str, version: &str) -> PathBuf {
        let path = root.join(name);
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        writeln!(file, "#!/bin/sh").unwrap();
        writeln!(file, "echo '{version}'").unwrap();
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&path, permissions).unwrap();
        }
        path
    }

    #[test]
    fn precedence_deduplicates_canonical_paths_and_classifies_versions() {
        let root = tempfile_root();
        let chrome = fixture(&root, "chrome", "Google Chrome 123.4.5");
        let chromium = fixture(&root, "chromium", "Chromium 123.4.5");
        let result = discover_installations_with(DiscoveryInputs {
            explicit: Some(chrome.clone()),
            environment_override: Some(chrome.clone()),
            platform_defaults: vec![chromium.clone()],
            path_names: vec!["chrome".into()],
            path: Some(root.clone()),
        });
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source, BrowserInstallationSource::ExplicitRequest);
        assert_eq!(result[1].product, BrowserProduct::Chromium);
        let _ = fs::remove_dir_all(root);
    }

    fn tempfile_root() -> PathBuf {
        let root = env::temp_dir().join(format!("krometrail-discovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }
}
