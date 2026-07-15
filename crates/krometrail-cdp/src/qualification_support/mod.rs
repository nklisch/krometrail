//! Opt-in support for local real-browser qualification tests.
//!
//! This module is intentionally feature-gated. It is a test harness boundary, not a product
//! launcher or public browser-control API.

pub mod chrome;
pub mod static_fixture;

pub use chrome::{
    ChromeViewport, ChromeWrapper, ChromeWrapperVariant, TemporaryRootGuard,
    cleanup_real_browser_roots, process_references, real_browser_lock, real_browser_tests_enabled,
    temporary_profile_root,
};
pub use static_fixture::{FixtureServer, contains_stable_fixture_markers};
