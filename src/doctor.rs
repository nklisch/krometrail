//! Discovery-only `doctor` composition.
//!
//! Doctor answers one question: can this machine supply a compatible browser? It
//! invokes the existing bounded discovery authority directly instead of going
//! through the recording runtime, so no instance ownership, cache reclamation or
//! recovery, storage validation, or recording-only configuration parsing ever
//! runs for it. An unusable storage root or an invalid `KROMETRAIL_DISK_BUDGET_BYTES`
//! / `KROMETRAIL_RETENTION_MAX_AGE_SECS` value therefore cannot block discovery.
//! The only data-root side effect doctor can produce is the process-wide
//! best-effort diagnostic log owned by `main`.

use std::sync::Arc;

use krometrail_cdp::{ChromeLauncher, LaunchError, LauncherConfig, SystemChromeLauncher};
use krometrail_core::{ErrorCode, KrometrailError, NonEmptyText, Result, RetryAdvice};

/// The doctor command, composed from the discovery seam alone.
pub(crate) struct Doctor {
    launcher: Arc<dyn ChromeLauncher>,
}

impl Doctor {
    /// Composes doctor with the production discovery authority. The launcher's
    /// profile configuration is inert here: doctor only ever asks for
    /// installations and never launches a browser or lists profiles.
    pub(crate) fn with_system_launcher() -> Self {
        Self::new(Arc::new(SystemChromeLauncher::new(
            LauncherConfig::default(),
        )))
    }

    pub(crate) fn new(launcher: Arc<dyn ChromeLauncher>) -> Self {
        Self { launcher }
    }

    /// Reports the discovered installations, or the stable `browser_not_found`
    /// error when none exist.
    pub(crate) async fn run(&self) -> Result<DoctorOutcome> {
        let installations = self
            .launcher
            .installations()
            .await
            .map_err(doctor_launcher_error)?;
        if installations.is_empty() {
            return Err(browser_not_found());
        }
        Ok(DoctorOutcome {
            installation_count: installations.len(),
        })
    }
}

/// What doctor found. The success line is derived here so the stdout contract
/// has exactly one owner.
#[derive(Debug)]
pub(crate) struct DoctorOutcome {
    installation_count: usize,
}

impl DoctorOutcome {
    pub(crate) fn success_line(&self) -> String {
        format!(
            "browser available: {} installation(s)",
            self.installation_count
        )
    }
}

/// Maps launcher failures onto the stable error surface without leaking
/// executable or profile paths, matching the connector's error discipline.
fn doctor_launcher_error(error: LaunchError) -> KrometrailError {
    let code = error.stable_code();
    let message = match code {
        ErrorCode::BrowserNotFound => "no supported browser installation was found",
        _ => "browser discovery failed",
    };
    KrometrailError::new(
        code,
        NonEmptyText::new(message).expect("static doctor error message is non-empty"),
    )
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("install Chrome or Chromium, then run doctor again")
            .expect("static doctor recovery message is non-empty"),
    )
}

fn browser_not_found() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::BrowserNotFound,
        NonEmptyText::new("no supported browser installation was found")
            .expect("static browser error message is non-empty"),
    )
    .with_retry(RetryAdvice::AfterRecovery)
    .with_recovery(
        NonEmptyText::new("install Chrome or Chromium, then run doctor again")
            .expect("static browser recovery message is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_cdp::launcher::LauncherFuture;
    use krometrail_core::{
        BrowserInstallation, BrowserInstallationSource, BrowserProduct, BrowserProductVersion,
        LaunchBrowser, ManagedProfileSummary,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeLauncher {
        installations: Vec<BrowserInstallation>,
        // `LaunchError` is not `Clone`, and the trait hands the fake only
        // `&self`, so the fake stores a constructor and builds it per call.
        error: Option<fn() -> LaunchError>,
        installation_calls: AtomicUsize,
    }

    impl FakeLauncher {
        fn finding(installations: Vec<BrowserInstallation>) -> Self {
            Self {
                installations,
                error: None,
                installation_calls: AtomicUsize::new(0),
            }
        }

        fn failing(error: fn() -> LaunchError) -> Self {
            Self {
                installations: Vec::new(),
                error: Some(error),
                installation_calls: AtomicUsize::new(0),
            }
        }
    }

    impl ChromeLauncher for FakeLauncher {
        fn installations(
            &self,
        ) -> LauncherFuture<'_, Result<Vec<BrowserInstallation>, LaunchError>> {
            self.installation_calls.fetch_add(1, Ordering::SeqCst);
            let error = self.error;
            let installations = self.installations.clone();
            Box::pin(async move {
                if let Some(make_error) = error {
                    return Err(make_error());
                }
                Ok(installations)
            })
        }

        fn managed_profiles(
            &self,
        ) -> LauncherFuture<'_, Result<Vec<ManagedProfileSummary>, LaunchError>> {
            panic!("doctor must not enumerate managed profiles");
        }

        fn launch(
            &self,
            _request: &LaunchBrowser,
        ) -> LauncherFuture<'_, Result<krometrail_cdp::LaunchedChrome, LaunchError>> {
            panic!("doctor must never launch a browser");
        }
    }

    fn installation(executable: &str) -> BrowserInstallation {
        BrowserInstallation::new(
            executable,
            BrowserInstallationSource::PathLookup,
            BrowserProduct::Chrome,
            BrowserProductVersion::new("123.4.5.6").unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn doctor_reports_discovered_installations_without_touching_the_browser() {
        let launcher = Arc::new(FakeLauncher::finding(vec![
            installation("/usr/bin/fixture-chrome"),
            installation("/usr/bin/fixture-chromium"),
        ]));
        let doctor = Doctor::new(Arc::clone(&launcher) as Arc<dyn ChromeLauncher>);

        let outcome = doctor.run().await.unwrap();

        assert_eq!(
            outcome.success_line(),
            "browser available: 2 installation(s)"
        );
        assert_eq!(launcher.installation_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn doctor_reports_the_explicit_browser_not_found_error_without_a_browser() {
        let launcher = Arc::new(FakeLauncher::finding(Vec::new()));
        let doctor = Doctor::new(Arc::clone(&launcher) as Arc<dyn ChromeLauncher>);

        let error = doctor.run().await.unwrap_err();

        assert_eq!(error.code, ErrorCode::BrowserNotFound);
        assert_eq!(
            error.message.as_str(),
            "no supported browser installation was found"
        );
        assert_eq!(error.retry, RetryAdvice::AfterRecovery);
        assert_eq!(
            error.recovery.as_ref().map(|recovery| recovery.as_str()),
            Some("install Chrome or Chromium, then run doctor again")
        );
        assert_eq!(launcher.installation_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn doctor_maps_launcher_failures_onto_stable_errors() {
        let launcher = Arc::new(FakeLauncher::failing(|| LaunchError::ExecutableUnavailable));
        let doctor = Doctor::new(launcher);

        let error = doctor.run().await.unwrap_err();

        assert_eq!(error.code, ErrorCode::BrowserNotFound);
        assert_eq!(
            error.message.as_str(),
            "no supported browser installation was found"
        );
    }
}
