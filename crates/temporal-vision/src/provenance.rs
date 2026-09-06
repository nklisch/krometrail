use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    BinaryMask, ComparisonOutcome, DeclaredGap, ErrorCode, FrameRegion, FrameSequence, Marker,
    PixelDimensions, Result, SelectionReason, StoryboardSelection, TimeRange, VisionError,
    sequence::{NonEmptyText, validate_gaps, validate_markers},
};

stable_registry! {
    /// Stable visual artifact family.
    pub enum ArtifactKind {
        BeforeDuringAfter => "before_during_after",
        Storyboard => "storyboard",
        DifferenceMap => "difference_map",
        RegionFilmstrip => "region_filmstrip",
        MotionHistory => "motion_history",
    }
}

/// Authoritative name/version descriptor for one generated artifact family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GeneratorDescriptor {
    pub name: &'static str,
    pub version: &'static str,
}

/// One registry drives both generated manifests and callers' pre-generation cache keys.
pub const fn generator_descriptor(kind: ArtifactKind) -> GeneratorDescriptor {
    match kind {
        ArtifactKind::Storyboard | ArtifactKind::BeforeDuringAfter => GeneratorDescriptor {
            name: "temporal-storyboard",
            version: "1.1.0",
        },
        ArtifactKind::DifferenceMap => GeneratorDescriptor {
            name: "temporal-difference-map",
            version: "v1",
        },
        ArtifactKind::RegionFilmstrip => GeneratorDescriptor {
            name: "region-filmstrip",
            version: "1.0.0",
        },
        ArtifactKind::MotionHistory => GeneratorDescriptor {
            name: "motion-history",
            version: "1.0.0",
        },
    }
}

stable_registry! {
    /// Relationship between an output and source observations.
    pub enum EvidenceClass {
        SourceFrame => "source_frame",
        SourceDerived => "source_derived",
        Inferred => "inferred",
    }
}

stable_registry! {
    /// Stable source-derived normalization operation.
    pub enum NormalizationKind {
        ColorSpaceConversion => "color_space_conversion",
        AlphaCompositing => "alpha_compositing",
        IntegerScaling => "integer_scaling",
        FixedCrop => "fixed_crop",
        Denoising => "denoising",
        Thresholding => "thresholding",
    }
}

/// Named and versioned artifact algorithm.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct AlgorithmDescriptor {
    name: NonEmptyText,
    version: NonEmptyText,
}

impl AlgorithmDescriptor {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Result<Self> {
        Ok(Self {
            name: NonEmptyText::new(
                name,
                ErrorCode::InvalidParameter,
                "algorithm name must not be empty",
            )?,
            version: NonEmptyText::new(
                version,
                ErrorCode::InvalidParameter,
                "algorithm version must not be empty",
            )?,
        })
    }

    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    pub fn version(&self) -> &str {
        self.version.as_str()
    }
}

impl<'de> Deserialize<'de> for AlgorithmDescriptor {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            name: String,
            version: String,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.name, wire.version).map_err(serde::de::Error::custom)
    }
}

/// A finite deterministic floating-point parameter.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(transparent)]
pub struct FiniteNumber(f64);

impl FiniteNumber {
    pub fn new(value: f64) -> Result<Self> {
        if !value.is_finite() {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "numeric parameters must be finite",
            ));
        }
        Ok(Self(if value == 0.0 { 0.0 } else { value }))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl<'de> Deserialize<'de> for FiniteNumber {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(f64::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Recursive, tagged parameter value with deterministic object ordering.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ParameterValue {
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Number(FiniteNumber),
    Text(Box<str>),
    List(Vec<ParameterValue>),
    Object(BTreeMap<Box<str>, ParameterValue>),
}

impl ParameterValue {
    fn validate(&self) -> Result<()> {
        match self {
            Self::List(values) => values.iter().try_for_each(Self::validate),
            Self::Object(values) => {
                for (key, value) in values {
                    if key.is_empty() {
                        return Err(VisionError::new(
                            ErrorCode::InvalidParameter,
                            "parameter object keys must not be empty",
                        ));
                    }
                    value.validate()?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

impl<'de> Deserialize<'de> for ParameterValue {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(tag = "type", content = "value", rename_all = "snake_case")]
        enum Wire {
            Bool(bool),
            Signed(i64),
            Unsigned(u64),
            Number(FiniteNumber),
            Text(Box<str>),
            List(Vec<Wire>),
            Object(BTreeMap<Box<str>, Wire>),
        }

        fn convert(value: Wire) -> ParameterValue {
            match value {
                Wire::Bool(value) => ParameterValue::Bool(value),
                Wire::Signed(value) => ParameterValue::Signed(value),
                Wire::Unsigned(value) => ParameterValue::Unsigned(value),
                Wire::Number(value) => ParameterValue::Number(value),
                Wire::Text(value) => ParameterValue::Text(value),
                Wire::List(values) => {
                    ParameterValue::List(values.into_iter().map(convert).collect())
                }
                Wire::Object(values) => ParameterValue::Object(
                    values
                        .into_iter()
                        .map(|(key, value)| (key, convert(value)))
                        .collect(),
                ),
            }
        }

        let value = convert(Wire::deserialize(deserializer)?);
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}

/// Artifact parameters with deterministic lexicographic object ordering.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Parameters(BTreeMap<Box<str>, ParameterValue>);

impl Parameters {
    pub fn new(values: BTreeMap<Box<str>, ParameterValue>) -> Result<Self> {
        for (key, value) in &values {
            if key.is_empty() {
                return Err(VisionError::new(
                    ErrorCode::InvalidParameter,
                    "parameter names must not be empty",
                ));
            }
            value.validate()?;
        }
        Ok(Self(values))
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&ParameterValue> {
        self.0.get(name)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ParameterValue)> {
        self.0.iter().map(|(name, value)| (name.as_ref(), value))
    }

    pub(crate) fn insert(&mut self, name: &'static str, value: ParameterValue) -> Result<()> {
        if name.is_empty() {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "parameter names must not be empty",
            ));
        }
        value.validate()?;
        self.0.insert(name.into(), value);
        Ok(())
    }
}

/// The only sampling scheme this crate produces. A disclosure naming anything
/// else is not describing an artifact these generators made.
pub(crate) const ANALYSIS_SAMPLING_MODE: &str = "uniform_bounded";
pub(crate) const ANALYSIS_SAMPLING_SPACING: &str = "uniform";
/// The complete field set of a sampling disclosure, so an extra member is
/// rejected rather than silently carried as an unvalidated claim.
const ANALYSIS_SAMPLING_FIELDS: [&str; 5] = [
    "analyzed_frame_count",
    "analyzed_source_indices",
    "mode",
    "source_frame_count",
    "spacing",
];

pub(crate) fn analysis_sampling_parameters<F, M, G, P>(
    source: &FrameSequence<F, M, G, P>,
) -> Result<Option<ParameterValue>>
where
    F: Eq,
    M: Eq,
    G: Eq,
    P: AsRef<[u8]>,
{
    let Some(indices) = source.source_indices() else {
        return Ok(None);
    };
    // Only a genuinely decimated analysis may claim a sampling mode. An exhaustive
    // run carries source provenance too, and describing it as `uniform_bounded`
    // would be a false claim about how the evidence was produced.
    if indices.len() >= source.source_frame_count() {
        return Ok(None);
    }
    let source_frame_count = u64::try_from(source.source_frame_count()).map_err(|_| {
        VisionError::new(
            ErrorCode::InvalidManifest,
            "analysis sampling source frame count exceeds the manifest format",
        )
    })?;
    let analyzed_frame_count = u64::try_from(indices.len()).map_err(|_| {
        VisionError::new(
            ErrorCode::InvalidManifest,
            "analysis sampling frame count exceeds the manifest format",
        )
    })?;
    let analyzed_source_indices = indices
        .iter()
        .map(|index| {
            u64::try_from(*index)
                .map(ParameterValue::Unsigned)
                .map_err(|_| {
                    VisionError::new(
                        ErrorCode::InvalidManifest,
                        "analysis sampling source index exceeds the manifest format",
                    )
                })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(Some(ParameterValue::Object(
        [
            (
                "source_frame_count".into(),
                ParameterValue::Unsigned(source_frame_count),
            ),
            (
                "analyzed_frame_count".into(),
                ParameterValue::Unsigned(analyzed_frame_count),
            ),
            (
                "analyzed_source_indices".into(),
                ParameterValue::List(analyzed_source_indices),
            ),
            (
                "mode".into(),
                ParameterValue::Text(ANALYSIS_SAMPLING_MODE.into()),
            ),
            (
                "spacing".into(),
                ParameterValue::Text(ANALYSIS_SAMPLING_SPACING.into()),
            ),
        ]
        .into_iter()
        .collect(),
    )))
}

impl<'de> Deserialize<'de> for Parameters {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(BTreeMap::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// One ordered normalization operation included in provenance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NormalizationStep {
    kind: NormalizationKind,
    algorithm_version: NonEmptyText,
    parameters: Parameters,
}

impl NormalizationStep {
    pub fn new(
        kind: NormalizationKind,
        algorithm_version: impl Into<String>,
        parameters: Parameters,
    ) -> Result<Self> {
        Ok(Self {
            kind,
            algorithm_version: NonEmptyText::new(
                algorithm_version,
                ErrorCode::InvalidParameter,
                "normalization algorithm version must not be empty",
            )?,
            parameters,
        })
    }

    pub const fn kind(&self) -> NormalizationKind {
        self.kind
    }

    pub fn algorithm_version(&self) -> &str {
        self.algorithm_version.as_str()
    }

    pub const fn parameters(&self) -> &Parameters {
        &self.parameters
    }
}

impl<'de> Deserialize<'de> for NormalizationStep {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            kind: NormalizationKind,
            algorithm_version: String,
            parameters: Parameters,
        }
        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.kind, wire.algorithm_version, wire.parameters)
            .map_err(serde::de::Error::custom)
    }
}

/// SHA-256 of the exact encoded bytes returned for an artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OutputHash([u8; 32]);

impl OutputHash {
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for OutputHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for OutputHash {
    type Err = VisionError;

    fn from_str(value: &str) -> Result<Self> {
        if value.len() != 64
            || value
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && !(b'a'..=b'f').contains(&byte))
        {
            return Err(VisionError::new(
                ErrorCode::InvalidOutputHash,
                "output hash must be exactly 64 lowercase hexadecimal characters",
            ));
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = (hex_nibble(pair[0]) << 4) | hex_nibble(pair[1]);
        }
        Ok(Self(bytes))
    }
}

const fn hex_nibble(value: u8) -> u8 {
    match value {
        b'0'..=b'9' => value - b'0',
        b'a'..=b'f' => value - b'a' + 10,
        _ => 0,
    }
}

impl Serialize for OutputHash {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for OutputHash {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        <&str>::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// How a generator consumes the sequence it was handed.
///
/// This is the one thing a generator must declare that the manifest cannot work
/// out for itself, and it is deliberately a two-value choice rather than a
/// per-generator hook: the declaration says *which shape* the generator has, and
/// the manifest derives every frame population from it. A generator cannot get
/// the counts wrong, only the shape — and the shape is checked, because
/// `SelectedFramesOnly` must name frames the sequence actually contains.
///
/// Getting this wrong in the permissive direction is what makes a filmstrip
/// claim it analyzed five frames when three tiles were rendered and the other
/// two were never looked at.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SequenceConsumption {
    /// Every decoded frame contributed to the result.
    ///
    /// True of difference maps and motion history, which measure across the whole
    /// decoded sequence, and of storyboards, whose selection is derived by reading
    /// every frame.
    EveryDecodedFrame,
    /// Only the frames the output renders or references contributed.
    ///
    /// True of a filmstrip: its tiles are chosen by position in the sequence, and
    /// a frame that backs no tile is decoded but never examined.
    SelectedFramesOnly,
}

/// Reproducible machine-readable provenance for one generated artifact.
///
/// Three frame populations are distinguished, narrowest last:
///
/// - `source_frame_ids` — every retained frame in the artifact's visual epoch.
/// - `analyzed_frame_ids` — the frames that actually contributed to the artifact.
///   Sampling and bounded selection drop source frames here.
/// - `selected_frame_ids` — the analyzed frames rendered or referenced in the
///   output. An analysis artifact examines many frames but references only one.
///
/// `omitted_frame_count` is `source - analyzed`: frames that contributed nothing.
/// Frames that were analyzed but not rendered are `analyzed - selected`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> {
    artifact_id: ArtifactId,
    artifact_kind: ArtifactKind,
    evidence_class: EvidenceClass,
    algorithm: Box<AlgorithmDescriptor>,
    source_frame_ids: Box<[FrameId]>,
    analyzed_frame_ids: Box<[FrameId]>,
    selected_frame_ids: Box<[FrameId]>,
    storyboard_selection: Option<Box<StoryboardSelection<FrameId>>>,
    source_frame_count: u64,
    analyzed_frame_count: u64,
    omitted_frame_count: u64,
    range: TimeRange,
    markers: Box<[Marker<MarkerId>]>,
    gaps: Box<[DeclaredGap<GapId>]>,
    region: Option<FrameRegion>,
    /// Boxed like the other rarely populated members: only region filmstrips carry
    /// a mask, and an inline one widens every manifest-bearing value.
    mask: Option<Box<BinaryMask>>,
    normalization: Box<[NormalizationStep]>,
    parameters: Parameters,
    output_dimensions: PixelDimensions,
    output_hash: OutputHash,
}

impl<A, F: Clone + Eq, M: Clone + Eq, G: Clone + Eq> ArtifactManifest<A, F, M, G> {
    #[allow(clippy::too_many_arguments)]
    pub fn from_sequence<P: AsRef<[u8]>>(
        artifact_id: A,
        artifact_kind: ArtifactKind,
        evidence_class: EvidenceClass,
        algorithm: AlgorithmDescriptor,
        sequence: &FrameSequence<F, M, G, P>,
        selected_frame_ids: Vec<F>,
        normalization: Vec<NormalizationStep>,
        parameters: Parameters,
        output_dimensions: PixelDimensions,
        output_hash: OutputHash,
    ) -> Result<Self> {
        Self::from_sequence_with_trace_and_domain(
            artifact_id,
            artifact_kind,
            evidence_class,
            algorithm,
            sequence,
            sequence.region(),
            sequence.mask().cloned(),
            selected_frame_ids,
            None,
            SequenceConsumption::EveryDecodedFrame,
            normalization,
            parameters,
            output_dimensions,
            output_hash,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_storyboard_sequence<P: AsRef<[u8]>>(
        artifact_id: A,
        artifact_kind: ArtifactKind,
        evidence_class: EvidenceClass,
        algorithm: AlgorithmDescriptor,
        sequence: &FrameSequence<F, M, G, P>,
        selected_frame_ids: Vec<F>,
        storyboard_selection: StoryboardSelection<F>,
        normalization: Vec<NormalizationStep>,
        parameters: Parameters,
        output_dimensions: PixelDimensions,
        output_hash: OutputHash,
    ) -> Result<Self> {
        Self::from_sequence_with_trace_and_domain(
            artifact_id,
            artifact_kind,
            evidence_class,
            algorithm,
            sequence,
            sequence.region(),
            sequence.mask().cloned(),
            selected_frame_ids,
            Some(Box::new(storyboard_selection)),
            SequenceConsumption::EveryDecodedFrame,
            normalization,
            parameters,
            output_dimensions,
            output_hash,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_sequence_with_domain<P: AsRef<[u8]>>(
        artifact_id: A,
        artifact_kind: ArtifactKind,
        evidence_class: EvidenceClass,
        algorithm: AlgorithmDescriptor,
        sequence: &FrameSequence<F, M, G, P>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
        selected_frame_ids: Vec<F>,
        consumption: SequenceConsumption,
        normalization: Vec<NormalizationStep>,
        parameters: Parameters,
        output_dimensions: PixelDimensions,
        output_hash: OutputHash,
    ) -> Result<Self> {
        Self::from_sequence_with_trace_and_domain(
            artifact_id,
            artifact_kind,
            evidence_class,
            algorithm,
            sequence,
            region,
            mask,
            selected_frame_ids,
            None,
            consumption,
            normalization,
            parameters,
            output_dimensions,
            output_hash,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_sequence_with_trace_and_domain<P: AsRef<[u8]>>(
        artifact_id: A,
        artifact_kind: ArtifactKind,
        evidence_class: EvidenceClass,
        algorithm: AlgorithmDescriptor,
        sequence: &FrameSequence<F, M, G, P>,
        region: Option<FrameRegion>,
        mask: Option<BinaryMask>,
        selected_frame_ids: Vec<F>,
        storyboard_selection: Option<Box<StoryboardSelection<F>>>,
        consumption: SequenceConsumption,
        normalization: Vec<NormalizationStep>,
        parameters: Parameters,
        output_dimensions: PixelDimensions,
        output_hash: OutputHash,
    ) -> Result<Self> {
        let source_frame_ids: Box<[F]> = sequence.source_frame_ids().to_vec().into_boxed_slice();
        // Decoding a frame is not the same as consuming it. Which of the two the
        // generator did is the declaration it makes; the population itself is
        // derived here, in one place, so that the manifest counts and any sampling
        // disclosure they carry cannot describe different evidence.
        let analyzed_frame_ids: Box<[F]> = sequence
            .frames()
            .iter()
            .map(|frame| frame.id().clone())
            .filter(|id| match consumption {
                SequenceConsumption::EveryDecodedFrame => true,
                SequenceConsumption::SelectedFramesOnly => selected_frame_ids.contains(id),
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        if analyzed_frame_ids.len() < selected_frame_ids.len() {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest selected frames are not all present in the decoded sequence",
            ));
        }
        let source_frame_count = u64::try_from(source_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "source frame count exceeds the manifest representation",
            )
        })?;
        let analyzed_frame_count = u64::try_from(analyzed_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "analyzed frame count exceeds the manifest representation",
            )
        })?;
        let omitted_frame_count = source_frame_count
            .checked_sub(analyzed_frame_count)
            .ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analyzed frame count exceeds source frame count",
                )
            })?;

        let storyboard_selection = storyboard_selection
            .map(|selection| {
                let selection = *selection;
                sequence
                    .source_indices()
                    .map_or(Ok(Box::new(selection.clone())), |indices| {
                        selection.remap_source_indices(indices).map(Box::new)
                    })
            })
            .transpose()?;
        let manifest = Self {
            artifact_id,
            artifact_kind,
            evidence_class,
            algorithm: Box::new(algorithm),
            source_frame_ids,
            analyzed_frame_ids,
            selected_frame_ids: selected_frame_ids.into_boxed_slice(),
            storyboard_selection,
            source_frame_count,
            analyzed_frame_count,
            omitted_frame_count,
            range: sequence.range(),
            markers: sequence.markers().to_vec().into_boxed_slice(),
            gaps: sequence.gaps().to_vec().into_boxed_slice(),
            region,
            mask: mask.map(Box::new),
            normalization: normalization.into_boxed_slice(),
            parameters,
            output_dimensions,
            output_hash,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    fn validate(&self) -> Result<()> {
        if self.source_frame_ids.is_empty() {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest source frames must not be empty",
            ));
        }
        validate_unique(
            &self.source_frame_ids,
            "source frame identifiers must be unique",
        )?;
        if self.analyzed_frame_ids.is_empty() {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest analyzed frames must not be empty",
            ));
        }
        validate_ordered_subsequence(
            &self.source_frame_ids,
            &self.analyzed_frame_ids,
            "analyzed frame identifiers must be unique",
            "analyzed frames must be an ordered subsequence of source frames",
        )?;
        validate_ordered_subsequence(
            &self.analyzed_frame_ids,
            &self.selected_frame_ids,
            "selected frame identifiers must be unique",
            "selected frames must be an ordered subsequence of analyzed frames",
        )?;
        self.validate_storyboard_trace()?;
        let source_count = u64::try_from(self.source_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "source frame count is too large",
            )
        })?;
        let analyzed_count = u64::try_from(self.analyzed_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "analyzed frame count is too large",
            )
        })?;
        if self.source_frame_count != source_count
            || self.analyzed_frame_count != analyzed_count
            || self.omitted_frame_count != source_count - analyzed_count
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest frame counts contradict its identifiers",
            ));
        }
        self.validate_analysis_sampling_disclosure()?;
        validate_markers(&self.markers, self.range).map_err(as_manifest_error)?;
        validate_gaps(&self.gaps, self.range).map_err(as_manifest_error)?;
        if let (Some(region), Some(mask)) = (self.region, self.mask.as_ref())
            && !region.rect().fits_within(mask.dimensions())
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest region does not fit its source mask dimensions",
            ));
        }
        Ok(())
    }

    /// The `analysis_sampling` parameter block is what agent-facing surfaces read to
    /// warn that an analysis was decimated. It must therefore describe the same
    /// evidence as the manifest counts, and it must exist whenever an analysis
    /// artifact actually dropped source frames.
    fn validate_analysis_sampling_disclosure(&self) -> Result<()> {
        let decimated = self.analyzed_frame_count < self.source_frame_count;
        let Some(value) = self.parameters.get("analysis_sampling") else {
            if decimated
                && matches!(
                    self.artifact_kind,
                    ArtifactKind::DifferenceMap | ArtifactKind::MotionHistory
                )
            {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "a decimated analysis manifest must disclose its analysis sampling",
                ));
            }
            return Ok(());
        };
        let ParameterValue::Object(values) = value else {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "analysis sampling disclosure must be an object",
            ));
        };
        for (name, expected) in [
            ("source_frame_count", self.source_frame_count),
            ("analyzed_frame_count", self.analyzed_frame_count),
        ] {
            if values.get(name) != Some(&ParameterValue::Unsigned(expected)) {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling disclosure contradicts the manifest frame counts",
                ));
            }
        }
        if !decimated {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "an undecimated manifest must not claim an analysis sampling mode",
            ));
        }
        // The counts are the smallest part of the claim. `mode` and `spacing` say
        // *how* frames were chosen, and `analyzed_source_indices` says *which* —
        // that is the part a reader would use to re-derive the analysis. Checking
        // only the counts would let a disclosure agree about how many frames were
        // examined while lying about every one of them.
        for (name, expected) in [
            ("mode", ANALYSIS_SAMPLING_MODE),
            ("spacing", ANALYSIS_SAMPLING_SPACING),
        ] {
            let ParameterValue::Text(text) = values.get(name).ok_or_else(|| {
                VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling disclosure is missing a required field",
                )
            })?
            else {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling mode and spacing must be text",
                ));
            };
            if text.as_ref() != expected {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling disclosure names a sampling scheme this crate does not produce",
                ));
            }
        }
        let Some(ParameterValue::List(indices)) = values.get("analyzed_source_indices") else {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "analysis sampling disclosure must list its analyzed source indices",
            ));
        };
        if indices.len() as u64 != self.analyzed_frame_count {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "analysis sampling disclosure contradicts the manifest frame counts",
            ));
        }
        let mut previous: Option<u64> = None;
        for (position, index) in indices.iter().enumerate() {
            let ParameterValue::Unsigned(index) = index else {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling source indices must be unsigned",
                ));
            };
            if *index >= self.source_frame_count {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling source index lies outside the manifest source frames",
                ));
            }
            // Strictly increasing: sampling selects a subset of the source order,
            // so a repeat or a reversal describes a selection that cannot have
            // happened.
            if previous.is_some_and(|previous| previous >= *index) {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling source indices must be strictly increasing",
                ));
            }
            previous = Some(*index);
            // Shape alone is not identity. A well-formed, in-range, strictly
            // increasing index list can still name frames the analysis never
            // examined — source [f0,f1,f2,f3] analyzed [f0,f2] disclosed as
            // [0,1] passes every check above while naming f0 and f1. Each index
            // must therefore resolve, through the manifest's own source order,
            // to exactly the analyzed frame it claims to identify.
            let resolved = usize::try_from(*index)
                .ok()
                .and_then(|index| self.source_frame_ids.get(index));
            if resolved != self.analyzed_frame_ids.get(position) {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling source index does not identify the analyzed frame",
                ));
            }
        }
        // An unrecognised member would be an undocumented claim about the
        // analysis riding along inside the block agents trust for exactly this.
        for name in values.keys() {
            if !ANALYSIS_SAMPLING_FIELDS.contains(&name.as_ref()) {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "analysis sampling disclosure carries an unrecognised field",
                ));
            }
        }
        Ok(())
    }

    fn validate_storyboard_trace(&self) -> Result<()> {
        let storyboard_kind = matches!(
            self.artifact_kind,
            ArtifactKind::Storyboard | ArtifactKind::BeforeDuringAfter
        );
        let Some(selection) = self.storyboard_selection.as_ref() else {
            if storyboard_kind {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "storyboard manifests require their selection trace",
                ));
            }
            return Ok(());
        };
        if !storyboard_kind {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "only storyboard artifacts may carry a storyboard selection trace",
            ));
        }
        selection.validate_local().map_err(as_manifest_error)?;
        let source_len = self.source_frame_ids.len();
        if selection.after_index() >= source_len
            || selection.continuity_segment_count() > source_len
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "storyboard selection roles exceed the manifest source frames",
            ));
        }
        for selected in selection.selected_frames() {
            if selected.frame_index() >= source_len
                || self.source_frame_ids[selected.frame_index()] != *selected.frame_id()
                || !self.range.contains(selected.timestamp())
            {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "storyboard selected trace disagrees with manifest source identity or time",
                ));
            }
        }
        if selection
            .omitted_anchors()
            .iter()
            .any(|anchor| anchor.frame_index() >= source_len)
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "storyboard omitted anchors exceed the manifest source frames",
            ));
        }
        let expected_selected: Vec<&F> = match self.artifact_kind {
            ArtifactKind::Storyboard => selection
                .selected_frames()
                .iter()
                .map(|frame| frame.frame_id())
                .collect(),
            ArtifactKind::BeforeDuringAfter => {
                let mut indices = vec![
                    selection.before_index(),
                    selection.during_index(),
                    selection.after_index(),
                ];
                indices.sort_unstable();
                indices.dedup();
                indices
                    .into_iter()
                    .map(|index| &self.source_frame_ids[index])
                    .collect()
            }
            _ => unreachable!("non-storyboard traces were rejected"),
        };
        if expected_selected
            .iter()
            .copied()
            .ne(self.selected_frame_ids.iter())
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "storyboard trace does not match manifest selected frame roles",
            ));
        }

        let validate_moment = |moment: &crate::VisualChangeMoment<F>| -> Result<()> {
            let comparison = moment.comparison();
            if comparison.earlier_frame_index() >= source_len
                || comparison.later_frame_index() >= source_len
                || self.source_frame_ids[moment.frame_index()] != *moment.frame_id()
                || !self.range.contains(moment.timestamp())
            {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "storyboard visual moment disagrees with manifest source identity or time",
                ));
            }
            let earlier_nanos = moment
                .timestamp()
                .as_nanos()
                .checked_sub(comparison.elapsed_nanos())
                .ok_or_else(|| {
                    VisionError::new(
                        ErrorCode::InvalidManifest,
                        "storyboard visual moment elapsed time precedes the manifest range",
                    )
                })?;
            let comparison_range = TimeRange::new(
                crate::Timestamp::from_nanos(earlier_nanos),
                moment.timestamp(),
            )?;
            if !self.range.contains(comparison_range.start())
                || self.gaps.iter().any(|gap| {
                    gap.range().start() <= comparison_range.end()
                        && gap.range().end() >= comparison_range.start()
                })
                || !matches!(comparison.outcome(), ComparisonOutcome::Measured(_))
            {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "storyboard visual moments must not cross declared gaps",
                ));
            }
            if let Some(selected) = selection
                .selected_frames()
                .iter()
                .find(|selected| selected.frame_index() == moment.frame_index())
                && selected.timestamp() != moment.timestamp()
            {
                return Err(VisionError::new(
                    ErrorCode::InvalidManifest,
                    "storyboard visual moment timestamp disagrees with its selected frame",
                ));
            }
            Ok(())
        };
        let summary = selection.visual_summary();
        for moment in [
            summary.first_change(),
            summary.peak_baseline_change(),
            summary.peak_adjacent_changed_area(),
        ]
        .into_iter()
        .flatten()
        {
            validate_moment(moment)?;
        }
        for (moment, reason) in [
            (summary.first_change(), SelectionReason::FirstChange),
            (
                summary.peak_baseline_change(),
                SelectionReason::PeakBaselineChange,
            ),
        ] {
            if let Some(moment) = moment {
                let represented = selection.selected_frames().iter().any(|frame| {
                    frame.frame_index() == moment.frame_index() && frame.reasons().contains(&reason)
                }) || selection.omitted_anchors().iter().any(|anchor| {
                    anchor.frame_index() == moment.frame_index() && anchor.reason() == reason
                });
                if !represented {
                    return Err(VisionError::new(
                        ErrorCode::InvalidManifest,
                        "storyboard visual moment is missing its selection reason",
                    ));
                }
            }
        }
        Ok(())
    }

    pub const fn artifact_id(&self) -> &A {
        &self.artifact_id
    }
    pub const fn artifact_kind(&self) -> ArtifactKind {
        self.artifact_kind
    }
    pub const fn evidence_class(&self) -> EvidenceClass {
        self.evidence_class
    }
    pub const fn algorithm(&self) -> &AlgorithmDescriptor {
        &self.algorithm
    }
    pub fn source_frame_ids(&self) -> &[F] {
        &self.source_frame_ids
    }
    /// The frames that contributed to this artifact, before render selection.
    pub fn analyzed_frame_ids(&self) -> &[F] {
        &self.analyzed_frame_ids
    }
    pub fn selected_frame_ids(&self) -> &[F] {
        &self.selected_frame_ids
    }
    pub fn storyboard_selection(&self) -> Option<&StoryboardSelection<F>> {
        self.storyboard_selection.as_deref()
    }
    pub const fn source_frame_count(&self) -> u64 {
        self.source_frame_count
    }
    pub const fn analyzed_frame_count(&self) -> u64 {
        self.analyzed_frame_count
    }
    /// Source frames that contributed nothing to this artifact.
    pub const fn omitted_frame_count(&self) -> u64 {
        self.omitted_frame_count
    }
    pub const fn range(&self) -> TimeRange {
        self.range
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
        self.mask.as_deref()
    }
    pub fn normalization(&self) -> &[NormalizationStep] {
        &self.normalization
    }
    pub const fn parameters(&self) -> &Parameters {
        &self.parameters
    }
    pub const fn output_dimensions(&self) -> PixelDimensions {
        self.output_dimensions
    }
    pub const fn output_hash(&self) -> OutputHash {
        self.output_hash
    }
}

impl<'de, A, F, M, G> Deserialize<'de> for ArtifactManifest<A, F, M, G>
where
    A: Deserialize<'de>,
    F: Deserialize<'de> + Clone + Eq,
    M: Deserialize<'de> + Clone + Eq,
    G: Deserialize<'de> + Clone + Eq,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(
            bound(
                deserialize = "A: Deserialize<'de>, F: Deserialize<'de> + Eq, M: Deserialize<'de>, G: Deserialize<'de>"
            ),
            deny_unknown_fields
        )]
        struct Wire<A, F, M, G> {
            artifact_id: A,
            artifact_kind: ArtifactKind,
            evidence_class: EvidenceClass,
            algorithm: AlgorithmDescriptor,
            source_frame_ids: Box<[F]>,
            analyzed_frame_ids: Box<[F]>,
            selected_frame_ids: Box<[F]>,
            storyboard_selection: Option<Box<StoryboardSelection<F>>>,
            source_frame_count: u64,
            analyzed_frame_count: u64,
            omitted_frame_count: u64,
            range: TimeRange,
            markers: Box<[Marker<M>]>,
            gaps: Box<[DeclaredGap<G>]>,
            region: Option<FrameRegion>,
            mask: Option<Box<BinaryMask>>,
            normalization: Box<[NormalizationStep]>,
            parameters: Parameters,
            output_dimensions: PixelDimensions,
            output_hash: OutputHash,
        }
        let wire = Wire::deserialize(deserializer)?;
        let manifest = Self {
            artifact_id: wire.artifact_id,
            artifact_kind: wire.artifact_kind,
            evidence_class: wire.evidence_class,
            algorithm: Box::new(wire.algorithm),
            source_frame_ids: wire.source_frame_ids,
            analyzed_frame_ids: wire.analyzed_frame_ids,
            selected_frame_ids: wire.selected_frame_ids,
            storyboard_selection: wire.storyboard_selection,
            source_frame_count: wire.source_frame_count,
            analyzed_frame_count: wire.analyzed_frame_count,
            omitted_frame_count: wire.omitted_frame_count,
            range: wire.range,
            markers: wire.markers,
            gaps: wire.gaps,
            region: wire.region,
            mask: wire.mask,
            normalization: wire.normalization,
            parameters: wire.parameters,
            output_dimensions: wire.output_dimensions,
            output_hash: wire.output_hash,
        };
        manifest.validate().map_err(serde::de::Error::custom)?;
        Ok(manifest)
    }
}

fn validate_unique<T: Eq>(values: &[T], message: &'static str) -> Result<()> {
    for (index, value) in values.iter().enumerate() {
        if values[..index].contains(value) {
            return Err(VisionError::at(ErrorCode::InvalidManifest, message, index));
        }
    }
    Ok(())
}

fn validate_ordered_subsequence<F: Eq>(
    source: &[F],
    subset: &[F],
    unique_message: &'static str,
    subsequence_message: &'static str,
) -> Result<()> {
    validate_unique(subset, unique_message)?;
    let mut source_index = 0;
    for (index, subset_id) in subset.iter().enumerate() {
        let Some(offset) = source[source_index..]
            .iter()
            .position(|source_id| source_id == subset_id)
        else {
            return Err(VisionError::at(
                ErrorCode::InvalidManifest,
                subsequence_message,
                index,
            ));
        };
        source_index += offset + 1;
    }
    Ok(())
}

fn as_manifest_error(error: VisionError) -> VisionError {
    VisionError {
        code: ErrorCode::InvalidManifest,
        message: error.message,
        index: error.index,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registries_have_one_stable_wire_table() {
        fn check<T>(values: &[T], name: impl Fn(T) -> &'static str)
        where
            T: Copy + fmt::Display + Serialize + for<'de> Deserialize<'de> + Eq + fmt::Debug,
        {
            for value in values {
                let json = serde_json::to_string(value).unwrap();
                assert_eq!(json, format!("\"{}\"", name(*value)));
                assert_eq!(serde_json::from_str::<T>(&json).unwrap(), *value);
                assert_eq!(value.to_string(), name(*value));
            }
        }
        check(ArtifactKind::ALL, ArtifactKind::as_str);
        check(EvidenceClass::ALL, EvidenceClass::as_str);
        check(NormalizationKind::ALL, NormalizationKind::as_str);
    }

    #[test]
    fn parameters_are_finite_canonical_and_recursive() {
        assert!(FiniteNumber::new(f64::NAN).is_err());
        assert_eq!(
            FiniteNumber::new(-0.0).unwrap().get().to_bits(),
            0.0_f64.to_bits()
        );
        assert!(
            serde_json::from_str::<ParameterValue>(
                r#"{"type":"object","value":{"":{"type":"bool","value":true}}}"#
            )
            .is_err()
        );
    }

    #[test]
    fn output_hash_requires_canonical_sha256_hex() {
        let hash = OutputHash::from_bytes([0xab; 32]);
        let encoded = hash.to_string();
        assert_eq!(encoded.len(), 64);
        assert_eq!(encoded.parse::<OutputHash>().unwrap(), hash);
        assert!(encoded.to_uppercase().parse::<OutputHash>().is_err());
        assert!("ab".parse::<OutputHash>().is_err());
    }
}
