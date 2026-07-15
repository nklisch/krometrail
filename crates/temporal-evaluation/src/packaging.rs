use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    ArtifactKind, ConditionId, ContractError, EvidenceAvailability, NamedVersion, Result,
    RetentionState, SourceFrameEvidence, SourceInterval, canonical_conditions, canonical_json,
    privacy, sha256_prefixed,
};

pub const CONDITION_PACKAGER_VERSION: &str = "temporal-condition-packager-v1";
pub const UNIFORM_SOURCE_FRAME_SLOTS: usize = 8;

const STORYBOARD_NAME: &str = "temporal-storyboard";
const STORYBOARD_VERSION: &str = "1.1.0";
const DIFFERENCE_MAP_NAME: &str = "temporal-difference-map";
const DIFFERENCE_MAP_VERSION: &str = "v1";
const REGION_FILMSTRIP_NAME: &str = "region-filmstrip";
const REGION_FILMSTRIP_VERSION: &str = "1.0.0";

/// Fixed non-claims attached to every condition package.
///
/// This registry is deliberately shared with later result records rather than allowing each
/// condition to invent its own privacy disclaimer set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NonClaimId {
    NoChromeCaptureClaim,
    NoNetworkClaim,
    NoPaidModelClaim,
    NoModelComprehensionClaim,
    NoProductThesisClaim,
    NoCausalDiagnosisClaim,
    NoDeterministicReplayClaim,
    NoCrossModelGeneralizationClaim,
    NoUnobservedFrameClaim,
    NoGroundTruthFromArtifactClaim,
}

impl NonClaimId {
    pub const ALL: [Self; 10] = [
        Self::NoChromeCaptureClaim,
        Self::NoNetworkClaim,
        Self::NoPaidModelClaim,
        Self::NoModelComprehensionClaim,
        Self::NoProductThesisClaim,
        Self::NoCausalDiagnosisClaim,
        Self::NoDeterministicReplayClaim,
        Self::NoCrossModelGeneralizationClaim,
        Self::NoUnobservedFrameClaim,
        Self::NoGroundTruthFromArtifactClaim,
    ];
}

/// The kind of an opaque evidence handle. Artifact kinds reuse the temporal-vision registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "artifact_kind",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum EvidenceReferenceKind {
    SourceFrame,
    CurrentObservation,
    Artifact(ArtifactKind),
    CaptureSummary,
    ContextSummary,
    ProgressiveRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceReference {
    pub id: String,
    pub kind: EvidenceReferenceKind,
    pub sha256: Option<String>,
    pub availability: EvidenceAvailability,
}

impl EvidenceReference {
    pub fn new(
        id: impl Into<String>,
        kind: EvidenceReferenceKind,
        sha256: Option<String>,
        availability: EvidenceAvailability,
    ) -> Result<Self> {
        let value = Self {
            id: id.into(),
            kind,
            sha256,
            availability,
        };
        value.validate("evidence reference")?;
        Ok(value)
    }

    fn validate(&self, label: &str) -> Result<()> {
        privacy::validate_opaque_id(&self.id, &format!("{label} id"))?;
        if let Some(hash) = &self.sha256 {
            privacy::validate_sha256(hash, &format!("{label} hash"))?;
        }
        if self.availability == EvidenceAvailability::Retained && self.sha256.is_none() {
            return Err(ContractError::new(format!(
                "retained {label} must carry its exact SHA-256"
            )));
        }
        Ok(())
    }

    fn is_retained_source(&self, frame: &SourceFrameEvidence) -> bool {
        self.kind == EvidenceReferenceKind::SourceFrame
            && self.id == frame.id
            && self.availability == EvidenceAvailability::Retained
            && self.sha256.as_deref() == Some(frame.encoded_sha256.as_str())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactCacheIdentity {
    pub cache_schema_version: u32,
    pub cache_key: String,
    pub source_fingerprint: String,
    pub parameter_hash: String,
    pub visual_epoch_hash: String,
    pub adapter_version: NamedVersion,
    pub generator: NamedVersion,
}

impl ArtifactCacheIdentity {
    pub fn validate(&self) -> Result<()> {
        if self.cache_schema_version == 0 {
            return Err(ContractError::new(
                "artifact cache schema version must be non-zero",
            ));
        }
        for (value, label) in [
            (&self.cache_key, "artifact cache key"),
            (&self.source_fingerprint, "artifact source fingerprint"),
            (&self.parameter_hash, "artifact parameter hash"),
            (&self.visual_epoch_hash, "artifact visual epoch hash"),
        ] {
            privacy::validate_sha256(value, label)?;
        }
        validate_named_version(&self.adapter_version, "artifact adapter version")?;
        validate_named_version(&self.generator, "artifact generator")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEvidenceReference {
    pub output: EvidenceReference,
    /// The authority manifest's resolved range, retained solely to reject mixed intervals.
    pub resolved_range: crate::TimeRangeNs,
    pub manifest_sha256: String,
    pub source_frame_ids: Vec<String>,
    pub selected_frame_ids: Vec<String>,
    pub gap_ids: Vec<String>,
    pub algorithm_versions: Vec<NamedVersion>,
    pub cache: ArtifactCacheIdentity,
}

impl ArtifactEvidenceReference {
    pub fn validate(&self, interval: &SourceInterval, label: &str) -> Result<()> {
        self.validate_shape(label)?;
        if self.resolved_range != interval.resolved_range {
            return Err(ContractError::new(format!(
                "{label} does not preserve the exact source interval range"
            )));
        }
        validate_exact_ids(
            &self.source_frame_ids,
            &interval.frame_ids(),
            &format!("{label} source frame ids"),
        )?;
        for id in &self.selected_frame_ids {
            let frame = interval.frame(id).ok_or_else(|| {
                ContractError::new(format!("{label} selected source frame is unknown"))
            })?;
            if frame.availability != EvidenceAvailability::Retained {
                return Err(ContractError::new(format!(
                    "{label} selects a source frame that is not retained"
                )));
            }
        }
        validate_exact_ids(
            &self.gap_ids,
            &interval.gap_ids(),
            &format!("{label} gap ids"),
        )?;
        Ok(())
    }

    fn validate_shape(&self, label: &str) -> Result<()> {
        self.output.validate(&format!("{label} output"))?;
        if !matches!(self.output.kind, EvidenceReferenceKind::Artifact(_)) {
            return Err(ContractError::new(format!(
                "{label} output must be an artifact reference"
            )));
        }
        privacy::validate_sha256(&self.manifest_sha256, &format!("{label} manifest hash"))?;
        validate_ordered_unique_ids(&self.source_frame_ids, &format!("{label} source frame ids"))?;
        validate_ordered_subset(
            &self.source_frame_ids,
            &self.selected_frame_ids,
            &format!("{label} selected frame ids"),
        )?;
        if self.selected_frame_ids.len() > UNIFORM_SOURCE_FRAME_SLOTS {
            return Err(ContractError::new(format!(
                "{label} selected source frames exceed the eight-frame artifact limit"
            )));
        }
        validate_ordered_unique_ids(&self.gap_ids, &format!("{label} gap ids"))?;
        if self.algorithm_versions.is_empty() {
            return Err(ContractError::new(format!(
                "{label} must retain its authority algorithm version"
            )));
        }
        for (index, version) in self.algorithm_versions.iter().enumerate() {
            validate_named_version(version, &format!("{label} algorithm version {index}"))?;
        }
        self.cache.validate()
    }

    fn validate_for_kind(
        &self,
        interval: &SourceInterval,
        expected_kind: ArtifactKind,
        label: &str,
    ) -> Result<()> {
        self.validate(interval, label)?;
        if self.output.kind != EvidenceReferenceKind::Artifact(expected_kind) {
            return Err(ContractError::new(format!(
                "{label} has the wrong artifact kind"
            )));
        }
        self.validate_authority(expected_kind, label)
    }

    fn validate_authority(&self, expected_kind: ArtifactKind, label: &str) -> Result<()> {
        let (expected_name, expected_version) =
            artifact_authority(expected_kind).ok_or_else(|| {
                ContractError::new(format!(
                    "{label} is not a supported source-derived artifact"
                ))
            })?;
        if self.algorithm_versions.len() != 1
            || self.algorithm_versions[0].name != expected_name
            || self.algorithm_versions[0].version != expected_version
            || self.cache.generator.name != expected_name
            || self.cache.generator.version != expected_version
        {
            return Err(ContractError::new(format!(
                "{label} does not preserve the exact existing artifact authority identity"
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemporalBundleEvidence {
    pub bundle: EvidenceReference,
    pub before_during_after: Vec<ArtifactEvidenceReference>,
    pub storyboards: Vec<ArtifactEvidenceReference>,
    pub difference_maps: Vec<ArtifactEvidenceReference>,
    pub capture_summary: EvidenceReference,
    pub context_summary: EvidenceReference,
    pub evidence_references: Vec<EvidenceReference>,
}

impl TemporalBundleEvidence {
    fn validate(&self, interval: &SourceInterval) -> Result<()> {
        validate_temporal_bundle_shape(self)?;
        self.bundle.validate("temporal bundle")?;
        if self.bundle.kind != EvidenceReferenceKind::Artifact(ArtifactKind::TemporalDebugBundle) {
            return Err(ContractError::new(
                "temporal bundle handle must use the temporal-debug-bundle artifact kind",
            ));
        }
        validate_nonempty_artifacts(
            &self.before_during_after,
            interval,
            ArtifactKind::BeforeDuringAfter,
            "before/during/after",
        )?;
        validate_nonempty_artifacts(
            &self.storyboards,
            interval,
            ArtifactKind::ChangeAwareStoryboard,
            "temporal bundle storyboard",
        )?;
        for artifact in &self.difference_maps {
            artifact.validate(interval, "temporal bundle difference map")?;
            if artifact.output.kind != EvidenceReferenceKind::Artifact(ArtifactKind::DifferenceMap)
            {
                return Err(ContractError::new(
                    "temporal bundle difference map has the wrong artifact kind",
                ));
            }
            if artifact.algorithm_versions.len() != 1
                || artifact.algorithm_versions[0].name != DIFFERENCE_MAP_NAME
                || artifact.algorithm_versions[0].version != DIFFERENCE_MAP_VERSION
            {
                return Err(ContractError::new(
                    "temporal bundle difference map must use the existing difference-map authority",
                ));
            }
        }
        if self.difference_maps.is_empty() {
            return Err(ContractError::new(
                "temporal bundle must preserve its difference-map outcome",
            ));
        }
        self.capture_summary.validate("capture summary")?;
        if self.capture_summary.kind != EvidenceReferenceKind::CaptureSummary {
            return Err(ContractError::new(
                "temporal bundle capture summary has the wrong evidence kind",
            ));
        }
        self.context_summary.validate("context summary")?;
        if self.context_summary.kind != EvidenceReferenceKind::ContextSummary {
            return Err(ContractError::new(
                "temporal bundle context summary has the wrong evidence kind",
            ));
        }
        for reference in &self.evidence_references {
            reference.validate("temporal bundle evidence reference")?;
        }
        validate_unique_reference_ids(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressiveRetrievalRecord {
    pub request_id: String,
    pub requested_frame_ids: Vec<String>,
    pub returned_frames: Vec<EvidenceReference>,
    pub unavailable_frame_ids: Vec<String>,
}

impl ProgressiveRetrievalRecord {
    fn validate(&self, interval: &SourceInterval) -> Result<()> {
        privacy::validate_opaque_id(&self.request_id, "progressive request id")?;
        validate_ordered_unique_ids(&self.requested_frame_ids, "progressive requested frame ids")?;
        for id in &self.requested_frame_ids {
            if interval.frame(id).is_none() {
                return Err(ContractError::new(
                    "progressive request contains an unknown source frame",
                ));
            }
        }
        if self.requested_frame_ids.is_empty() {
            return Err(ContractError::new(
                "progressive request must contain at least one source frame",
            ));
        }
        if self.requested_frame_ids.len() > 4 {
            return Err(ContractError::new(
                "progressive request exceeds the four-frame budget",
            ));
        }
        validate_ordered_unique_ids(
            &self.unavailable_frame_ids,
            "progressive unavailable frame ids",
        )?;
        for id in &self.unavailable_frame_ids {
            if !self.requested_frame_ids.contains(id) {
                return Err(ContractError::new(
                    "progressive unavailable frame is not part of its request",
                ));
            }
        }
        let mut returned_ids = Vec::with_capacity(self.returned_frames.len());
        for reference in &self.returned_frames {
            reference.validate("progressive returned frame")?;
            if reference.kind != EvidenceReferenceKind::SourceFrame {
                return Err(ContractError::new(
                    "progressive returned evidence must be a source frame",
                ));
            }
            let frame = interval.frame(&reference.id).ok_or_else(|| {
                ContractError::new("progressive returned frame is outside the source interval")
            })?;
            if !reference.is_retained_source(frame) {
                return Err(ContractError::new(
                    "progressive returned frame is not the exact retained source identity",
                ));
            }
            returned_ids.push(reference.id.clone());
        }
        validate_ordered_unique_ids(&returned_ids, "progressive returned frame ids")?;
        if returned_ids
            .iter()
            .any(|id| self.unavailable_frame_ids.contains(id))
            || returned_ids
                .iter()
                .any(|id| !self.requested_frame_ids.contains(id))
        {
            return Err(ContractError::new(
                "progressive returned and unavailable frame identities overlap or escape the request",
            ));
        }
        if returned_ids.len() + self.unavailable_frame_ids.len() != self.requested_frame_ids.len() {
            return Err(ContractError::new(
                "progressive request must preserve every requested id as returned or unavailable",
            ));
        }
        if !is_subsequence(&self.requested_frame_ids, &returned_ids)
            || !is_subsequence(&self.requested_frame_ids, &self.unavailable_frame_ids)
        {
            return Err(ContractError::new(
                "progressive returned and unavailable ids must retain request order",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProgressiveConditionEvidence {
    pub bundle: TemporalBundleEvidence,
    pub source_retrievals: Vec<ProgressiveRetrievalRecord>,
    pub region_filmstrip: Option<ArtifactEvidenceReference>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "evidence", rename_all = "snake_case", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)]
pub enum ConditionEvidence {
    FinalScreenshot {
        final_frame_id: String,
        current_observation: EvidenceReference,
    },
    UniformStoryboard {
        slot_frame_ids: Vec<String>,
    },
    ChangeAwareStoryboard {
        artifacts: Vec<ArtifactEvidenceReference>,
    },
    TemporalBundle(TemporalBundleEvidence),
    ProgressiveSource(ProgressiveConditionEvidence),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConditionPackage {
    pub packager_version: String,
    pub condition_id: ConditionId,
    pub source_interval_digest: String,
    pub source_frame_ids: Vec<String>,
    pub gap_ids: Vec<String>,
    pub retention: RetentionState,
    pub evidence: ConditionEvidence,
    pub non_claims: Vec<NonClaimId>,
    pub digest: String,
}

impl ConditionPackage {
    pub fn validate(&self) -> Result<()> {
        if self.packager_version != CONDITION_PACKAGER_VERSION {
            return Err(ContractError::new(
                "unsupported temporal condition packager version",
            ));
        }
        privacy::validate_sha256(
            &self.source_interval_digest,
            "condition source interval digest",
        )?;
        if self.source_frame_ids.is_empty() {
            return Err(ContractError::new(
                "condition package must retain the source interval frame identities",
            ));
        }
        validate_ordered_unique_ids(&self.source_frame_ids, "condition source frame ids")?;
        validate_ordered_unique_ids(&self.gap_ids, "condition gap ids")?;
        if self.non_claims != NonClaimId::ALL {
            return Err(ContractError::new(
                "condition package must carry the fixed non-claim registry",
            ));
        }
        if !canonical_conditions()
            .iter()
            .any(|condition| condition.condition_id == self.condition_id)
        {
            return Err(ContractError::new(
                "condition package uses an unknown condition",
            ));
        }
        validate_evidence_shape(&self.evidence, self.condition_id)?;
        privacy::validate_sha256(&self.digest, "condition package digest")?;
        if self.digest != self.computed_digest()? {
            return Err(ContractError::new(
                "condition package digest does not match its exact identities",
            ));
        }
        validate_package_privacy(self)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(self.digest.clone())
    }

    fn computed_digest(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .ok_or_else(|| ContractError::new("condition package did not serialize as an object"))?
            .remove("digest");
        Ok(sha256_prefixed(&canonical_json(&value)?))
    }

    fn from_interval(
        interval: &SourceInterval,
        condition_id: ConditionId,
        evidence: ConditionEvidence,
    ) -> Result<Self> {
        interval.validate()?;
        let mut value = Self {
            packager_version: CONDITION_PACKAGER_VERSION.into(),
            condition_id,
            source_interval_digest: interval.digest()?,
            source_frame_ids: interval.frame_ids(),
            gap_ids: interval.gap_ids(),
            retention: interval.retention,
            evidence,
            non_claims: NonClaimId::ALL.to_vec(),
            digest: String::new(),
        };
        value.validate_without_digest(interval)?;
        value.digest = value.computed_digest()?;
        value.validate()?;
        Ok(value)
    }

    fn validate_without_digest(&self, interval: &SourceInterval) -> Result<()> {
        if self.source_interval_digest != interval.digest()? {
            return Err(ContractError::new(
                "condition package does not use the exact source interval digest",
            ));
        }
        if self.source_frame_ids != interval.frame_ids() || self.gap_ids != interval.gap_ids() {
            return Err(ContractError::new(
                "condition package source identities do not match the source interval",
            ));
        }
        if self.retention != interval.retention {
            return Err(ContractError::new(
                "condition package retention does not match the source interval",
            ));
        }
        validate_evidence(&self.evidence, self.condition_id, interval)
    }
}

impl<'de> Deserialize<'de> for ConditionPackage {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            packager_version: String,
            condition_id: ConditionId,
            source_interval_digest: String,
            source_frame_ids: Vec<String>,
            gap_ids: Vec<String>,
            retention: RetentionState,
            evidence: ConditionEvidence,
            non_claims: Vec<NonClaimId>,
            digest: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        let value = Self {
            packager_version: wire.packager_version,
            condition_id: wire.condition_id,
            source_interval_digest: wire.source_interval_digest,
            source_frame_ids: wire.source_frame_ids,
            gap_ids: wire.gap_ids,
            retention: wire.retention,
            evidence: wire.evidence,
            non_claims: wire.non_claims,
            digest: wire.digest,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

/// Pure constructors for the five evidence conditions. Authority adapters supply the references;
/// this type never reads, decodes, renders, or regenerates evidence.
pub struct ConditionPackager;

impl ConditionPackager {
    pub fn final_screenshot(
        interval: &SourceInterval,
        final_frame_id: &str,
        current_observation: EvidenceReference,
    ) -> Result<ConditionPackage> {
        let final_frame = interval
            .retained_frames()
            .last()
            .ok_or_else(|| ContractError::new("condition A has no retained final source frame"))?;
        if final_frame.id != final_frame_id {
            return Err(ContractError::new(
                "condition A must reference the interval's final retained source frame",
            ));
        }
        if current_observation.kind != EvidenceReferenceKind::CurrentObservation {
            return Err(ContractError::new(
                "condition A current observation has the wrong evidence kind",
            ));
        }
        ConditionPackage::from_interval(
            interval,
            ConditionId::AFinalScreenshot,
            ConditionEvidence::FinalScreenshot {
                final_frame_id: final_frame_id.into(),
                current_observation,
            },
        )
    }

    pub fn uniform_storyboard(interval: &SourceInterval) -> Result<ConditionPackage> {
        let retained = interval.retained_frames().collect::<Vec<_>>();
        if retained.len() < UNIFORM_SOURCE_FRAME_SLOTS {
            return Err(ContractError::new(
                "condition B is unavailable below eight retained source frames",
            ));
        }
        let last = retained.len() - 1;
        let slot_frame_ids = (0..UNIFORM_SOURCE_FRAME_SLOTS)
            .map(|slot| {
                let numerator = (slot as u128) * (last as u128);
                let index = (numerator / 7) as usize;
                retained[index].id.clone()
            })
            .collect();
        ConditionPackage::from_interval(
            interval,
            ConditionId::BUniformStoryboard,
            ConditionEvidence::UniformStoryboard { slot_frame_ids },
        )
    }

    pub fn change_aware_storyboard(
        interval: &SourceInterval,
        artifacts: Vec<ArtifactEvidenceReference>,
    ) -> Result<ConditionPackage> {
        if artifacts.is_empty() {
            return Err(ContractError::new(
                "condition C requires an existing storyboard projection",
            ));
        }
        validate_unique_artifact_outputs(&artifacts)?;
        ConditionPackage::from_interval(
            interval,
            ConditionId::CChangeAwareStoryboard,
            ConditionEvidence::ChangeAwareStoryboard { artifacts },
        )
    }

    pub fn temporal_bundle(
        interval: &SourceInterval,
        bundle: TemporalBundleEvidence,
    ) -> Result<ConditionPackage> {
        ConditionPackage::from_interval(
            interval,
            ConditionId::DTemporalBundle,
            ConditionEvidence::TemporalBundle(bundle),
        )
    }

    pub fn progressive_source(
        interval: &SourceInterval,
        evidence: ProgressiveConditionEvidence,
    ) -> Result<ConditionPackage> {
        ConditionPackage::from_interval(
            interval,
            ConditionId::EProgressiveSource,
            ConditionEvidence::ProgressiveSource(evidence),
        )
    }
}

pub fn require_one_source_interval(packages: &[ConditionPackage]) -> Result<String> {
    let first = packages
        .first()
        .ok_or_else(|| ContractError::new("at least one condition package is required"))?;
    first.validate()?;
    for package in &packages[1..] {
        package.validate()?;
        if package.source_interval_digest != first.source_interval_digest
            || package.source_frame_ids != first.source_frame_ids
            || package.gap_ids != first.gap_ids
            || package.retention != first.retention
        {
            return Err(ContractError::new(
                "condition packages do not share one exact source interval",
            ));
        }
    }
    Ok(first.source_interval_digest.clone())
}

fn validate_temporal_bundle_shape(bundle: &TemporalBundleEvidence) -> Result<()> {
    bundle.bundle.validate("temporal bundle")?;
    if bundle.bundle.kind != EvidenceReferenceKind::Artifact(ArtifactKind::TemporalDebugBundle) {
        return Err(ContractError::new(
            "temporal bundle handle must use the temporal-debug-bundle artifact kind",
        ));
    }
    validate_nonempty_artifact_shapes(
        &bundle.before_during_after,
        ArtifactKind::BeforeDuringAfter,
        "before/during/after",
    )?;
    validate_nonempty_artifact_shapes(
        &bundle.storyboards,
        ArtifactKind::ChangeAwareStoryboard,
        "temporal bundle storyboard",
    )?;
    validate_nonempty_artifact_shapes(
        &bundle.difference_maps,
        ArtifactKind::DifferenceMap,
        "temporal bundle difference map",
    )?;
    bundle.capture_summary.validate("capture summary")?;
    if bundle.capture_summary.kind != EvidenceReferenceKind::CaptureSummary {
        return Err(ContractError::new(
            "temporal bundle capture summary has the wrong evidence kind",
        ));
    }
    bundle.context_summary.validate("context summary")?;
    if bundle.context_summary.kind != EvidenceReferenceKind::ContextSummary {
        return Err(ContractError::new(
            "temporal bundle context summary has the wrong evidence kind",
        ));
    }
    for reference in &bundle.evidence_references {
        reference.validate("temporal bundle evidence reference")?;
    }
    validate_unique_reference_ids(bundle)
}

fn validate_nonempty_artifact_shapes(
    artifacts: &[ArtifactEvidenceReference],
    kind: ArtifactKind,
    label: &str,
) -> Result<()> {
    if artifacts.is_empty() {
        return Err(ContractError::new(format!("{label} outcome is missing")));
    }
    for artifact in artifacts {
        artifact.validate_shape(label)?;
        artifact.validate_authority(kind, label)?;
        if artifact.output.kind != EvidenceReferenceKind::Artifact(kind) {
            return Err(ContractError::new(format!(
                "{label} has the wrong artifact kind"
            )));
        }
    }
    Ok(())
}

fn validate_evidence_shape(evidence: &ConditionEvidence, condition_id: ConditionId) -> Result<()> {
    let matches = match (condition_id, evidence) {
        (
            ConditionId::AFinalScreenshot,
            ConditionEvidence::FinalScreenshot {
                current_observation,
                ..
            },
        ) => {
            current_observation.validate("condition A current observation")?;
            current_observation.kind == EvidenceReferenceKind::CurrentObservation
        }
        (
            ConditionId::BUniformStoryboard,
            ConditionEvidence::UniformStoryboard { slot_frame_ids },
        ) => {
            slot_frame_ids.len() == UNIFORM_SOURCE_FRAME_SLOTS
                && slot_frame_ids.windows(2).all(|pair| pair[0] != pair[1])
                && slot_frame_ids
                    .iter()
                    .all(|id| privacy::validate_opaque_id(id, "condition B slot id").is_ok())
        }
        (
            ConditionId::CChangeAwareStoryboard,
            ConditionEvidence::ChangeAwareStoryboard { artifacts },
        ) => {
            !artifacts.is_empty()
                && artifacts.iter().all(|artifact| {
                    artifact.validate_shape("condition C storyboard").is_ok()
                        && artifact
                            .validate_authority(
                                ArtifactKind::ChangeAwareStoryboard,
                                "condition C storyboard",
                            )
                            .is_ok()
                })
        }
        (ConditionId::DTemporalBundle, ConditionEvidence::TemporalBundle(bundle)) => {
            validate_temporal_bundle_shape(bundle).is_ok()
        }
        (ConditionId::EProgressiveSource, ConditionEvidence::ProgressiveSource(evidence)) => {
            validate_temporal_bundle_shape(&evidence.bundle).is_ok()
                && evidence.source_retrievals.len() <= 2
        }
        _ => false,
    };
    if matches {
        Ok(())
    } else {
        Err(ContractError::new(
            "condition evidence does not match its registered condition",
        ))
    }
}

fn validate_evidence(
    evidence: &ConditionEvidence,
    condition_id: ConditionId,
    interval: &SourceInterval,
) -> Result<()> {
    match (condition_id, evidence) {
        (
            ConditionId::AFinalScreenshot,
            ConditionEvidence::FinalScreenshot {
                final_frame_id,
                current_observation,
            },
        ) => {
            let final_frame = interval
                .retained_frames()
                .last()
                .ok_or_else(|| ContractError::new("condition A has no retained final frame"))?;
            if final_frame.id != *final_frame_id {
                return Err(ContractError::new(
                    "condition A final frame is not the last retained interval frame",
                ));
            }
            current_observation.validate("condition A current observation")?;
            if current_observation.kind != EvidenceReferenceKind::CurrentObservation {
                return Err(ContractError::new(
                    "condition A current observation has the wrong evidence kind",
                ));
            }
            Ok(())
        }
        (
            ConditionId::BUniformStoryboard,
            ConditionEvidence::UniformStoryboard { slot_frame_ids },
        ) => {
            if slot_frame_ids.len() != UNIFORM_SOURCE_FRAME_SLOTS {
                return Err(ContractError::new(
                    "condition B must contain exactly eight uniform source slots",
                ));
            }
            let retained = interval.retained_frames().collect::<Vec<_>>();
            if retained.len() < UNIFORM_SOURCE_FRAME_SLOTS {
                return Err(ContractError::new(
                    "condition B is unavailable below eight retained source frames",
                ));
            }
            let expected = (0..UNIFORM_SOURCE_FRAME_SLOTS)
                .map(|slot| {
                    let last = retained.len() - 1;
                    retained[((slot as u128 * last as u128) / 7) as usize]
                        .id
                        .clone()
                })
                .collect::<Vec<_>>();
            if *slot_frame_ids != expected {
                return Err(ContractError::new(
                    "condition B slots do not match integer uniform selection",
                ));
            }
            if slot_frame_ids.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(ContractError::new("condition B slots must be distinct"));
            }
            Ok(())
        }
        (
            ConditionId::CChangeAwareStoryboard,
            ConditionEvidence::ChangeAwareStoryboard { artifacts },
        ) => {
            if artifacts.is_empty() {
                return Err(ContractError::new(
                    "condition C must retain storyboard evidence",
                ));
            }
            for artifact in artifacts {
                artifact.validate_for_kind(
                    interval,
                    ArtifactKind::ChangeAwareStoryboard,
                    "condition C storyboard",
                )?;
            }
            Ok(())
        }
        (ConditionId::DTemporalBundle, ConditionEvidence::TemporalBundle(bundle)) => {
            bundle.validate(interval)
        }
        (ConditionId::EProgressiveSource, ConditionEvidence::ProgressiveSource(evidence)) => {
            evidence.bundle.validate(interval)?;
            if evidence.source_retrievals.len() > 2 {
                return Err(ContractError::new(
                    "condition E exceeds the two source-frame retrieval budget",
                ));
            }
            let mut request_ids = HashSet::new();
            for retrieval in &evidence.source_retrievals {
                if !request_ids.insert(&retrieval.request_id) {
                    return Err(ContractError::new(
                        "condition E retrieval request ids must be unique",
                    ));
                }
                retrieval.validate(interval)?;
            }
            if let Some(filmstrip) = &evidence.region_filmstrip {
                filmstrip.validate_for_kind(
                    interval,
                    ArtifactKind::RegionFilmstrip,
                    "condition E fixed-region filmstrip",
                )?;
                if filmstrip.algorithm_versions.len() != 1
                    || filmstrip.algorithm_versions[0].name != REGION_FILMSTRIP_NAME
                    || filmstrip.algorithm_versions[0].version != REGION_FILMSTRIP_VERSION
                {
                    return Err(ContractError::new(
                        "condition E filmstrip must use the existing fixed-region authority",
                    ));
                }
            }
            Ok(())
        }
        _ => Err(ContractError::new(
            "condition evidence does not match its registered condition",
        )),
    }
}

fn validate_nonempty_artifacts(
    artifacts: &[ArtifactEvidenceReference],
    interval: &SourceInterval,
    kind: ArtifactKind,
    label: &str,
) -> Result<()> {
    if artifacts.is_empty() {
        return Err(ContractError::new(format!("{label} outcome is missing")));
    }
    for artifact in artifacts {
        artifact.validate_for_kind(interval, kind, label)?;
    }
    Ok(())
}

fn artifact_authority(kind: ArtifactKind) -> Option<(&'static str, &'static str)> {
    match kind {
        ArtifactKind::BeforeDuringAfter | ArtifactKind::ChangeAwareStoryboard => {
            Some((STORYBOARD_NAME, STORYBOARD_VERSION))
        }
        ArtifactKind::DifferenceMap => Some((DIFFERENCE_MAP_NAME, DIFFERENCE_MAP_VERSION)),
        ArtifactKind::RegionFilmstrip => Some((REGION_FILMSTRIP_NAME, REGION_FILMSTRIP_VERSION)),
        ArtifactKind::FinalScreenshot
        | ArtifactKind::UniformStoryboard
        | ArtifactKind::SourceFrame
        | ArtifactKind::TemporalDebugBundle => None,
    }
}

fn validate_unique_artifact_outputs(artifacts: &[ArtifactEvidenceReference]) -> Result<()> {
    let mut ids = HashSet::with_capacity(artifacts.len());
    for artifact in artifacts {
        if !ids.insert(&artifact.output.id) {
            return Err(ContractError::new(
                "condition artifact output identities must be unique",
            ));
        }
    }
    Ok(())
}

fn validate_named_version(value: &NamedVersion, label: &str) -> Result<()> {
    privacy::validate_safe_text(
        &value.name,
        &format!("{label} name"),
        privacy::MAX_SHORT_TEXT,
    )?;
    privacy::validate_safe_text(
        &value.version,
        &format!("{label} version"),
        privacy::MAX_SHORT_TEXT,
    )
}

fn validate_exact_ids(actual: &[String], expected: &[String], label: &str) -> Result<()> {
    validate_ordered_unique_ids(actual, label)?;
    if actual != expected {
        return Err(ContractError::new(format!(
            "{label} do not match the authority order"
        )));
    }
    Ok(())
}

fn validate_ordered_unique_ids(ids: &[String], label: &str) -> Result<()> {
    let mut seen = HashSet::with_capacity(ids.len());
    for id in ids {
        privacy::validate_opaque_id(id, label)?;
        if !seen.insert(id) {
            return Err(ContractError::new(format!("{label} must be unique")));
        }
    }
    Ok(())
}

fn validate_ordered_subset(source: &[String], selected: &[String], label: &str) -> Result<()> {
    validate_ordered_unique_ids(selected, label)?;
    if !is_subsequence(source, selected) {
        return Err(ContractError::new(format!(
            "{label} must preserve authority order"
        )));
    }
    Ok(())
}

fn is_subsequence(source: &[String], selected: &[String]) -> bool {
    let mut position = 0;
    selected.iter().all(|id| {
        let Some(offset) = source[position..]
            .iter()
            .position(|candidate| candidate == id)
        else {
            return false;
        };
        position += offset + 1;
        true
    })
}

fn validate_unique_reference_ids(bundle: &TemporalBundleEvidence) -> Result<()> {
    let mut ids = HashSet::new();
    let mut check = |reference: &EvidenceReference| {
        if !ids.insert(reference.id.clone()) {
            Err(ContractError::new(
                "temporal bundle evidence reference ids must be unique",
            ))
        } else {
            Ok(())
        }
    };
    check(&bundle.bundle)?;
    check(&bundle.capture_summary)?;
    check(&bundle.context_summary)?;
    for reference in &bundle.evidence_references {
        check(reference)?;
    }
    for artifact in bundle
        .before_during_after
        .iter()
        .chain(&bundle.storyboards)
        .chain(&bundle.difference_maps)
    {
        check(&artifact.output)?;
    }
    Ok(())
}

fn validate_package_privacy(package: &ConditionPackage) -> Result<()> {
    privacy::sanitize_serialized(package)?;
    let serialized = serde_json::to_string(package)?.to_ascii_lowercase();
    for forbidden in [
        "base64",
        "data:image",
        "ground truth",
        "raw answer",
        "page text",
        "model answer",
        "image payload",
    ] {
        if serialized.contains(forbidden) {
            return Err(ContractError::new(
                "condition package contains forbidden payload or claim text",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GapEvidence, ScopeIdentity, SourceFrameEvidence, TimeRangeNs};

    fn interval() -> SourceInterval {
        let frames = (0..10)
            .map(|index| SourceFrameEvidence {
                id: format!("frame-{index}"),
                capture_ordinal: index + 1,
                source_time_ns: Some(index),
                observed_time_ns: index + 100,
                session_time_ns: index * 10,
                encoded_sha256: format!("sha256:{:0>64}", index + 1),
                availability: EvidenceAvailability::Retained,
            })
            .collect();
        SourceInterval::new(
            "interval-1",
            ScopeIdentity::new("session-1", "target-1").unwrap(),
            TimeRangeNs::new(0, 90).unwrap(),
            TimeRangeNs::new(0, 90).unwrap(),
            40,
            frames,
            Vec::new(),
            RetentionState::Retained,
        )
        .unwrap()
    }

    fn current() -> EvidenceReference {
        EvidenceReference::new(
            "observation-1",
            EvidenceReferenceKind::CurrentObservation,
            Some(format!("sha256:{:0>64}", 99)),
            EvidenceAvailability::Retained,
        )
        .unwrap()
    }

    #[test]
    fn all_packages_share_one_interval_and_repeated_bytes() {
        let interval = interval();
        let a = ConditionPackager::final_screenshot(&interval, "frame-9", current()).unwrap();
        let b = ConditionPackager::uniform_storyboard(&interval).unwrap();
        assert_eq!(
            require_one_source_interval(&[a.clone(), b.clone()]).unwrap(),
            interval.digest().unwrap()
        );
        assert_eq!(a.canonical_bytes().unwrap(), a.canonical_bytes().unwrap());
        assert_eq!(b.canonical_bytes().unwrap(), b.canonical_bytes().unwrap());
        let slots = match b.evidence {
            ConditionEvidence::UniformStoryboard { slot_frame_ids } => slot_frame_ids,
            _ => unreachable!(),
        };
        assert_eq!(
            slots,
            vec![
                "frame-0", "frame-1", "frame-2", "frame-3", "frame-5", "frame-6", "frame-7",
                "frame-9"
            ]
        );
    }

    #[test]
    fn source_interval_rejects_overlapping_gaps_and_bad_retention() {
        let frames = vec![SourceFrameEvidence {
            id: "frame-1".into(),
            capture_ordinal: 1,
            source_time_ns: None,
            observed_time_ns: 10,
            session_time_ns: 1,
            encoded_sha256: format!("sha256:{:0>64}", 1),
            availability: EvidenceAvailability::Retained,
        }];
        let result = SourceInterval::new(
            "interval-1",
            ScopeIdentity::new("session-1", "target-1").unwrap(),
            TimeRangeNs::new(0, 10).unwrap(),
            TimeRangeNs::new(0, 10).unwrap(),
            1,
            frames,
            vec![
                GapEvidence::new("gap-1", 1, 5, "capture gap", None).unwrap(),
                GapEvidence::new("gap-2", 5, 8, "capture gap", None).unwrap(),
            ],
            RetentionState::Retained,
        );
        assert!(result.is_err());
    }
}
