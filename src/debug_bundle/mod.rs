//! Temporal debug bundle policy, marker assembly, and focus extraction.
//!
//! This module owns the pure, versioned `temporal-debug-bundle-v1` evidence
//! policy: the exact artifact generator requests, bounded privacy-safe marker
//! assembly, and deterministic major-change focus extraction. It produces the
//! inputs the bundle service (Unit 3) composes into one `TemporalDebugBundle`;
//! it does not perform generation, event correlation, or orchestration.
//!
//! The functions and types here are `pub(crate)` for the upcoming
//! `TemporalDebugBundleService` (Unit 3 — bounded composition). They are
//! exercised by focused tests until the service wires them into `RuntimeDependencies`.

mod error;
mod focus;
mod header;
mod markers;
mod policy;
mod service;

#[cfg(test)]
mod tests;

use krometrail_core::{
    ArtifactFailurePolicy, BrowserEventFilter, BrowserEventSelection, EffectiveBundlePolicy,
    InteractionAnchorSource, InteractionRecordSource, OrientationPolicy, ResolvedRange, Result,
    SessionTime, TimelineStore,
};
use policy::policy_version;

// Re-exported for the upcoming `TemporalDebugBundleService` (Unit 3). They are
// exercised by focused tests until the service wires them into composition.
pub(crate) use error::controlled;
pub(crate) use focus::extract_focus_times;
pub(crate) use header::{BrowserEventEvidenceState, VisualEvidenceState, compose_header};
pub(crate) use markers::{MarkerEvidence, assemble_markers};
pub(crate) use policy::{default_artifact_request, default_generators};
pub(crate) use service::{BundleWorkLimits, TemporalDebugBundleService};

/// The zero-method intersection of the three evidence-reading ports the bundle
/// service projects over one concrete store. It introduces no facade methods;
/// any type implementing `TimelineStore + InteractionAnchorSource +
/// InteractionRecordSource` implements it automatically.
pub(crate) trait TemporalDebugEvidenceStore:
    TimelineStore + InteractionAnchorSource + InteractionRecordSource
{
}

impl<T> TemporalDebugEvidenceStore for T where
    T: TimelineStore + InteractionAnchorSource + InteractionRecordSource
{
}

/// Builds the effective bundle policy from the resolved range, caller
/// orientation choice, and extracted focus times.
///
/// The effective policy carries the versioned identifier, the exact resolved
/// anchor time, the two v1 generator requests, `AllowPartial` failure policy,
/// the default all-class/debug event filter, the default compact event
/// selection (limit 24), and the focus times. It is the observable contract
/// between the bundle request and the artifact/context services.
pub(crate) fn build_effective_policy(
    range: &ResolvedRange,
    orientation: OrientationPolicy,
    focus_times: Vec<SessionTime>,
) -> Result<EffectiveBundlePolicy> {
    EffectiveBundlePolicy::new(
        policy_version(),
        range.resolved_anchor.effective_time,
        default_generators(range, orientation),
        ArtifactFailurePolicy::AllowPartial,
        BrowserEventFilter::default(),
        BrowserEventSelection::compact_default(),
        focus_times,
    )
}
