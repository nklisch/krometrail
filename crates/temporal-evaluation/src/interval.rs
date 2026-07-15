use std::collections::HashSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    ContractError, EvidenceAvailability, Result, RetentionState, canonical_json, privacy,
    sha256_prefixed,
};

/// One opaque session/target scope. Evaluation never interprets browser identities.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScopeIdentity {
    pub session_id: String,
    pub target_id: String,
}

impl ScopeIdentity {
    pub fn new(session_id: impl Into<String>, target_id: impl Into<String>) -> Result<Self> {
        let value = Self {
            session_id: session_id.into(),
            target_id: target_id.into(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        privacy::validate_opaque_id(&self.session_id, "session scope id")?;
        privacy::validate_opaque_id(&self.target_id, "target scope id")
    }
}

impl<'de> Deserialize<'de> for ScopeIdentity {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            session_id: String,
            target_id: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.session_id, wire.target_id).map_err(serde::de::Error::custom)
    }
}

/// An inclusive normalized session-time range. Source and observed clocks are not converted here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeRangeNs {
    pub start_ns: u64,
    pub end_ns: u64,
}

impl TimeRangeNs {
    pub fn new(start_ns: u64, end_ns: u64) -> Result<Self> {
        if start_ns > end_ns {
            return Err(ContractError::new(
                "normalized time range start must not exceed its end",
            ));
        }
        Ok(Self { start_ns, end_ns })
    }

    pub fn validate(self) -> Result<()> {
        if self.start_ns > self.end_ns {
            return Err(ContractError::new(
                "normalized time range start must not exceed its end",
            ));
        }
        Ok(())
    }

    pub const fn contains(self, value: u64) -> bool {
        value >= self.start_ns && value <= self.end_ns
    }

    pub const fn intersects(self, other: Self) -> bool {
        self.start_ns <= other.end_ns && other.start_ns <= self.end_ns
    }
}

impl<'de> Deserialize<'de> for TimeRangeNs {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            start_ns: u64,
            end_ns: u64,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.start_ns, wire.end_ns).map_err(serde::de::Error::custom)
    }
}

/// Metadata for one source observation. Image payloads never cross this boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceFrameEvidence {
    pub id: String,
    pub capture_ordinal: u64,
    pub source_time_ns: Option<u64>,
    pub observed_time_ns: u64,
    pub session_time_ns: u64,
    pub encoded_sha256: String,
    pub availability: EvidenceAvailability,
}

impl SourceFrameEvidence {
    pub fn new(
        id: impl Into<String>,
        capture_ordinal: u64,
        source_time_ns: Option<u64>,
        observed_time_ns: u64,
        session_time_ns: u64,
        encoded_sha256: impl Into<String>,
        availability: EvidenceAvailability,
    ) -> Result<Self> {
        let value = Self {
            id: id.into(),
            capture_ordinal,
            source_time_ns,
            observed_time_ns,
            session_time_ns,
            encoded_sha256: encoded_sha256.into(),
            availability,
        };
        value.validate(0)?;
        Ok(value)
    }

    pub fn validate(&self, index: usize) -> Result<()> {
        privacy::validate_opaque_id(&self.id, &format!("source frame id at index {index}"))?;
        if self.capture_ordinal == 0 {
            return Err(ContractError::new(format!(
                "source frame at index {index} has a zero capture ordinal"
            )));
        }
        if self.session_time_ns > self.observed_time_ns {
            return Err(ContractError::new(format!(
                "source frame at index {index} has normalized time after observed time"
            )));
        }
        privacy::validate_sha256(
            &self.encoded_sha256,
            &format!("source frame hash at index {index}"),
        )
    }
}

impl<'de> Deserialize<'de> for SourceFrameEvidence {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            id: String,
            capture_ordinal: u64,
            source_time_ns: Option<u64>,
            observed_time_ns: u64,
            session_time_ns: u64,
            encoded_sha256: String,
            availability: EvidenceAvailability,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.capture_ordinal,
            wire.source_time_ns,
            wire.observed_time_ns,
            wire.session_time_ns,
            wire.encoded_sha256,
            wire.availability,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// A declared interval where the recorder did not provide a continuous source sequence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GapEvidence {
    pub id: String,
    pub start_session_time_ns: u64,
    pub end_session_time_ns: u64,
    pub reason: String,
    pub estimated_missing_frames: Option<u64>,
}

impl GapEvidence {
    pub fn new(
        id: impl Into<String>,
        start_session_time_ns: u64,
        end_session_time_ns: u64,
        reason: impl Into<String>,
        estimated_missing_frames: Option<u64>,
    ) -> Result<Self> {
        let value = Self {
            id: id.into(),
            start_session_time_ns,
            end_session_time_ns,
            reason: reason.into(),
            estimated_missing_frames,
        };
        value.validate(0)?;
        Ok(value)
    }

    fn validate(&self, index: usize) -> Result<()> {
        privacy::validate_opaque_id(&self.id, &format!("gap id at index {index}"))?;
        if self.start_session_time_ns > self.end_session_time_ns {
            return Err(ContractError::new(format!(
                "gap at index {index} has an inverted normalized range"
            )));
        }
        privacy::validate_safe_text(
            &self.reason,
            &format!("gap reason at index {index}"),
            privacy::MAX_SHORT_TEXT,
        )
    }

    pub const fn range(&self) -> TimeRangeNs {
        TimeRangeNs {
            start_ns: self.start_session_time_ns,
            end_ns: self.end_session_time_ns,
        }
    }
}

impl<'de> Deserialize<'de> for GapEvidence {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            id: String,
            start_session_time_ns: u64,
            end_session_time_ns: u64,
            reason: String,
            estimated_missing_frames: Option<u64>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.start_session_time_ns,
            wire.end_session_time_ns,
            wire.reason,
            wire.estimated_missing_frames,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// The exact source interval shared by every evidence condition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceInterval {
    pub interval_id: String,
    pub session_scope: ScopeIdentity,
    pub requested_range: TimeRangeNs,
    pub resolved_range: TimeRangeNs,
    pub anchor_session_time_ns: u64,
    pub frames: Vec<SourceFrameEvidence>,
    pub gaps: Vec<GapEvidence>,
    pub retention: RetentionState,
    pub digest: String,
}

impl SourceInterval {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        interval_id: impl Into<String>,
        session_scope: ScopeIdentity,
        requested_range: TimeRangeNs,
        resolved_range: TimeRangeNs,
        anchor_session_time_ns: u64,
        frames: Vec<SourceFrameEvidence>,
        gaps: Vec<GapEvidence>,
        retention: RetentionState,
    ) -> Result<Self> {
        let mut value = Self {
            interval_id: interval_id.into(),
            session_scope,
            requested_range,
            resolved_range,
            anchor_session_time_ns,
            frames,
            gaps,
            retention,
            digest: String::new(),
        };
        value.validate_without_digest()?;
        value.digest = value.computed_digest()?;
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<()> {
        self.validate_without_digest()?;
        privacy::validate_sha256(&self.digest, "source interval digest")?;
        if self.digest != self.computed_digest()? {
            return Err(ContractError::new(
                "source interval digest does not match its exact identities",
            ));
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical_json(self)
    }

    pub fn digest(&self) -> Result<String> {
        self.validate()?;
        Ok(self.digest.clone())
    }

    pub fn frame(&self, id: &str) -> Option<&SourceFrameEvidence> {
        self.frames.iter().find(|frame| frame.id == id)
    }

    pub fn gap(&self, id: &str) -> Option<&GapEvidence> {
        self.gaps.iter().find(|gap| gap.id == id)
    }

    pub fn has_unresolved_gap(&self) -> bool {
        !self.gaps.is_empty()
    }

    pub fn retained_frames(&self) -> impl Iterator<Item = &SourceFrameEvidence> {
        self.frames
            .iter()
            .filter(|frame| frame.availability == EvidenceAvailability::Retained)
    }

    pub fn frame_ids(&self) -> Vec<String> {
        self.frames.iter().map(|frame| frame.id.clone()).collect()
    }

    pub fn gap_ids(&self) -> Vec<String> {
        self.gaps.iter().map(|gap| gap.id.clone()).collect()
    }

    fn validate_without_digest(&self) -> Result<()> {
        privacy::validate_opaque_id(&self.interval_id, "source interval id")?;
        self.session_scope.validate()?;
        self.requested_range.validate()?;
        self.resolved_range.validate()?;
        if self.resolved_range.start_ns < self.requested_range.start_ns
            || self.resolved_range.end_ns > self.requested_range.end_ns
        {
            return Err(ContractError::new(
                "resolved source interval must be contained in its requested range",
            ));
        }
        if !self.resolved_range.contains(self.anchor_session_time_ns) {
            return Err(ContractError::new(
                "source interval anchor must be inside the resolved range",
            ));
        }
        if self.frames.is_empty() {
            return Err(ContractError::new(
                "source interval must contain at least one source frame record",
            ));
        }

        let mut frame_ids = HashSet::with_capacity(self.frames.len());
        for (index, frame) in self.frames.iter().enumerate() {
            frame.validate(index)?;
            if !frame_ids.insert(&frame.id) {
                return Err(ContractError::new(format!(
                    "source interval contains duplicate frame id at index {index}"
                )));
            }
            if !self.resolved_range.contains(frame.session_time_ns) {
                return Err(ContractError::new(format!(
                    "source frame at index {index} is outside the resolved interval"
                )));
            }
            if index > 0 {
                let previous = &self.frames[index - 1];
                if previous.capture_ordinal >= frame.capture_ordinal {
                    return Err(ContractError::new(
                        "source frame capture ordinals must be strictly increasing",
                    ));
                }
                if previous.observed_time_ns > frame.observed_time_ns {
                    return Err(ContractError::new(
                        "source frame observed times must be nondecreasing",
                    ));
                }
                if previous.session_time_ns > frame.session_time_ns {
                    return Err(ContractError::new(
                        "source frame normalized session times must be nondecreasing",
                    ));
                }
            }
        }

        let mut gap_ids = HashSet::with_capacity(self.gaps.len());
        for (index, gap) in self.gaps.iter().enumerate() {
            gap.validate(index)?;
            if !gap_ids.insert(&gap.id) {
                return Err(ContractError::new(format!(
                    "source interval contains duplicate gap id at index {index}"
                )));
            }
            if !self.resolved_range.contains(gap.start_session_time_ns)
                || !self.resolved_range.contains(gap.end_session_time_ns)
            {
                return Err(ContractError::new(format!(
                    "gap at index {index} is outside the resolved interval"
                )));
            }
            if let Some(previous) = self.gaps.get(index.wrapping_sub(1)) {
                if previous.start_session_time_ns > gap.start_session_time_ns {
                    return Err(ContractError::new(
                        "declared gaps must be ordered by normalized start time",
                    ));
                }
                if previous.end_session_time_ns >= gap.start_session_time_ns {
                    return Err(ContractError::new("declared gaps must not overlap"));
                }
            }
        }

        if self
            .frames
            .iter()
            .filter(|frame| frame.availability == EvidenceAvailability::Gap)
            .any(|frame| {
                !self
                    .gaps
                    .iter()
                    .any(|gap| gap.range().contains(frame.session_time_ns))
            })
        {
            return Err(ContractError::new(
                "a gap-unavailable source frame must be covered by a declared gap",
            ));
        }
        let expected_retention = expected_retention(&self.frames);
        if self.retention != expected_retention {
            return Err(ContractError::new(
                "source interval retention contradicts source-frame availability",
            ));
        }
        Ok(())
    }

    fn computed_digest(&self) -> Result<String> {
        let mut value = serde_json::to_value(self)?;
        value
            .as_object_mut()
            .ok_or_else(|| ContractError::new("source interval did not serialize as an object"))?
            .remove("digest");
        Ok(sha256_prefixed(&canonical_json(&value)?))
    }
}

fn expected_retention(frames: &[SourceFrameEvidence]) -> RetentionState {
    let retained = frames
        .iter()
        .filter(|frame| frame.availability == EvidenceAvailability::Retained)
        .count();
    if retained == frames.len() {
        RetentionState::Retained
    } else if retained == 0
        && frames
            .iter()
            .all(|frame| frame.availability == EvidenceAvailability::Evicted)
    {
        RetentionState::Evicted
    } else if retained > 0 {
        RetentionState::PartiallyRetained
    } else {
        RetentionState::Unavailable
    }
}

impl<'de> Deserialize<'de> for SourceInterval {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            interval_id: String,
            session_scope: ScopeIdentity,
            requested_range: TimeRangeNs,
            resolved_range: TimeRangeNs,
            anchor_session_time_ns: u64,
            frames: Vec<SourceFrameEvidence>,
            gaps: Vec<GapEvidence>,
            retention: RetentionState,
            digest: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        let value = Self {
            interval_id: wire.interval_id,
            session_scope: wire.session_scope,
            requested_range: wire.requested_range,
            resolved_range: wire.resolved_range,
            anchor_session_time_ns: wire.anchor_session_time_ns,
            frames: wire.frames,
            gaps: wire.gaps,
            retention: wire.retention,
            digest: wire.digest,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: &str, ordinal: u64, session_time_ns: u64) -> SourceFrameEvidence {
        SourceFrameEvidence {
            id: id.into(),
            capture_ordinal: ordinal,
            source_time_ns: Some(ordinal),
            observed_time_ns: ordinal + 100,
            session_time_ns,
            encoded_sha256: format!("sha256:{:0>64}", ordinal),
            availability: EvidenceAvailability::Retained,
        }
    }

    #[test]
    fn interval_digest_is_identity_only_and_byte_stable() {
        let interval = SourceInterval::new(
            "interval-1",
            ScopeIdentity::new("session-1", "target-1").unwrap(),
            TimeRangeNs::new(0, 20).unwrap(),
            TimeRangeNs::new(2, 18).unwrap(),
            10,
            vec![frame("frame-1", 1, 2), frame("frame-2", 2, 18)],
            Vec::new(),
            RetentionState::Retained,
        )
        .unwrap();
        let decoded: SourceInterval =
            serde_json::from_slice(&interval.canonical_bytes().unwrap()).unwrap();
        assert_eq!(decoded, interval);
        assert_eq!(
            interval.canonical_bytes().unwrap(),
            decoded.canonical_bytes().unwrap()
        );
        assert_eq!(interval.digest().unwrap(), decoded.digest().unwrap());
    }
}
