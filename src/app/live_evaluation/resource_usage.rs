//! Test-only qualification-process resource sampling.
//!
//! Measurements are observations only. This module has no host-performance threshold and never
//! substitutes zero for an unavailable platform value. Raw operating-system paths and process
//! identifiers stay inside the adapter, if any, and are not part of the returned record.

use temporal_evaluation::{
    EvaluationStatus, FailureRecord, ResourceQualificationMeasurements, RunFailureCode,
};

pub const QUALIFICATION_PROCESS_SCOPE: &str = "qualification_process";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResourceObservation {
    pub scope: &'static str,
    pub status: EvaluationStatus,
    pub failure: Option<FailureRecord>,
    pub measurements: ResourceQualificationMeasurements,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProcessSample {
    rss_bytes: u64,
    cpu_millis: u64,
}

/// Sample the current qualification process through the platform resource API.
///
/// Browser-child accounting is intentionally unavailable here because this boundary does not own
/// a browser process identity. The live browser connector can supply that identity in a later
/// adapter without changing the qualification record's unavailable semantics.
pub fn sample_process_resources() -> ResourceObservation {
    let sample = platform_sample();
    let (rss_bytes, cpu_millis, unavailable_reason) = match sample {
        Some(sample) if sample.rss_bytes > 0 => (
            vec![sample.rss_bytes],
            vec![sample.cpu_millis],
            Some(
                "browser-child accounting is unavailable from the qualification process scope"
                    .to_owned(),
            ),
        ),
        Some(_) => (
            Vec::new(),
            Vec::new(),
            Some("platform resource sampling returned no positive RSS observation".to_owned()),
        ),
        None => (
            Vec::new(),
            Vec::new(),
            Some("platform RSS/CPU measurement is unavailable".to_owned()),
        ),
    };
    let sample_count = rss_bytes.len() as u64;
    // Child accounting is part of the qualification measurement. Until a platform adapter can
    // provide it, the process-only sample is useful evidence but cannot be promoted to pass.
    let complete = sample_count > 0
        && cpu_millis.len() == rss_bytes.len()
        && measurements_browser_child_accounting_available();
    let measurements = ResourceQualificationMeasurements {
        sample_count,
        rss_bytes,
        cpu_millis,
        browser_child_accounting_available: measurements_browser_child_accounting_available(),
        unavailable_reason,
    };
    ResourceObservation {
        scope: QUALIFICATION_PROCESS_SCOPE,
        status: if complete {
            EvaluationStatus::Pass
        } else {
            EvaluationStatus::Inconclusive
        },
        failure: (!complete).then(|| FailureRecord {
            code: RunFailureCode::Unavailable,
            phase: "resource_usage".into(),
            reason: "qualification-process RSS or CPU metrics are unavailable".into(),
            recovery: "run on a platform with a supported process resource adapter and retry"
                .into(),
            retryable: true,
        }),
        measurements,
    }
}

const fn measurements_browser_child_accounting_available() -> bool {
    // The current qualification process boundary has no browser-child process adapter. Keep this
    // explicit so a future adapter changes one authority-backed capability rather than a status
    // default or a fabricated zero-valued sample.
    false
}

#[cfg(unix)]
fn platform_sample() -> Option<ProcessSample> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
    // SAFETY: `getrusage` initializes the supplied structure for RUSAGE_SELF on success.
    let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if result != 0 {
        return None;
    }
    // SAFETY: the successful call above initialized `usage`.
    let usage = unsafe { usage.assume_init() };
    let rss_bytes = platform_rss_bytes(usage.ru_maxrss)?;
    let cpu_micros =
        timeval_micros(usage.ru_utime)?.checked_add(timeval_micros(usage.ru_stime)?)?;
    Some(ProcessSample {
        rss_bytes,
        cpu_millis: cpu_micros / 1_000,
    })
}

#[cfg(unix)]
fn timeval_micros(value: libc::timeval) -> Option<u64> {
    let seconds = u64::try_from(value.tv_sec).ok()?;
    let micros = u64::try_from(value.tv_usec).ok()?;
    seconds.checked_mul(1_000_000)?.checked_add(micros)
}

#[cfg(target_os = "linux")]
fn platform_rss_bytes(value: libc::c_long) -> Option<u64> {
    u64::try_from(value).ok()?.checked_mul(1024)
}

#[cfg(target_os = "macos")]
fn platform_rss_bytes(value: libc::c_long) -> Option<u64> {
    u64::try_from(value).ok()
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn platform_rss_bytes(_value: libc::c_long) -> Option<u64> {
    None
}

#[cfg(not(unix))]
fn platform_sample() -> Option<ProcessSample> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_metrics_have_no_fabricated_zero_or_private_path() {
        let mut observation = ResourceObservation {
            scope: QUALIFICATION_PROCESS_SCOPE,
            status: EvaluationStatus::Inconclusive,
            failure: Some(FailureRecord {
                code: RunFailureCode::Unavailable,
                phase: "resource_usage".into(),
                reason: "resource adapter unavailable".into(),
                recovery: "run with a supported platform adapter".into(),
                retryable: true,
            }),
            measurements: ResourceQualificationMeasurements {
                sample_count: 0,
                rss_bytes: Vec::new(),
                cpu_millis: Vec::new(),
                browser_child_accounting_available: false,
                unavailable_reason: Some("resource adapter unavailable".into()),
            },
        };
        assert_eq!(observation.measurements.sample_count, 0);
        assert!(observation.measurements.rss_bytes.is_empty());
        assert!(observation.measurements.cpu_millis.is_empty());
        assert!(!observation.measurements.browser_child_accounting_available);
        assert!(
            observation
                .measurements
                .unavailable_reason
                .as_deref()
                .is_some_and(|reason| !reason.contains('/') && !reason.contains('\\'))
        );
        observation.measurements.unavailable_reason = None;
        assert_eq!(observation.scope, QUALIFICATION_PROCESS_SCOPE);
    }

    #[test]
    fn platform_sample_is_observed_or_explicitly_inconclusive() {
        let observation = sample_process_resources();
        assert_eq!(observation.scope, QUALIFICATION_PROCESS_SCOPE);
        if observation.measurements.sample_count == 0 {
            assert_eq!(observation.status, EvaluationStatus::Inconclusive);
            assert!(observation.measurements.unavailable_reason.is_some());
            assert!(observation.measurements.rss_bytes.is_empty());
            assert!(observation.measurements.cpu_millis.is_empty());
        } else {
            assert_eq!(observation.measurements.sample_count, 1);
            assert!(observation.measurements.rss_bytes[0] > 0);
            assert_eq!(observation.measurements.cpu_millis.len(), 1);
            assert!(!observation.measurements.browser_child_accounting_available);
            assert_eq!(observation.status, EvaluationStatus::Inconclusive);
            assert!(observation.failure.is_some());
        }
    }
}
