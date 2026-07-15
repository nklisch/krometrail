#![allow(dead_code, unused_imports)]

#[cfg(feature = "qualification-support")]
pub use krometrail_cdp::qualification_support::static_fixture::*;

#[cfg(not(feature = "qualification-support"))]
#[path = "legacy_static_fixture.rs"]
mod legacy;

#[cfg(not(feature = "qualification-support"))]
pub use legacy::*;
