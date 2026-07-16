//! Temporal debug bundle policy, marker assembly, and focus extraction.
//!
//! The wired [`TemporalDebugBundleService`] composes the existing temporal-query,
//! artifact-generation, context, and timeline ports. This module keeps the
//! versioned policy plus its pure marker and visual-focus helpers alongside that
//! service; it performs no browser or storage I/O itself.

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

// Keep the service and its pure policy helpers together as the bundle composition boundary.
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
