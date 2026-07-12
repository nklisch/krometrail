//! Chrome DevTools Protocol adapter boundary.
//!
//! The production boundary intentionally has no implementation yet. Qualification code is
//! disposable and is only compiled when an explicit spike feature is requested.

#[cfg(feature = "cdp-spike")]
#[doc(hidden)]
pub mod spike;
