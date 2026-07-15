#![allow(dead_code, unused_imports)]

#[cfg(feature = "qualification-support")]
pub use krometrail_cdp::qualification_support::chrome::*;

#[cfg(not(feature = "qualification-support"))]
#[path = "legacy_chrome.rs"]
mod legacy;

#[cfg(not(feature = "qualification-support"))]
pub use legacy::*;
