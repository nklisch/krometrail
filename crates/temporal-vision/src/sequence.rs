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
    /// Complete retained source identity in source order: every retained
    /// frame of the sequence's visual epoch, decoded or not.
    source_frame_ids: Box<[FrameId]>,
    /// Each decoded frame's position within `source_frame_ids`, or `None`
    /// when the decoded frames are the complete retained source.
    source_indices: Option<Box<[usize]>>,
    /// The caller-declared inclusive time range of the retained source;
    /// present exactly when `source_indices` is.
    source_range: Option<TimeRange>,
}

pub type OwnedFrameSequence<F, M, G> = FrameSequence<F, M, G, Box<[u8]>>;
pub type BorrowedFrameSequence<'a, F, M, G> = FrameSequence<F, M, G, &'a [u8]>;

/// Caller-supplied provenance linking decoded frames to their complete
/// retained source, for use with [`FrameSequence::with_provenance`].
///
/// `indices` and `range` are an all-or-nothing pair: both `None` when the
/// decoded frames are the complete retained source, or both `Some` when they
/// declare the decoded frames' positions within the retained source — which
/// may be a subset of it or all of it (explicit complete-source indices are
/// valid and let the caller declare a wider source time range).
#[derive(Clone, Debug)]
pub struct SourceProvenance<F> {
    /// Complete retained source identity in source order: every retained
    /// frame of the sequence's visual epoch, decoded or not. When `indices`
    /// is `None`, these must be exactly the decoded frame identifiers in
    /// order.
    pub frame_ids: Vec<F>,
    /// Each decoded frame's position within `frame_ids`, or `None` when the
    /// decoded frames are the complete retained source.
    pub indices: Option<Vec<usize>>,
    /// The caller-declared inclusive time range of the retained source;
    /// present exactly when `indices` is.
    pub range: Option<TimeRange>,
}

impl<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>> FrameSequence<F, M, G, P> {
    pub fn new(
        frames: Vec<Frame<F, P>>,
        markers: Vec<Marker<M>>,
        gaps: Vec<DeclaredGap<G>>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
    ) -> Result<Self>
    where
        F: Clone,
    {
        let source_frame_ids = frames.iter().map(|frame| frame.id().clone()).collect();
        Self::assemble(
            frames,
            markers,
            gaps,
            region,
            mask,
            SourceProvenance {
                frame_ids: source_frame_ids,
                indices: None,
                range: None,
            },
        )
    }

    /// Construct a sequence from complete inputs: decoded content and source
    /// provenance in one call.
    ///
    /// This is the in-memory counterpart of deserialization: both delegate to
    /// the same validating authority, so a caller can construct — with the
    /// same invariants — sequences the wire can express and `new` cannot,
    /// such as annotations between decoded frames but inside the declared
    /// source range. [`Self::new`] remains the convenience path for the plain
    /// full-source shape, and [`Self::with_source_provenance`] attaches
    /// provenance to an already-built sequence.
    pub fn with_provenance(
        frames: Vec<Frame<F, P>>,
        markers: Vec<Marker<M>>,
        gaps: Vec<DeclaredGap<G>>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
        provenance: SourceProvenance<F>,
    ) -> Result<Self> {
        Self::assemble(frames, markers, gaps, region, mask, provenance)
    }

    /// Single construction authority for decoded content and provenance:
    /// `new` and `Deserialize` construct through it, and
    /// `with_source_provenance` revalidates through the same shared
    /// validators (`validate_source_provenance`, `validate_markers`,
    /// `validate_gaps`) without rebuilding the sequence. Constructing here
    /// keeps the wire unable to express a sequence the constructors cannot,
    /// and no constructor able to skip the shared frame, annotation, or
    /// provenance checks. Annotations are validated against the effective
    /// range: the declared source range when provenance is declared, the
    /// decoded frame range otherwise.
    fn assemble(
        frames: Vec<Frame<F, P>>,
        markers: Vec<Marker<M>>,
        gaps: Vec<DeclaredGap<G>>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
        provenance: SourceProvenance<F>,
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

        let decoded_range = TimeRange::new(first.timestamp(), frames.last().unwrap().timestamp())?;
        validate_source_provenance(
            &frames,
            &provenance.frame_ids,
            provenance.indices.as_deref(),
            provenance.range,
        )?;
        let effective_range = provenance.range.unwrap_or(decoded_range);
        validate_markers(&markers, effective_range)?;
        validate_gaps(&gaps, effective_range)?;

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
            source_frame_ids: provenance.frame_ids.into_boxed_slice(),
            source_indices: provenance.indices.map(Vec::into_boxed_slice),
            source_range: provenance.range,
        })
    }

    /// Attach the full retained source identity to a bounded decoded subset.
    /// Rendering and measurement continue to use the decoded frames, while
    /// manifests retain the complete source-frame and time-range provenance.
    ///
    /// Annotations are revalidated against the newly declared range through
    /// the same shared validators construction uses: reattaching provenance
    /// that strands an annotation outside the effective range is an error,
    /// not a silent rewrite.
    pub fn with_source_provenance(
        mut self,
        source_frame_ids: Vec<F>,
        source_indices: Vec<usize>,
        source_range: TimeRange,
    ) -> Result<Self> {
        validate_source_provenance(
            &self.frames,
            &source_frame_ids,
            Some(&source_indices),
            Some(source_range),
        )?;
        validate_markers(&self.markers, source_range)?;
        validate_gaps(&self.gaps, source_range)?;
        self.source_frame_ids = source_frame_ids.into_boxed_slice();
        self.source_indices = Some(source_indices.into_boxed_slice());
        self.source_range = Some(source_range);
        Ok(self)
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

    pub fn source_frame_ids(&self) -> &[F] {
        &self.source_frame_ids
    }

    pub fn source_indices(&self) -> Option<&[usize]> {
        self.source_indices.as_deref()
    }

    pub fn source_frame_count(&self) -> usize {
        self.source_frame_ids.len()
    }

    pub fn range(&self) -> TimeRange {
        self.source_range.unwrap_or(TimeRange {
            start: self.frames[0].timestamp(),
            end: self.frames[self.frames.len() - 1].timestamp(),
        })
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
            source_frame_ids: self.source_frame_ids.clone(),
            source_indices: self.source_indices.clone(),
            source_range: self.source_range,
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
        // One current representation: `source_frame_ids` is required — the
        // historical provenance-dropping payload stays rejected — while
        // `source_indices` and `source_range` are ordinary optional members
        // where omission equals null. Their all-or-nothing pairing, subset
        // identity and order, and declared-range containment are validated by
        // the same authority as construction.
        #[derive(Deserialize)]
        #[serde(
            bound(
                deserialize = "F: Deserialize<'de>, M: Deserialize<'de>, G: Deserialize<'de>, P: Deserialize<'de> + AsRef<[u8]>"
            ),
            deny_unknown_fields
        )]
        struct Wire<F, M, G, P> {
            frames: Vec<Frame<F, P>>,
            markers: Vec<Marker<M>>,
            gaps: Vec<DeclaredGap<G>>,
            region: Option<FrameRegion>,
            mask: Option<BinaryMask>,
            source_frame_ids: Vec<F>,
            source_indices: Option<Vec<usize>>,
            source_range: Option<TimeRange>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::assemble(
            wire.frames,
            wire.markers,
            wire.gaps,
            wire.region,
            wire.mask,
            SourceProvenance {
                frame_ids: wire.source_frame_ids,
                indices: wire.source_indices,
                range: wire.source_range,
            },
        )
        .map_err(serde::de::Error::custom)
    }
}

/// Validate the provenance triplet linking a bounded decoded sequence to its
/// complete retained source. Exactly one shape is valid: either the decoded
/// frames are the complete retained source (indices and range both `None`,
/// with `source_frame_ids` equal to the decoded identifiers in order), or the
/// decoded frames declare their positions within the retained source (both
/// `Some`) — a subset of it, or all of it with explicit indices. The declared
/// `source_range` must contain the decoded frame range.
///
/// Duplicate source identifiers are rejected at any distance with a quadratic
/// scan: requiring `Hash` or `Ord` on caller identifier types would add a
/// bound the public generic contract deliberately does not make, so identity
/// comparison stays `Eq`-only. The retained source may be far larger than the
/// decoded subset; no cap relates the two populations.
fn validate_source_provenance<F: Eq, P: AsRef<[u8]>>(
    frames: &[Frame<F, P>],
    source_frame_ids: &[F],
    source_indices: Option<&[usize]>,
    source_range: Option<TimeRange>,
) -> Result<()> {
    let (Some(indices), Some(source_range)) = (source_indices, source_range) else {
        if source_indices.is_none() && source_range.is_none() {
            if source_frame_ids.len() != frames.len()
                || source_frame_ids.iter().ne(frames.iter().map(Frame::id))
            {
                return Err(VisionError::new(
                    ErrorCode::InvalidParameter,
                    "a sequence without source indices must retain every decoded frame as its complete source",
                ));
            }
            return Ok(());
        }
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "source indices and source range must be declared together",
        ));
    };
    if indices.len() != frames.len() {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "source indices must identify every decoded frame",
        ));
    }
    // Duplicates anywhere in the retained source make its identity ambiguous,
    // including between frames the decoded subset never maps.
    for (position, id) in source_frame_ids.iter().enumerate() {
        if source_frame_ids[..position].contains(id) {
            return Err(VisionError::at(
                ErrorCode::DuplicateIdentifier,
                "source frame identifiers must be unique",
                position,
            ));
        }
    }
    // Structural order is checked before identity so a reordered subset is
    // reported as an ordering problem even when its identifiers also drift.
    for (position, &index) in indices.iter().enumerate() {
        if position > 0 && indices[position - 1] >= index {
            return Err(VisionError::at(
                ErrorCode::OutOfOrder,
                "source indices must be strictly increasing",
                position,
            ));
        }
    }
    for (position, (&index, frame)) in indices.iter().zip(frames).enumerate() {
        if index >= source_frame_ids.len() {
            return Err(VisionError::at(
                ErrorCode::InvalidParameter,
                "source index lies outside the retained source frames",
                position,
            ));
        }
        if &source_frame_ids[index] != frame.id() {
            return Err(VisionError::at(
                ErrorCode::InvalidParameter,
                "decoded frame does not match its declared source frame",
                position,
            ));
        }
    }
    let contains = |frame: &Frame<F, P>| source_range.contains(frame.timestamp());
    if !frames.first().is_some_and(contains) || !frames.last().is_some_and(contains) {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "the declared source range must contain the decoded frame range",
        ));
    }
    Ok(())
}

pub(crate) fn validate_markers<M: Eq>(markers: &[Marker<M>], range: TimeRange) -> Result<()> {
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

pub(crate) fn validate_gaps<G: Eq>(gaps: &[DeclaredGap<G>], range: TimeRange) -> Result<()> {
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
