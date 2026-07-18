use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Architecture, ContractError, Platform, Result, privacy};

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub enum VideoConditionId {
    #[serde(rename = "F-real-time-video")]
    FRealTimeVideo,
    #[serde(rename = "G-model-optimized-video")]
    GModelOptimizedVideo,
}

impl VideoConditionId {
    pub const ALL: [Self; 2] = [Self::FRealTimeVideo, Self::GModelOptimizedVideo];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoPresentationPolicy {
    RealTime,
    ModelOptimized,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionalVideoCondition {
    pub condition_id: VideoConditionId,
    pub required: bool,
    pub presentation_policy: VideoPresentationPolicy,
}

pub fn optional_video_conditions() -> [OptionalVideoCondition; 2] {
    [
        OptionalVideoCondition {
            condition_id: VideoConditionId::FRealTimeVideo,
            required: false,
            presentation_policy: VideoPresentationPolicy::RealTime,
        },
        OptionalVideoCondition {
            condition_id: VideoConditionId::GModelOptimizedVideo,
            required: false,
            presentation_policy: VideoPresentationPolicy::ModelOptimized,
        },
    ]
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoHostModelIdentity {
    pub host: String,
    pub platform: Platform,
    pub architecture: Architecture,
    pub provider: String,
    pub model_id: String,
    pub model_version_or_dated_alias: String,
    pub video_input_declared: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoEncoderEvidence {
    pub implementation_version: String,
    pub build_sha256: String,
    pub encoder_name: String,
    pub adapter_version: String,
    pub argument_policy_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoResourceEvidence {
    pub source_interval_sha256: String,
    pub gap_ids: Vec<String>,
    pub artifact_id: String,
    pub output_sha256: String,
    pub video_uri: String,
    pub manifest_uri: String,
    pub manifest_sha256: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VideoConditionEvidence {
    pub condition_id: VideoConditionId,
    pub required: bool,
    pub host_model: VideoHostModelIdentity,
    pub encoder: VideoEncoderEvidence,
    pub presentation_policy: VideoPresentationPolicy,
    pub resource: VideoResourceEvidence,
}

impl VideoConditionEvidence {
    pub fn validate(&self) -> Result<()> {
        let expected = optional_video_conditions()
            .into_iter()
            .find(|condition| condition.condition_id == self.condition_id)
            .ok_or_else(|| ContractError::new("unknown optional video condition"))?;
        if self.required
            || self.required != expected.required
            || self.presentation_policy != expected.presentation_policy
        {
            return Err(ContractError::new(
                "optional video condition policy or requiredness does not match the registry",
            ));
        }
        if !self.host_model.video_input_declared {
            return Err(ContractError::new(
                "optional video evidence requires declared host/model video input support",
            ));
        }
        for (value, label) in [
            (&self.host_model.host, "video host"),
            (&self.host_model.provider, "video provider"),
            (&self.host_model.model_id, "video model id"),
            (
                &self.host_model.model_version_or_dated_alias,
                "video model version",
            ),
            (
                &self.encoder.implementation_version,
                "video encoder implementation",
            ),
            (&self.encoder.encoder_name, "video encoder name"),
            (&self.encoder.adapter_version, "video adapter version"),
            (
                &self.encoder.argument_policy_version,
                "video argument policy version",
            ),
        ] {
            privacy::validate_safe_text(value, label, privacy::MAX_SHORT_TEXT)?;
        }
        privacy::validate_sha256(&self.encoder.build_sha256, "video encoder build")?;
        privacy::validate_sha256(
            &self.resource.source_interval_sha256,
            "video source interval",
        )?;
        privacy::validate_sha256(&self.resource.output_sha256, "video output")?;
        privacy::validate_sha256(&self.resource.manifest_sha256, "video manifest")?;
        privacy::validate_opaque_id(&self.resource.artifact_id, "video artifact id")?;
        for gap_id in &self.resource.gap_ids {
            privacy::validate_opaque_id(gap_id, "video gap id")?;
        }
        validate_resource_pair(&self.resource)?;
        privacy::sanitize_serialized(self)
    }
}

fn validate_resource_pair(resource: &VideoResourceEvidence) -> Result<()> {
    let prefix = "krometrail://evidence/";
    let video = resource
        .video_uri
        .strip_prefix(prefix)
        .ok_or_else(|| ContractError::new("video URI must be a local Krometrail evidence URI"))?;
    let manifest = resource.manifest_uri.strip_prefix(prefix).ok_or_else(|| {
        ContractError::new("manifest URI must be a local Krometrail evidence URI")
    })?;
    let video = video.split('/').collect::<Vec<_>>();
    let manifest = manifest.split('/').collect::<Vec<_>>();
    if video.len() != 4
        || manifest.len() != 4
        || video[0..2] != manifest[0..2]
        || video[2] != "videos"
        || manifest[2] != "video-manifests"
        || video[3] != resource.artifact_id
        || manifest[3] != resource.artifact_id
        || video.iter().any(|segment| segment.is_empty())
        || manifest.iter().any(|segment| segment.is_empty())
    {
        return Err(ContractError::new(
            "video and manifest URIs must be one canonical scoped artifact pair",
        ));
    }
    Ok(())
}
