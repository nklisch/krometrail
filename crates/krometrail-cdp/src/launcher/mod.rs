//! Chrome discovery and owned launch lifecycle.
//!
//! This module deliberately stops at a ready browser endpoint. Target supervision and transport
//! ownership are layered above it. The attach path only validates/resolves an endpoint and never
//! constructs a process or profile guard.

mod discovery;
mod process;
mod profile;
mod startup;

pub use discovery::{
    DiscoveryCandidate, DiscoveryInputs, discover_installations, discover_installations_with,
};
pub use process::{ManagedChromeProcess, ProcessError, ProcessTermination, SanitizedProcessExit};
pub use profile::{ProfileError, ProfileLease, ProfileLeaseKind};
pub use startup::{
    LaunchError, LaunchedChrome, LauncherConfig, SystemChromeLauncher, attach_endpoint,
};

use krometrail_core::{BrowserInstallation, LaunchBrowser, ManagedProfileSummary};
use std::{future::Future, pin::Pin, sync::Arc};

pub type LauncherFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Adapter seam for discovery and managed Chrome launch.
pub trait ChromeLauncher: Send + Sync {
    fn installations(&self) -> LauncherFuture<'_, Result<Vec<BrowserInstallation>, LaunchError>>;
    fn managed_profiles(
        &self,
    ) -> LauncherFuture<'_, Result<Vec<ManagedProfileSummary>, LaunchError>> {
        Box::pin(async { Ok(Vec::new()) })
    }
    fn launch(
        &self,
        request: &LaunchBrowser,
    ) -> LauncherFuture<'_, Result<LaunchedChrome, LaunchError>>;
}

impl<T: ChromeLauncher + ?Sized> ChromeLauncher for Arc<T> {
    fn installations(&self) -> LauncherFuture<'_, Result<Vec<BrowserInstallation>, LaunchError>> {
        (**self).installations()
    }

    fn managed_profiles(
        &self,
    ) -> LauncherFuture<'_, Result<Vec<ManagedProfileSummary>, LaunchError>> {
        (**self).managed_profiles()
    }

    fn launch(
        &self,
        request: &LaunchBrowser,
    ) -> LauncherFuture<'_, Result<LaunchedChrome, LaunchError>> {
        (**self).launch(request)
    }
}
