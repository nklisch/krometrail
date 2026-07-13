#![allow(dead_code)]

use std::{env, path::PathBuf, sync::OnceLock};

static REAL_BROWSER_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

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

pub fn temporary_profile_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("krometrail-real-{name}-{}", std::process::id()))
}
