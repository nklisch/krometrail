#![allow(dead_code)]

//! Deterministic browser target fixture helpers used by supervision tests.

pub const INDEX_HTML: &str =
    include_str!("../../../../tests/fixtures/browser/cdp-transport-gate/index.html");
pub const ANIMATION_JS: &str =
    include_str!("../../../../tests/fixtures/browser/cdp-transport-gate/animation.js");

pub fn contains_stable_fixture_markers() -> bool {
    INDEX_HTML.contains("CDP") && !ANIMATION_JS.trim().is_empty()
}
