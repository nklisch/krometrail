use std::num::NonZeroU64;

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    BinaryMask, ErrorCode, Frame, FrameRegion, PixelDimensions, PixelFormat, Result, Timestamp,
    VisionError,
};

/// Inclusive range in a caller-declared sequence clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TimeRange {
    start: Timestamp,
    end: Timestamp,
}

impl TimeRange {
    pub fn new(start: Timestamp, end: Timestamp) -> Result<Self> {
        if start > end {
            return Err(VisionError::new(
                ErrorCode::OutOfOrder,
                "time range start must not follow its end",
            ));
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> Timestamp {
        self.start
    }

    pub const fn end(self) -> Timestamp {
        self.end
    }

    pub const fn contains(self, timestamp: Timestamp) -> bool {
        self.start.as_nanos() <= timestamp.as_nanos() && timestamp.as_nanos() <= self.end.as_nanos()
    }
}

impl<'de> Deserialize<'de> for TimeRange {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            start: Timestamp,
            end: Timestamp,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.start, wire.end).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct NonEmptyText(Box<str>);

impl NonEmptyText {
    pub(crate) fn new(
        value: impl Into<String>,
        code: ErrorCode,
        message: &'static str,
    ) -> Result<Self> {
        let value = value.into();
        if value.is_empty() {
            return Err(VisionError::new(code, message));
        }
        Ok(Self(value.into_boxed_str()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

/// A caller-defined annotation on the sequence timeline.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Marker<Id> {
    id: Id,
    timestamp: Timestamp,
    kind: NonEmptyText,
    label: NonEmptyText,
}

impl<Id> Marker<Id> {
    pub fn new(
        id: Id,
        timestamp: Timestamp,
        kind: impl Into<String>,
        label: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            id,
            timestamp,
            kind: NonEmptyText::new(
                kind,
                ErrorCode::InvalidParameter,
                "marker kind must not be empty",
            )?,
            label: NonEmptyText::new(
                label,
                ErrorCode::InvalidParameter,
                "marker label must not be empty",
            )?,
        })
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    pub fn kind(&self) -> &str {
        self.kind.as_str()
    }

    pub fn label(&self) -> &str {
        self.label.as_str()
    }
}

impl<'de, Id: Deserialize<'de>> Deserialize<'de> for Marker<Id> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire<Id> {
            id: Id,
            timestamp: Timestamp,
            kind: String,
            label: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.id, wire.timestamp, wire.kind, wire.label).map_err(serde::de::Error::custom)
    }
}

/// A caller-declared interval for which visual continuity is unknown.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DeclaredGap<Id> {
    id: Id,
    range: TimeRange,
    reason: NonEmptyText,
    estimated_missing_frames: Option<NonZeroU64>,
}

impl<Id> DeclaredGap<Id> {
    pub fn new(
        id: Id,
        range: TimeRange,
        reason: impl Into<String>,
        estimated_missing_frames: Option<NonZeroU64>,
    ) -> Result<Self> {
        Ok(Self {
            id,
            range,
            reason: NonEmptyText::new(
                reason,
                ErrorCode::InvalidParameter,
                "gap reason must not be empty",
            )?,
            estimated_missing_frames,
        })
    }

    pub fn id(&self) -> &Id {
        &self.id
    }

    pub const fn range(&self) -> TimeRange {
        self.range
    }

    pub fn reason(&self) -> &str {
        self.reason.as_str()
    }

    pub const fn estimated_missing_frames(&self) -> Option<NonZeroU64> {
        self.estimated_missing_frames
    }
}

impl<'de, Id: Deserialize<'de>> Deserialize<'de> for DeclaredGap<Id> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire<Id> {
            id: Id,
            range: TimeRange,
            reason: String,
            estimated_missing_frames: Option<NonZeroU64>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(
            wire.id,
            wire.range,
            wire.reason,
            wire.estimated_missing_frames,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// An immutable, validated sequence in one coordinate space and clock.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FrameSequence<FrameId, MarkerId, GapId, Pixels> {
    frames: Box<[Frame<FrameId, Pixels>]>,
    markers: Box<[Marker<MarkerId>]>,
    gaps: Box<[DeclaredGap<GapId>]>,
    region: Option<FrameRegion>,
    mask: Option<BinaryMask>,
}

pub type OwnedFrameSequence<F, M, G> = FrameSequence<F, M, G, Box<[u8]>>;
pub type BorrowedFrameSequence<'a, F, M, G> = FrameSequence<F, M, G, &'a [u8]>;

impl<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>> FrameSequence<F, M, G, P> {
    pub fn new(
        frames: Vec<Frame<F, P>>,
        markers: Vec<Marker<M>>,
        gaps: Vec<DeclaredGap<G>>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
    ) -> Result<Self> {
        let Some(first) = frames.first() else {
            return Err(VisionError::new(
                ErrorCode::EmptySequence,
                "frame sequence must not be empty",
            ));
        };
        let dimensions = first.dimensions();
        let pixel_format = first.pixel_format();
        for (index, frame) in frames.iter().enumerate() {
            if frames[..index].iter().any(|prior| prior.id() == frame.id()) {
                return Err(VisionError::at(
                    ErrorCode::DuplicateIdentifier,
                    "frame identifiers must be unique",
                    index,
                ));
            }
            if index > 0 && frames[index - 1].timestamp() > frame.timestamp() {
                return Err(VisionError::at(
                    ErrorCode::OutOfOrder,
                    "frame timestamps must be nondecreasing",
                    index,
                ));
            }
            if frame.dimensions() != dimensions || frame.pixel_format() != pixel_format {
                return Err(VisionError::at(
                    ErrorCode::IncompatibleFrame,
                    "all frames must use common dimensions and pixel format",
                    index,
                ));
            }
        }

        let range = TimeRange::new(first.timestamp(), frames.last().unwrap().timestamp())?;
        validate_markers(&markers, range)?;
        validate_gaps(&gaps, range)?;

        if region.is_some_and(|value| !value.rect().fits_within(dimensions)) {
            return Err(VisionError::new(
                ErrorCode::InvalidRegion,
                "sequence region does not fit its frame dimensions",
            ));
        }
        if mask
            .as_ref()
            .is_some_and(|value| value.dimensions() != dimensions)
        {
            return Err(VisionError::new(
                ErrorCode::InvalidMask,
                "sequence mask dimensions do not match its frames",
            ));
        }

        Ok(Self {
            frames: frames.into_boxed_slice(),
            markers: markers.into_boxed_slice(),
            gaps: gaps.into_boxed_slice(),
            region,
            mask,
        })
    }

    pub fn frames(&self) -> &[Frame<F, P>] {
        &self.frames
    }

    pub fn markers(&self) -> &[Marker<M>] {
        &self.markers
    }

    pub fn gaps(&self) -> &[DeclaredGap<G>] {
        &self.gaps
    }

    pub const fn region(&self) -> Option<FrameRegion> {
        self.region
    }

    pub fn mask(&self) -> Option<&BinaryMask> {
        self.mask.as_ref()
    }

    pub fn range(&self) -> TimeRange {
        TimeRange {
            start: self.frames[0].timestamp(),
            end: self.frames[self.frames.len() - 1].timestamp(),
        }
    }

    pub fn dimensions(&self) -> PixelDimensions {
        self.frames[0].dimensions()
    }

    pub fn pixel_format(&self) -> PixelFormat {
        self.frames[0].pixel_format()
    }

    pub fn frame_by_id(&self, id: &F) -> Option<&Frame<F, P>> {
        self.frames.iter().find(|frame| frame.id() == id)
    }
}

impl<F: Clone + Eq, M: Clone + Eq, G: Clone + Eq, P: AsRef<[u8]>> FrameSequence<F, M, G, P> {
    pub fn to_owned(&self) -> OwnedFrameSequence<F, M, G> {
        FrameSequence {
            frames: self
                .frames
                .iter()
                .map(Frame::to_owned)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            markers: self.markers.clone(),
            gaps: self.gaps.clone(),
            region: self.region,
            mask: self.mask.clone(),
        }
    }
}

impl<'de, F, M, G, P> Deserialize<'de> for FrameSequence<F, M, G, P>
where
    F: Deserialize<'de> + Eq,
    M: Deserialize<'de> + Eq,
    G: Deserialize<'de> + Eq,
    P: Deserialize<'de> + AsRef<[u8]>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(bound(
            deserialize = "F: Deserialize<'de>, M: Deserialize<'de>, G: Deserialize<'de>, P: Deserialize<'de> + AsRef<[u8]>"
        ))]
        struct Wire<F, M, G, P> {
            frames: Vec<Frame<F, P>>,
            markers: Vec<Marker<M>>,
            gaps: Vec<DeclaredGap<G>>,
            region: Option<FrameRegion>,
            mask: Option<BinaryMask>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.frames, wire.markers, wire.gaps, wire.region, wire.mask)
            .map_err(serde::de::Error::custom)
    }
}

fn validate_markers<M: Eq>(markers: &[Marker<M>], range: TimeRange) -> Result<()> {
    for (index, marker) in markers.iter().enumerate() {
        if markers[..index]
            .iter()
            .any(|prior| prior.id() == marker.id())
        {
            return Err(VisionError::at(
                ErrorCode::DuplicateIdentifier,
                "marker identifiers must be unique",
                index,
            ));
        }
        if index > 0 && markers[index - 1].timestamp() > marker.timestamp() {
            return Err(VisionError::at(
                ErrorCode::OutOfOrder,
                "marker timestamps must be nondecreasing",
                index,
            ));
        }
        if !range.contains(marker.timestamp()) {
            return Err(VisionError::at(
                ErrorCode::AnnotationOutOfRange,
                "marker timestamp lies outside the frame range",
                index,
            ));
        }
    }
    Ok(())
}

fn validate_gaps<G: Eq>(gaps: &[DeclaredGap<G>], range: TimeRange) -> Result<()> {
    for (index, gap) in gaps.iter().enumerate() {
        if gaps[..index].iter().any(|prior| prior.id() == gap.id()) {
            return Err(VisionError::at(
                ErrorCode::DuplicateIdentifier,
                "gap identifiers must be unique",
                index,
            ));
        }
        if index > 0 {
            let prior = gaps[index - 1].range();
            if prior.start() > gap.range().start() {
                return Err(VisionError::at(
                    ErrorCode::OutOfOrder,
                    "gap ranges must be ordered by start time",
                    index,
                ));
            }
            if prior.end() > gap.range().start() {
                return Err(VisionError::at(
                    ErrorCode::OutOfOrder,
                    "gap ranges must not overlap",
                    index,
                ));
            }
        }
        if !range.contains(gap.range().start()) || !range.contains(gap.range().end()) {
            return Err(VisionError::at(
                ErrorCode::AnnotationOutOfRange,
                "gap range lies outside the frame range",
                index,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{PixelFormat, PixelRect};

    fn frame(id: u8, timestamp: u64) -> Frame<u8, Box<[u8]>> {
        Frame::new(
            id,
            Timestamp::from_nanos(timestamp),
            PixelDimensions::new(1, 1).unwrap(),
            PixelFormat::Rgba8SrgbStraight,
            vec![0, 0, 0, 255].into_boxed_slice(),
        )
        .unwrap()
    }

    #[test]
    fn preserves_ties_and_rejects_ambiguous_sequence_order() {
        let tied = FrameSequence::new(
            vec![frame(2, 1), frame(1, 1)],
            vec![
                Marker::new(2, Timestamp::from_nanos(1), "a", "first").unwrap(),
                Marker::new(1, Timestamp::from_nanos(1), "b", "second").unwrap(),
            ],
            Vec::<DeclaredGap<u8>>::new(),
            None,
            None,
        )
        .unwrap();
        assert_eq!(tied.frames()[0].id(), &2);
        assert_eq!(tied.markers()[0].id(), &2);
        assert_eq!(
            FrameSequence::new(
                vec![frame(1, 2), frame(2, 1)],
                Vec::<Marker<u8>>::new(),
                Vec::<DeclaredGap<u8>>::new(),
                None,
                None,
            )
            .unwrap_err()
            .code,
            ErrorCode::OutOfOrder
        );
        assert_eq!(
            FrameSequence::new(
                vec![frame(1, 1), frame(1, 2)],
                Vec::<Marker<u8>>::new(),
                Vec::<DeclaredGap<u8>>::new(),
                None,
                None,
            )
            .unwrap_err()
            .code,
            ErrorCode::DuplicateIdentifier
        );
    }

    #[test]
    fn validates_annotations_gaps_and_sequence_geometry() {
        let frames = vec![frame(1, 1), frame(2, 3)];
        let gaps = vec![
            DeclaredGap::new(
                1,
                TimeRange::new(Timestamp::from_nanos(1), Timestamp::from_nanos(2)).unwrap(),
                "loss",
                None,
            )
            .unwrap(),
            DeclaredGap::new(
                2,
                TimeRange::new(Timestamp::from_nanos(2), Timestamp::from_nanos(3)).unwrap(),
                "loss",
                None,
            )
            .unwrap(),
        ];
        assert!(FrameSequence::new(frames, Vec::<Marker<u8>>::new(), gaps, None, None).is_ok());

        let wrong_region = FrameRegion::new(
            PixelRect::new(0, 0, 2, 1).unwrap(),
            PixelDimensions::new(2, 1).unwrap(),
        )
        .unwrap();
        assert_eq!(
            FrameSequence::new(
                vec![frame(1, 1)],
                Vec::<Marker<u8>>::new(),
                Vec::<DeclaredGap<u8>>::new(),
                Some(wrong_region),
                None,
            )
            .unwrap_err()
            .code,
            ErrorCode::InvalidRegion
        );
    }
}
