use std::{collections::BTreeMap, fmt, str::FromStr};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    BinaryMask, DeclaredGap, ErrorCode, FrameRegion, FrameSequence, Marker, PixelDimensions,
    Result, TimeRange, VisionError,
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

/// Reproducible machine-readable provenance for one generated artifact.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> {
    artifact_id: ArtifactId,
    artifact_kind: ArtifactKind,
    evidence_class: EvidenceClass,
    algorithm: AlgorithmDescriptor,
    source_frame_ids: Box<[FrameId]>,
    selected_frame_ids: Box<[FrameId]>,
    source_frame_count: u64,
    omitted_frame_count: u64,
    range: TimeRange,
    markers: Box<[Marker<MarkerId>]>,
    gaps: Box<[DeclaredGap<GapId>]>,
    region: Option<FrameRegion>,
    mask: Option<BinaryMask>,
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
        Self::from_sequence_with_domain(
            artifact_id,
            artifact_kind,
            evidence_class,
            algorithm,
            sequence,
            sequence.region(),
            sequence.mask().cloned(),
            selected_frame_ids,
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
        normalization: Vec<NormalizationStep>,
        parameters: Parameters,
        output_dimensions: PixelDimensions,
        output_hash: OutputHash,
    ) -> Result<Self> {
        let source_frame_ids: Box<[F]> = sequence
            .frames()
            .iter()
            .map(|frame| frame.id().clone())
            .collect();
        let source_frame_count = u64::try_from(source_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "source frame count exceeds the manifest representation",
            )
        })?;
        let selected_count = u64::try_from(selected_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "selected frame count exceeds the manifest representation",
            )
        })?;
        let omitted_frame_count =
            source_frame_count
                .checked_sub(selected_count)
                .ok_or_else(|| {
                    VisionError::new(
                        ErrorCode::InvalidManifest,
                        "selected frame count exceeds source frame count",
                    )
                })?;

        let manifest = Self {
            artifact_id,
            artifact_kind,
            evidence_class,
            algorithm,
            source_frame_ids,
            selected_frame_ids: selected_frame_ids.into_boxed_slice(),
            source_frame_count,
            omitted_frame_count,
            range: sequence.range(),
            markers: sequence.markers().to_vec().into_boxed_slice(),
            gaps: sequence.gaps().to_vec().into_boxed_slice(),
            region,
            mask,
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
        validate_selected_subsequence(&self.source_frame_ids, &self.selected_frame_ids)?;
        let source_count = u64::try_from(self.source_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "source frame count is too large",
            )
        })?;
        let selected_count = u64::try_from(self.selected_frame_ids.len()).map_err(|_| {
            VisionError::new(
                ErrorCode::InvalidManifest,
                "selected frame count is too large",
            )
        })?;
        if self.source_frame_count != source_count
            || self.omitted_frame_count != source_count - selected_count
        {
            return Err(VisionError::new(
                ErrorCode::InvalidManifest,
                "manifest frame counts contradict its identifiers",
            ));
        }
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
    pub fn selected_frame_ids(&self) -> &[F] {
        &self.selected_frame_ids
    }
    pub const fn source_frame_count(&self) -> u64 {
        self.source_frame_count
    }
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
        self.mask.as_ref()
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
        #[serde(bound(
            deserialize = "A: Deserialize<'de>, F: Deserialize<'de>, M: Deserialize<'de>, G: Deserialize<'de>"
        ))]
        struct Wire<A, F, M, G> {
            artifact_id: A,
            artifact_kind: ArtifactKind,
            evidence_class: EvidenceClass,
            algorithm: AlgorithmDescriptor,
            source_frame_ids: Box<[F]>,
            selected_frame_ids: Box<[F]>,
            source_frame_count: u64,
            omitted_frame_count: u64,
            range: TimeRange,
            markers: Box<[Marker<M>]>,
            gaps: Box<[DeclaredGap<G>]>,
            region: Option<FrameRegion>,
            mask: Option<BinaryMask>,
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
            algorithm: wire.algorithm,
            source_frame_ids: wire.source_frame_ids,
            selected_frame_ids: wire.selected_frame_ids,
            source_frame_count: wire.source_frame_count,
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

fn validate_selected_subsequence<F: Eq>(source: &[F], selected: &[F]) -> Result<()> {
    validate_unique(selected, "selected frame identifiers must be unique")?;
    let mut source_index = 0;
    for (index, selected_id) in selected.iter().enumerate() {
        let Some(offset) = source[source_index..]
            .iter()
            .position(|source_id| source_id == selected_id)
        else {
            return Err(VisionError::at(
                ErrorCode::InvalidManifest,
                "selected frames must be an ordered subsequence of source frames",
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
