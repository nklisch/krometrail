#![allow(dead_code)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

static REAL_BROWSER_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
static TEMPORARY_ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
            || name.starts_with("krometrail-real-targets-"))
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

#[cfg(not(target_os = "linux"))]
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
}
