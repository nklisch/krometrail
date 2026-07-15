use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    AnswerValidationContext, ArtifactCacheIdentity, ArtifactEvidenceReference, ArtifactKind,
    BENCHMARK_ID, CaseFamily, ConditionAggregate, ConditionEvidence, ConditionId, ConditionPackage,
    ContractError, EvaluationStatus, EvidenceAvailability, EvidenceReference,
    EvidenceReferenceKind, FailureRecord, FamilyThresholdCheck, NamedVersion, NonClaimId,
    RetentionState, SCORER_VERSION, ScorerIdentity, SourceFrameAvailability, SourceInterval,
    ThresholdAssessment, ThresholdCheck, ThresholdProfile, TrialIdentity, TrialScore,
    aggregate_condition, assess_thresholds, canonical_json, privacy, require_one_source_interval,
    sha256_prefixed, validate_interpretation_answer,
};

pub const RESULT_SCHEMA_VERSION: u16 = 1;
pub const RESULT_KIND: &str = "temporal_benchmark_evaluation_result";

const ZERO_REVISION: &str = "0000000000000000000000000000000000000000";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceLayer {
    DeterministicCi,
    LiveCapture,
    ManualModel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ThesisEligibility {
    Eligible,
    NotEligible,
    Inconclusive,
}

/// The bounded evidence projection retained by a result record.
///
/// Artifact manifests, source payloads, cache rows, and model prose stay in their owning
/// authorities. This record keeps only the identities needed to audit a score.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvidenceTraceRecord {
    pub id: String,
    pub kind: EvidenceReferenceKind,
    pub availability: EvidenceAvailability,
    pub output_sha256: Option<String>,
    pub manifest_sha256: Option<String>,
    pub source_frame_ids: Vec<String>,
    pub selected_frame_ids: Vec<String>,
    pub gap_ids: Vec<String>,
    pub algorithm_versions: Vec<NamedVersion>,
    pub cache: Option<ArtifactCacheIdentity>,
}

impl EvidenceTraceRecord {
    fn validate(&self, source_frame_ids: &[String], gap_ids: &[String]) -> crate::Result<()> {
        privacy::validate_opaque_id(&self.id, "result evidence id")?;
        if let Some(hash) = &self.output_sha256 {
            privacy::validate_sha256(hash, "result evidence output hash")?;
        }
        if let Some(hash) = &self.manifest_sha256 {
            privacy::validate_sha256(hash, "result evidence manifest hash")?;
        }
        validate_unique_ids(&self.source_frame_ids, "result evidence source frame ids")?;
        validate_unique_ids(
            &self.selected_frame_ids,
            "result evidence selected frame ids",
        )?;
        validate_unique_ids(&self.gap_ids, "result evidence gap ids")?;
        if self
            .source_frame_ids
            .iter()
            .any(|id| !source_frame_ids.contains(id))
            || self
                .selected_frame_ids
                .iter()
                .any(|id| !source_frame_ids.contains(id))
        {
            return Err(ContractError::new(
                "result evidence references a source frame outside its interval",
            ));
        }
        if self.gap_ids.iter().any(|id| !gap_ids.contains(id)) {
            return Err(ContractError::new(
                "result evidence references a gap outside its interval",
            ));
        }
        if !is_subsequence(source_frame_ids, &self.selected_frame_ids) {
            return Err(ContractError::new(
                "result evidence selected source ids must preserve interval order",
            ));
        }
        if let Some(cache) = &self.cache {
            cache.validate()?;
        }
        for (index, version) in self.algorithm_versions.iter().enumerate() {
            validate_named_version(version, &format!("result algorithm version {index}"))?;
        }
        match &self.kind {
            EvidenceReferenceKind::SourceFrame => {
                if self.manifest_sha256.is_some()
                    || !self.source_frame_ids.is_empty()
                    || !self.selected_frame_ids.is_empty()
                    || !self.gap_ids.is_empty()
                    || !self.algorithm_versions.is_empty()
                    || self.cache.is_some()
                {
                    return Err(ContractError::new(
                        "source-frame result evidence may carry only its direct output identity",
                    ));
                }
            }
            EvidenceReferenceKind::Artifact(kind) => {
                let is_bundle_handle = *kind == ArtifactKind::TemporalDebugBundle
                    && self.manifest_sha256.is_none()
                    && self.cache.is_none()
                    && self.algorithm_versions.is_empty();
                if is_bundle_handle {
                    if !self.source_frame_ids.is_empty()
                        || !self.selected_frame_ids.is_empty()
                        || !self.gap_ids.is_empty()
                    {
                        return Err(ContractError::new(
                            "direct bundle evidence cannot carry an artifact projection",
                        ));
                    }
                } else if self.manifest_sha256.is_none()
                    || self.cache.is_none()
                    || self.algorithm_versions.is_empty()
                    || self.source_frame_ids.is_empty()
                {
                    return Err(ContractError::new(
                        "artifact result evidence must preserve manifest, source, version, and cache identities",
                    ));
                }
            }
            EvidenceReferenceKind::CurrentObservation
            | EvidenceReferenceKind::CaptureSummary
            | EvidenceReferenceKind::ContextSummary
            | EvidenceReferenceKind::ProgressiveRequest => {
                if self.manifest_sha256.is_some()
                    || !self.source_frame_ids.is_empty()
                    || !self.selected_frame_ids.is_empty()
                    || !self.gap_ids.is_empty()
                    || !self.algorithm_versions.is_empty()
                    || self.cache.is_some()
                {
                    return Err(ContractError::new(
                        "non-artifact result evidence may carry only its direct output identity",
                    ));
                }
            }
        }
        if self.availability == EvidenceAvailability::Retained
            && self.output_sha256.is_none()
            && !matches!(
                self.kind,
                EvidenceReferenceKind::SourceFrame | EvidenceReferenceKind::ProgressiveRequest
            )
        {
            return Err(ContractError::new(
                "retained result evidence must preserve its output hash",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrialResultRecord {
    pub score: TrialScore,
    pub package_digest: String,
    pub source_interval_digest: String,
    pub source_frame_ids: Vec<String>,
    pub source_frame_availability: Vec<SourceFrameAvailability>,
    pub gap_ids: Vec<String>,
    pub retention: RetentionState,
    pub evidence_ids: Vec<String>,
    pub evidence: Vec<EvidenceTraceRecord>,
    pub cache_keys: Vec<String>,
}

impl TrialResultRecord {
    pub fn validate(&self) -> crate::Result<()> {
        self.score.validate()?;
        if self.package_digest != self.score.package_digest
            || self.source_interval_digest != self.score.source_interval_digest
        {
            return Err(ContractError::new(
                "trial result identity disagrees with its score",
            ));
        }
        privacy::validate_sha256(&self.package_digest, "trial result package digest")?;
        privacy::validate_sha256(
            &self.source_interval_digest,
            "trial result source interval digest",
        )?;
        validate_unique_ids(&self.source_frame_ids, "trial result source frame ids")?;
        if self.source_frame_ids.is_empty() {
            return Err(ContractError::new(
                "trial result must preserve source frame identities",
            ));
        }
        validate_source_frame_availability(
            &self.source_frame_ids,
            &self.source_frame_availability,
            "trial result source frame availability",
        )?;
        if self.retention != expected_retention(&self.source_frame_availability) {
            return Err(ContractError::new(
                "trial result retention contradicts per-frame availability",
            ));
        }
        validate_unique_ids(&self.gap_ids, "trial result gap ids")?;
        let mut trace_ids = Vec::with_capacity(self.evidence.len());
        for trace in &self.evidence {
            trace.validate(&self.source_frame_ids, &self.gap_ids)?;
            if trace_ids.iter().all(|id| id != &trace.id) {
                trace_ids.push(trace.id.clone());
            } else {
                return Err(ContractError::new(
                    "trial result evidence identifiers must be unique",
                ));
            }
            if trace.kind == EvidenceReferenceKind::SourceFrame
                && !self.source_frame_ids.contains(&trace.id)
            {
                return Err(ContractError::new(
                    "source-frame result evidence is outside its source interval",
                ));
            }
            if trace.kind == EvidenceReferenceKind::SourceFrame {
                let expected = self
                    .source_frame_availability
                    .iter()
                    .find(|record| record.id == trace.id)
                    .ok_or_else(|| {
                        ContractError::new(
                            "source-frame result evidence is outside its source availability proof",
                        )
                    })?;
                if trace.availability != expected.availability {
                    return Err(ContractError::new(
                        "source-frame result evidence availability contradicts its exact proof",
                    ));
                }
            }
        }
        if self.evidence_ids != trace_ids {
            return Err(ContractError::new(
                "trial result evidence ids must preserve trace order",
            ));
        }
        for source_id in &self.source_frame_ids {
            if !self.evidence.iter().any(|trace| {
                trace.kind == EvidenceReferenceKind::SourceFrame && trace.id == *source_id
            }) {
                return Err(ContractError::new(
                    "trial result must retain every source identity in its evidence trace",
                ));
            }
        }
        validate_unique_hashes(&self.cache_keys, "trial result cache keys")?;
        let expected_cache_keys = self
            .evidence
            .iter()
            .filter_map(|trace| trace.cache.as_ref().map(|cache| cache.cache_key.clone()))
            .collect::<Vec<_>>();
        if self.cache_keys != expected_cache_keys {
            return Err(ContractError::new(
                "trial result cache keys do not match evidence trace identities",
            ));
        }

        validate_interpretation_answer(
            &self.score.answer,
            AnswerValidationContext {
                unresolved_capture_gap: false,
                missing_source: false,
            },
        )?;
        for reference in self.score.answer.evidence_refs.iter().chain(
            self.score
                .dimensions
                .iter()
                .flat_map(|dimension| dimension.evidence_ids.iter()),
        ) {
            require_retained_trace(self, reference)?;
        }
        for claim in &self.score.accepted_claims {
            for evidence_id in &claim.evidence_ids {
                require_retained_trace(self, evidence_id)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EvaluationResultRecord {
    pub schema_version: u16,
    pub kind: String,
    pub benchmark_id: String,
    pub run_manifest_input_digest: String,
    pub scorer: ScorerIdentity,
    pub evidence_layer: EvidenceLayer,
    pub thesis_eligibility: ThesisEligibility,
    pub threshold_profile: ThresholdProfile,
    pub trials: Vec<TrialResultRecord>,
    pub conditions: Vec<ConditionAggregate>,
    pub thresholds: ThresholdAssessment,
    pub status: EvaluationStatus,
    pub non_claims: Vec<NonClaimId>,
    pub failure: Option<FailureRecord>,
}

impl EvaluationResultRecord {
    pub fn from_scores(
        manifest_input_digest: String,
        evidence_layer: EvidenceLayer,
        packages: &[ConditionPackage],
        scores: &[TrialScore],
        aggregates: Vec<ConditionAggregate>,
        thresholds: ThresholdAssessment,
    ) -> crate::Result<Self> {
        let profile = ThresholdProfile::canonical();
        profile.validate()?;
        privacy::validate_sha256(&manifest_input_digest, "result manifest input digest")?;
        if packages.is_empty() || scores.is_empty() {
            return Err(ContractError::new(
                "result construction requires packages and trial scores",
            ));
        }
        require_package_order(packages)?;
        let interval_digest = require_one_source_interval(packages)?;
        let package_map = packages
            .iter()
            .map(|package| (package.condition_id, package))
            .collect::<BTreeMap<_, _>>();
        let mut seen_trials = BTreeSet::new();
        for score in scores {
            score.validate()?;
            if !seen_trials.insert(score.trial_id.clone()) {
                return Err(ContractError::new(
                    "result scores contain duplicate trial identities",
                ));
            }
            let package = package_map.get(&score.condition_id).ok_or_else(|| {
                ContractError::new("result score has no package for its condition")
            })?;
            if score.package_digest != package.digest
                || score.source_interval_digest != interval_digest
            {
                return Err(ContractError::new(
                    "result score is not bound to the exact condition package and interval",
                ));
            }
        }

        let mut grouped = BTreeMap::<ConditionId, Vec<TrialScore>>::new();
        for score in scores {
            grouped
                .entry(score.condition_id)
                .or_default()
                .push(score.clone());
        }
        require_aggregate_order(&aggregates)?;
        if aggregates
            .iter()
            .map(|aggregate| aggregate.condition_id)
            .collect::<BTreeSet<_>>()
            != grouped.keys().copied().collect::<BTreeSet<_>>()
        {
            return Err(ContractError::new(
                "result aggregates must bind exactly the supplied score conditions",
            ));
        }
        for aggregate in &aggregates {
            let expected = aggregate_condition(
                aggregate.condition_id,
                grouped
                    .get(&aggregate.condition_id)
                    .expect("aggregate condition set was checked"),
                &profile,
            )?;
            if *aggregate != expected {
                return Err(ContractError::new(
                    "result aggregate does not match its bound trial scores",
                ));
            }
        }
        let expected_thresholds = assess_thresholds(&aggregates, packages, &profile)?;
        if thresholds != expected_thresholds {
            return Err(ContractError::new(
                "result thresholds do not match its bound aggregates and packages",
            ));
        }

        let trials = scores
            .iter()
            .map(|score| {
                let package = package_map
                    .get(&score.condition_id)
                    .expect("score package set was checked");
                trial_result(score, package)
            })
            .collect::<crate::Result<Vec<_>>>()?;
        let record = Self {
            schema_version: RESULT_SCHEMA_VERSION,
            kind: RESULT_KIND.into(),
            benchmark_id: BENCHMARK_ID.into(),
            run_manifest_input_digest: manifest_input_digest,
            scorer: ScorerIdentity {
                git_revision: ZERO_REVISION.into(),
                version: SCORER_VERSION.into(),
            },
            thesis_eligibility: match evidence_layer {
                EvidenceLayer::DeterministicCi => ThesisEligibility::NotEligible,
                EvidenceLayer::LiveCapture | EvidenceLayer::ManualModel => {
                    // This constructor only binds deterministic scorer inputs; later live/model
                    // lanes must supply their own authorization and qualification evidence.
                    ThesisEligibility::Inconclusive
                }
            },
            evidence_layer,
            threshold_profile: profile,
            trials,
            conditions: aggregates,
            status: thresholds.status,
            non_claims: NonClaimId::ALL.to_vec(),
            failure: thresholds.failure.clone(),
            thresholds,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn validate(&self) -> crate::Result<()> {
        if self.schema_version != RESULT_SCHEMA_VERSION {
            return Err(ContractError::new(
                "unsupported evaluation result schema version",
            ));
        }
        if self.kind != RESULT_KIND {
            return Err(ContractError::new(
                "evaluation result kind is not current v1",
            ));
        }
        if self.benchmark_id != BENCHMARK_ID {
            return Err(ContractError::new("evaluation result benchmark is unknown"));
        }
        privacy::validate_sha256(
            &self.run_manifest_input_digest,
            "result manifest input digest",
        )?;
        privacy::validate_git_revision(&self.scorer.git_revision, "result scorer revision")?;
        if self.scorer.version != SCORER_VERSION {
            return Err(ContractError::new(
                "evaluation result scorer version is not current v1",
            ));
        }
        self.threshold_profile.validate()?;
        if self.non_claims != NonClaimId::ALL {
            return Err(ContractError::new(
                "evaluation result must carry the fixed non-claim registry",
            ));
        }
        if self.trials.is_empty() {
            return Err(ContractError::new(
                "evaluation result must contain trial records",
            ));
        }
        let mut trial_ids = BTreeSet::new();
        for trial in &self.trials {
            trial.validate()?;
            if !trial_ids.insert(&trial.score.trial_id) {
                return Err(ContractError::new(
                    "evaluation result trial identifiers must be unique",
                ));
            }
        }
        require_aggregate_order(&self.conditions)?;
        for aggregate in &self.conditions {
            aggregate.validate()?;
        }
        let mut grouped = BTreeMap::<ConditionId, Vec<TrialScore>>::new();
        for trial in &self.trials {
            grouped
                .entry(trial.score.condition_id)
                .or_default()
                .push(trial.score.clone());
        }
        if self
            .conditions
            .iter()
            .map(|aggregate| aggregate.condition_id)
            .collect::<BTreeSet<_>>()
            != grouped.keys().copied().collect::<BTreeSet<_>>()
        {
            return Err(ContractError::new(
                "result conditions do not cover exactly its trial conditions",
            ));
        }
        for aggregate in &self.conditions {
            let expected = aggregate_condition(
                aggregate.condition_id,
                grouped
                    .get(&aggregate.condition_id)
                    .expect("condition set was checked"),
                &self.threshold_profile,
            )?;
            if *aggregate != expected {
                return Err(ContractError::new(
                    "result condition aggregate contradicts trial dimensions",
                ));
            }
        }
        validate_threshold_assessment(&self.thresholds, &self.threshold_profile)?;
        if self.status != self.thresholds.status
            || self.status
                != status_precedence(&threshold_statuses(&self.thresholds, &self.conditions))
        {
            return Err(ContractError::new(
                "result status contradicts aggregate or threshold statuses",
            ));
        }
        validate_status_failure(self.status, self.failure.as_ref())?;
        if self.failure != self.thresholds.failure {
            return Err(ContractError::new(
                "result failure does not match threshold failure",
            ));
        }
        match (self.evidence_layer, self.thesis_eligibility, self.status) {
            (EvidenceLayer::DeterministicCi, ThesisEligibility::NotEligible, _) => {}
            (EvidenceLayer::DeterministicCi, _, _) => {
                return Err(ContractError::new(
                    "deterministic-CI results must be thesis-ineligible",
                ));
            }
            (_, ThesisEligibility::Eligible, EvaluationStatus::Pass) => {}
            (_, ThesisEligibility::Eligible, _) => {
                return Err(ContractError::new(
                    "only a passing result may be thesis-eligible",
                ));
            }
            (_, ThesisEligibility::NotEligible | ThesisEligibility::Inconclusive, _) => {}
        }
        Ok(())
    }

    pub fn from_canonical_json(bytes: &[u8]) -> crate::Result<Self> {
        let result: Self = serde_json::from_slice(bytes)?;
        result.validate()?;
        if canonical_json(&result)? != bytes {
            return Err(ContractError::new(
                "evaluation result is not in canonical form",
            ));
        }
        Ok(result)
    }

    pub fn canonical_bytes(&self) -> crate::Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn digest(&self) -> crate::Result<String> {
        Ok(sha256_prefixed(&self.canonical_bytes()?))
    }
}

fn trial_result(
    score: &TrialScore,
    package: &ConditionPackage,
) -> crate::Result<TrialResultRecord> {
    let mut builder = TraceBuilder::new(package);
    builder.add_package_evidence(&package.evidence)?;
    let evidence = builder.finish()?;
    let evidence_ids = evidence
        .iter()
        .map(|trace| trace.id.clone())
        .collect::<Vec<_>>();
    let cache_keys = evidence
        .iter()
        .filter_map(|trace| trace.cache.as_ref().map(|cache| cache.cache_key.clone()))
        .collect::<Vec<_>>();
    let result = TrialResultRecord {
        score: score.clone(),
        package_digest: package.digest.clone(),
        source_interval_digest: package.source_interval_digest.clone(),
        source_frame_ids: package.source_frame_ids.clone(),
        source_frame_availability: package.source_frame_availability.clone(),
        gap_ids: package.gap_ids.clone(),
        retention: package.retention,
        evidence_ids,
        evidence,
        cache_keys,
    };
    result.validate()?;
    for claim in &result.score.accepted_claims {
        for evidence_id in &claim.evidence_ids {
            require_retained_trace(&result, evidence_id)?;
        }
    }
    Ok(result)
}

struct TraceBuilder {
    source_frame_ids: Vec<String>,
    source_frame_availability: BTreeMap<String, EvidenceAvailability>,
    gap_ids: Vec<String>,
    traces: Vec<EvidenceTraceRecord>,
}

impl TraceBuilder {
    fn new(package: &ConditionPackage) -> Self {
        Self {
            source_frame_ids: package.source_frame_ids.clone(),
            source_frame_availability: package
                .source_frame_availability
                .iter()
                .map(|record| (record.id.clone(), record.availability))
                .collect(),
            gap_ids: package.gap_ids.clone(),
            traces: Vec::new(),
        }
    }

    fn add_package_evidence(&mut self, evidence: &ConditionEvidence) -> crate::Result<()> {
        for id in self.source_frame_ids.clone() {
            let availability = self
                .source_frame_availability
                .get(&id)
                .copied()
                .ok_or_else(|| ContractError::new("source frame proof is incomplete"))?;
            self.add_source_frame(&id, availability)?;
        }
        match evidence {
            ConditionEvidence::FinalScreenshot {
                final_frame_id,
                current_observation,
            } => {
                let availability = self
                    .source_frame_availability
                    .get(final_frame_id)
                    .copied()
                    .ok_or_else(|| {
                        ContractError::new("condition A final frame proof is missing")
                    })?;
                self.add_source_frame(final_frame_id, availability)?;
                self.add_reference(current_observation)?;
            }
            ConditionEvidence::UniformStoryboard { slot_frame_ids } => {
                for id in slot_frame_ids {
                    let availability =
                        self.source_frame_availability
                            .get(id)
                            .copied()
                            .ok_or_else(|| {
                                ContractError::new("condition B source frame proof is missing")
                            })?;
                    self.add_source_frame(id, availability)?;
                }
            }
            ConditionEvidence::ChangeAwareStoryboard { artifacts } => {
                for artifact in artifacts {
                    self.add_artifact(artifact)?;
                }
            }
            ConditionEvidence::TemporalBundle(bundle) => self.add_bundle(bundle)?,
            ConditionEvidence::ProgressiveSource(progress) => {
                self.add_bundle(&progress.bundle)?;
                for retrieval in &progress.source_retrievals {
                    let availability = if retrieval.unavailable_frame_ids.is_empty()
                        && retrieval.returned_frames.iter().all(|reference| {
                            reference.availability == EvidenceAvailability::Retained
                        }) {
                        EvidenceAvailability::Retained
                    } else {
                        EvidenceAvailability::NotCollected
                    };
                    self.add_trace(EvidenceTraceRecord {
                        id: retrieval.request_id.clone(),
                        kind: EvidenceReferenceKind::ProgressiveRequest,
                        availability,
                        output_sha256: None,
                        manifest_sha256: None,
                        source_frame_ids: Vec::new(),
                        selected_frame_ids: Vec::new(),
                        gap_ids: Vec::new(),
                        algorithm_versions: Vec::new(),
                        cache: None,
                    })?;
                    for reference in &retrieval.returned_frames {
                        self.add_reference(reference)?;
                    }
                }
                if let Some(filmstrip) = &progress.region_filmstrip {
                    self.add_artifact(filmstrip)?;
                }
            }
        }
        Ok(())
    }

    fn add_bundle(&mut self, bundle: &crate::TemporalBundleEvidence) -> crate::Result<()> {
        self.add_reference(&bundle.bundle)?;
        for artifact in &bundle.before_during_after {
            self.add_artifact(artifact)?;
        }
        for artifact in &bundle.storyboards {
            self.add_artifact(artifact)?;
        }
        for artifact in &bundle.difference_maps {
            self.add_artifact(artifact)?;
        }
        self.add_reference(&bundle.capture_summary)?;
        self.add_reference(&bundle.context_summary)?;
        for reference in &bundle.evidence_references {
            self.add_reference(reference)?;
        }
        Ok(())
    }

    fn add_artifact(&mut self, artifact: &ArtifactEvidenceReference) -> crate::Result<()> {
        self.add_trace(EvidenceTraceRecord {
            id: artifact.output.id.clone(),
            kind: artifact.output.kind,
            availability: artifact.output.availability,
            output_sha256: artifact.output.sha256.clone(),
            manifest_sha256: Some(artifact.manifest_sha256.clone()),
            source_frame_ids: artifact.source_frame_ids.clone(),
            selected_frame_ids: artifact.selected_frame_ids.clone(),
            gap_ids: artifact.gap_ids.clone(),
            algorithm_versions: artifact.algorithm_versions.clone(),
            cache: Some(artifact.cache.clone()),
        })
    }

    fn add_source_frame(
        &mut self,
        id: &str,
        availability: EvidenceAvailability,
    ) -> crate::Result<()> {
        if self.source_frame_availability.get(id).copied() != Some(availability) {
            return Err(ContractError::new(
                "source-frame trace availability does not match package proof",
            ));
        }
        self.add_trace(EvidenceTraceRecord {
            id: id.into(),
            kind: EvidenceReferenceKind::SourceFrame,
            availability,
            output_sha256: None,
            manifest_sha256: None,
            source_frame_ids: Vec::new(),
            selected_frame_ids: Vec::new(),
            gap_ids: Vec::new(),
            algorithm_versions: Vec::new(),
            cache: None,
        })
    }

    fn add_reference(&mut self, reference: &EvidenceReference) -> crate::Result<()> {
        self.add_trace(EvidenceTraceRecord {
            id: reference.id.clone(),
            kind: reference.kind,
            availability: reference.availability,
            output_sha256: reference.sha256.clone(),
            manifest_sha256: None,
            source_frame_ids: Vec::new(),
            selected_frame_ids: Vec::new(),
            gap_ids: Vec::new(),
            algorithm_versions: Vec::new(),
            cache: None,
        })
    }

    fn add_trace(&mut self, incoming: EvidenceTraceRecord) -> crate::Result<()> {
        if let Some(existing) = self.traces.iter_mut().find(|trace| trace.id == incoming.id) {
            if existing.kind != incoming.kind {
                return Err(ContractError::new(
                    "result evidence id is ambiguous across evidence kinds",
                ));
            }
            if existing.output_sha256.is_some()
                && incoming.output_sha256.is_some()
                && existing.output_sha256 != incoming.output_sha256
            {
                return Err(ContractError::new(
                    "result evidence id has contradictory output hashes",
                ));
            }
            if existing.output_sha256.is_none() {
                existing.output_sha256 = incoming.output_sha256;
            }
            if existing.availability != incoming.availability {
                return Err(ContractError::new(
                    "result evidence id has contradictory availability",
                ));
            }
            return Ok(());
        }
        incoming.validate(&self.source_frame_ids, &self.gap_ids)?;
        self.traces.push(incoming);
        Ok(())
    }

    fn finish(self) -> crate::Result<Vec<EvidenceTraceRecord>> {
        for trace in &self.traces {
            trace.validate(&self.source_frame_ids, &self.gap_ids)?;
        }
        Ok(self.traces)
    }
}

fn require_package_order(packages: &[ConditionPackage]) -> crate::Result<()> {
    let mut previous = None;
    let mut seen = BTreeSet::new();
    for package in packages {
        package.validate()?;
        if !seen.insert(package.condition_id) {
            return Err(ContractError::new(
                "result packages must contain one package per condition",
            ));
        }
        if previous.is_some_and(|rank| rank >= package.condition_id.rank()) {
            return Err(ContractError::new(
                "result packages must use canonical A-E order",
            ));
        }
        previous = Some(package.condition_id.rank());
    }
    Ok(())
}

fn require_aggregate_order(aggregates: &[ConditionAggregate]) -> crate::Result<()> {
    let mut previous = None;
    let mut seen = BTreeSet::new();
    for aggregate in aggregates {
        if !seen.insert(aggregate.condition_id) {
            return Err(ContractError::new(
                "result condition aggregates must be unique",
            ));
        }
        if previous.is_some_and(|rank| rank >= aggregate.condition_id.rank()) {
            return Err(ContractError::new(
                "result condition aggregates must use canonical A-E order",
            ));
        }
        previous = Some(aggregate.condition_id.rank());
    }
    Ok(())
}

fn require_retained_trace(result: &TrialResultRecord, evidence_id: &str) -> crate::Result<()> {
    let trace = result
        .evidence
        .iter()
        .find(|trace| trace.id == evidence_id)
        .ok_or_else(|| ContractError::new("result claim references unknown evidence"))?;
    if trace.availability != EvidenceAvailability::Retained {
        return Err(ContractError::new(
            "result claim references evidence that is not retained",
        ));
    }
    Ok(())
}

fn expected_retention(availability: &[SourceFrameAvailability]) -> RetentionState {
    let retained = availability
        .iter()
        .filter(|record| record.availability == EvidenceAvailability::Retained)
        .count();
    if retained == availability.len() {
        RetentionState::Retained
    } else if retained == 0
        && availability
            .iter()
            .all(|record| record.availability == EvidenceAvailability::Evicted)
    {
        RetentionState::Evicted
    } else if retained > 0 {
        RetentionState::PartiallyRetained
    } else {
        RetentionState::Unavailable
    }
}

fn validate_source_frame_availability(
    source_frame_ids: &[String],
    availability: &[SourceFrameAvailability],
    label: &str,
) -> crate::Result<()> {
    if availability.len() != source_frame_ids.len() {
        return Err(ContractError::new(format!(
            "{label} must contain one record for every source frame"
        )));
    }
    let ids = availability
        .iter()
        .map(|record| {
            privacy::validate_opaque_id(&record.id, &format!("{label} id"))?;
            Ok(record.id.clone())
        })
        .collect::<crate::Result<Vec<_>>>()?;
    if ids != source_frame_ids {
        return Err(ContractError::new(format!(
            "{label} must preserve source-frame order"
        )));
    }
    Ok(())
}

fn validate_unique_ids(values: &[String], label: &str) -> crate::Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        privacy::validate_opaque_id(value, label)?;
        if !seen.insert(value) {
            return Err(ContractError::new(format!("{label} must be unique")));
        }
    }
    Ok(())
}

fn validate_unique_hashes(values: &[String], label: &str) -> crate::Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        privacy::validate_sha256(value, label)?;
        if !seen.insert(value) {
            return Err(ContractError::new(format!("{label} must be unique")));
        }
    }
    Ok(())
}

fn validate_named_version(value: &NamedVersion, label: &str) -> crate::Result<()> {
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

fn validate_status_failure(
    status: EvaluationStatus,
    failure: Option<&FailureRecord>,
) -> crate::Result<()> {
    if status == EvaluationStatus::Pass && failure.is_some() {
        return Err(ContractError::new("passing result cannot carry a failure"));
    }
    if status != EvaluationStatus::Pass && failure.is_none() {
        return Err(ContractError::new("non-passing result requires a failure"));
    }
    if let Some(failure) = failure {
        privacy::validate_safe_text(
            &failure.phase,
            "result failure phase",
            privacy::MAX_SHORT_TEXT,
        )?;
        privacy::validate_safe_text(
            &failure.reason,
            "result failure reason",
            privacy::MAX_LONG_TEXT,
        )?;
        privacy::validate_safe_text(
            &failure.recovery,
            "result failure recovery",
            privacy::MAX_LONG_TEXT,
        )?;
    }
    Ok(())
}

fn validate_check(check: &ThresholdCheck, expected_delta: u16) -> crate::Result<()> {
    check
        .observed_rate
        .map(|rate| rate.validate())
        .transpose()?;
    check
        .reference_rate
        .map(|rate| rate.validate())
        .transpose()?;
    check
        .tile_observed_rate
        .map(|rate| rate.validate())
        .transpose()?;
    check
        .tile_reference_rate
        .map(|rate| rate.validate())
        .transpose()?;
    privacy::validate_safe_text(
        &check.rationale_code,
        "threshold check rationale",
        privacy::MAX_SHORT_TEXT,
    )?;
    if ((check.status == EvaluationStatus::Pass && check.threshold_delta_pp != expected_delta)
        || (check.status != EvaluationStatus::Pass
            && check.threshold_delta_pp != 0
            && check.threshold_delta_pp != expected_delta))
        || check.passed != (check.status == EvaluationStatus::Pass)
    {
        return Err(ContractError::new(
            "threshold check contradicts its status, threshold, or rationale",
        ));
    }
    if check.tile_passed.is_some()
        && (check.tile_observed_rate.is_none() || check.tile_reference_rate.is_none())
    {
        return Err(ContractError::new(
            "tile threshold check must preserve both tile rates",
        ));
    }
    validate_status_failure(check.status, check.failure.as_ref())
}

fn validate_family_check(check: &FamilyThresholdCheck, expected_delta: u16) -> crate::Result<()> {
    check
        .observed_rate
        .map(|rate| rate.validate())
        .transpose()?;
    check
        .reference_rate
        .map(|rate| rate.validate())
        .transpose()?;
    privacy::validate_safe_text(
        &check.rationale_code,
        "family threshold check rationale",
        privacy::MAX_SHORT_TEXT,
    )?;
    if ((check.status == EvaluationStatus::Pass && check.threshold_delta_pp != expected_delta)
        || (check.status != EvaluationStatus::Pass
            && check.threshold_delta_pp != 0
            && check.threshold_delta_pp != expected_delta))
        || check.passed != (check.status == EvaluationStatus::Pass)
    {
        return Err(ContractError::new(
            "family threshold check contradicts its status or threshold",
        ));
    }
    validate_status_failure(check.status, check.failure.as_ref())
}

fn validate_threshold_assessment(
    assessment: &ThresholdAssessment,
    profile: &ThresholdProfile,
) -> crate::Result<()> {
    validate_check(
        &assessment.final_vs_bundle,
        profile.improvement_over_final_screenshot_pp,
    )?;
    if assessment.required_family_improvements.len() != profile.required_families.len()
        || assessment
            .required_family_improvements
            .iter()
            .map(|check| check.family)
            .collect::<Vec<_>>()
            != profile.required_families
    {
        return Err(ContractError::new(
            "family threshold checks must use the canonical required-family order",
        ));
    }
    for check in &assessment.required_family_improvements {
        validate_family_check(check, 1)?;
    }
    validate_check(
        &assessment.bundle_vs_uniform,
        profile.bundle_vs_uniform_minimum_delta_pp,
    )?;
    validate_check(
        &assessment.stable_false_positive_delta,
        profile.stable_false_positive_delta_max_pp,
    )?;
    validate_check(&assessment.progressive_report, 0)?;
    validate_status_failure(assessment.status, assessment.failure.as_ref())
}

fn threshold_statuses(
    assessment: &ThresholdAssessment,
    aggregates: &[ConditionAggregate],
) -> Vec<EvaluationStatus> {
    let mut statuses = aggregates
        .iter()
        .map(|aggregate| aggregate.status)
        .collect::<Vec<_>>();
    statuses.push(assessment.final_vs_bundle.status);
    statuses.extend(
        assessment
            .required_family_improvements
            .iter()
            .map(|check| check.status),
    );
    statuses.push(assessment.bundle_vs_uniform.status);
    statuses.push(assessment.stable_false_positive_delta.status);
    statuses
}

fn status_precedence(statuses: &[EvaluationStatus]) -> EvaluationStatus {
    if statuses.contains(&EvaluationStatus::Blocked) {
        EvaluationStatus::Blocked
    } else if statuses.contains(&EvaluationStatus::Skipped) {
        EvaluationStatus::Skipped
    } else if statuses.contains(&EvaluationStatus::Inconclusive) {
        EvaluationStatus::Inconclusive
    } else if statuses.contains(&EvaluationStatus::Fail) {
        EvaluationStatus::Fail
    } else {
        EvaluationStatus::Pass
    }
}

/// Builds a small deterministic contract-only result for schema and documentation generation.
pub fn sample_evaluation_result() -> crate::Result<EvaluationResultRecord> {
    let interval = SourceInterval::new(
        "result-sample-interval",
        crate::ScopeIdentity::new("result-sample-session", "result-sample-target")?,
        crate::TimeRangeNs::new(0, 7_000)?,
        crate::TimeRangeNs::new(0, 7_000)?,
        3_000,
        (0..8)
            .map(|index| crate::SourceFrameEvidence {
                id: format!("result-frame-{index}"),
                capture_ordinal: index + 1,
                source_time_ns: Some(index * 1_000),
                observed_time_ns: index * 1_000 + 10_000,
                session_time_ns: index * 1_000,
                encoded_sha256: format!("sha256:{:0>64}", index + 1),
                availability: EvidenceAvailability::Retained,
            })
            .collect(),
        Vec::new(),
        RetentionState::Retained,
    )?;
    let package = crate::ConditionPackager::final_screenshot(
        &interval,
        "result-frame-7",
        EvidenceReference::new(
            "result-observation",
            EvidenceReferenceKind::CurrentObservation,
            Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()),
            EvidenceAvailability::Retained,
        )?,
    )?;
    let trial = TrialIdentity {
        trial_id: "result-sample-trial".into(),
        case_id: "movement-reversal/basic".into(),
        family: CaseFamily::MovementReversal,
        duration_ms: 100,
        repetition: 0,
        condition_id: ConditionId::AFinalScreenshot,
    };
    let answer = serde_json::to_vec(&crate::InterpretationAnswer {
        temporary_state: crate::AnswerTruth::Yes,
        state_order: vec![
            crate::StateLabel::Baseline,
            crate::StateLabel::Changed,
            crate::StateLabel::Final,
        ],
        affected_region: crate::AnswerRegion::Rect {
            x: 49,
            y: 73,
            width: 480,
            height: 120,
        },
        motion_behavior: crate::MotionBehavior::Reversal,
        judgment: crate::Judgment::Defective,
        uncertainty_reasons: Vec::new(),
        evidence_refs: vec!["result-frame-7".into()],
    })?;
    let score = crate::score_interpretation(crate::ScoreInput {
        trial: &trial,
        package: &package,
        truth: &crate::BenchmarkDefinition::canonical()
            .case("movement-reversal/basic")
            .expect("sample case")
            .ground_truth,
        raw_answer: &answer,
        raw_answer_ref: "result-sample-sidecar",
    })?;
    let aggregate = aggregate_condition(
        ConditionId::AFinalScreenshot,
        std::slice::from_ref(&score),
        &ThresholdProfile::canonical(),
    )?;
    let thresholds = assess_thresholds(
        std::slice::from_ref(&aggregate),
        std::slice::from_ref(&package),
        &ThresholdProfile::canonical(),
    )?;
    EvaluationResultRecord::from_scores(
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
        EvidenceLayer::DeterministicCi,
        std::slice::from_ref(&package),
        std::slice::from_ref(&score),
        vec![aggregate],
        thresholds,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_sample_is_byte_stable_and_round_trips() {
        let result = sample_evaluation_result().unwrap();
        let first = result.canonical_bytes().unwrap();
        let second = result.canonical_bytes().unwrap();
        assert_eq!(first, second);
        assert_eq!(result.digest().unwrap(), sha256_prefixed(&first));
        assert_eq!(
            EvaluationResultRecord::from_canonical_json(&first).unwrap(),
            result
        );
    }

    #[test]
    fn result_rejects_unknown_unsafe_duplicate_unsorted_and_contradictory_values() {
        let result = sample_evaluation_result().unwrap();
        let mut unknown = serde_json::to_value(&result).unwrap();
        unknown["unknown"] = serde_json::json!(true);
        assert!(serde_json::from_value::<EvaluationResultRecord>(unknown).is_err());

        let mut unsafe_ref = result.clone();
        unsafe_ref.trials[0].score.raw_answer_ref = "/tmp/raw-answer".into();
        assert!(unsafe_ref.validate().is_err());

        let mut duplicate = result.clone();
        duplicate.trials.push(duplicate.trials[0].clone());
        assert!(duplicate.validate().is_err());

        let mut unsorted = result.clone();
        unsorted.trials[0].evidence_ids.reverse();
        assert!(unsorted.validate().is_err());

        let mut contradictory = result.clone();
        contradictory.status = EvaluationStatus::Pass;
        assert!(contradictory.validate().is_err());
    }
}
