//! Typed application contracts for bounded, source-derived visual artifacts.
//!
//! The authoritative provenance value is the generic `temporal-vision` manifest
//! specialized with Krometrail identities. This module deliberately does not
//! project or copy that manifest into an application-specific DTO.

use std::{
    collections::HashSet,
    num::{NonZeroU32, NonZeroU64},
    sync::Arc,
    time::Instant,
};

use serde::{Deserialize, Deserializer, Serialize};

use crate::{
    ArtifactId, CancellationSignal, DeviceScaleFactor, FrameId, GapId, InteractionId,
    KrometrailError, MarkerId, NavigationId, NonEmptyText, PixelDimensions, PortFuture,
    ResolvedRange, Result, SessionTime, error::invalid, validation::deserialize_validated,
};

/// Shared defaults for direct artifact generation and the temporal bundle policy.
pub const DEFAULT_ARTIFACT_TILE_LIMIT: u8 = 8;
pub const MIN_ANALYSIS_DOWNSCALE_FACTOR: u8 = 2;
pub const MAX_ANALYSIS_DOWNSCALE_FACTOR: u8 = 8;
pub const MIN_STORYBOARD_TILE_LIMIT: u8 = 3;
pub const MAX_STORYBOARD_TILE_LIMIT: u8 = 12;
pub const MIN_FILMSTRIP_TILE_LIMIT: u8 = 1;
pub const MAX_FILMSTRIP_TILE_LIMIT: u8 = 24;
pub const DEFAULT_ARTIFACT_NOISE_FLOOR: u16 = 512;
pub const DEFAULT_STORYBOARD_MAX_WIDTH: u32 = 1920;
pub const DEFAULT_STORYBOARD_MAX_HEIGHT: u32 = 2048;
pub const DEFAULT_STORYBOARD_MAX_BYTES: u64 = 16 * 1024 * 1024;
pub const DEFAULT_DIFFERENCE_MAP_MAX_WIDTH: u32 = 8192;
pub const DEFAULT_DIFFERENCE_MAP_MAX_HEIGHT: u32 = 8192;
pub const DEFAULT_DIFFERENCE_MAP_MAX_BYTES: u64 = 64 * 1024 * 1024;
pub const DEFAULT_ARTIFACT_BLACK_BACKGROUND: temporal_vision::Rgb8 =
    temporal_vision::Rgb8::new(0, 0, 0);

const DEFAULT_STORYBOARD_TITLE: &str = "TEMPORAL STORYBOARD";
const DEFAULT_STORYBOARD_SOURCE: &str = "KROMETRAIL RETAINED SOURCE FRAMES";

pub(crate) fn default_labels() -> ArtifactLabelsRequest {
    ArtifactLabelsRequest::new(
        NonEmptyText::new(DEFAULT_STORYBOARD_TITLE).expect("default title is non-empty"),
        NonEmptyText::new(DEFAULT_STORYBOARD_SOURCE).expect("default source is non-empty"),
    )
}

fn default_normalization() -> NormalizationRequest {
    NormalizationRequest::new(
        None,
        DEFAULT_ARTIFACT_BLACK_BACKGROUND,
        AnalysisScale::FitLimits,
    )
    .expect("default artifact normalization is valid")
}

pub(crate) fn default_output() -> OutputLimitsRequest {
    OutputLimitsRequest::new(
        DEFAULT_STORYBOARD_MAX_WIDTH,
        DEFAULT_STORYBOARD_MAX_HEIGHT,
        DEFAULT_STORYBOARD_MAX_BYTES,
    )
    .expect("default artifact output is valid")
}

fn default_difference_output() -> OutputLimitsRequest {
    OutputLimitsRequest::new(
        DEFAULT_DIFFERENCE_MAP_MAX_WIDTH,
        DEFAULT_DIFFERENCE_MAP_MAX_HEIGHT,
        DEFAULT_DIFFERENCE_MAP_MAX_BYTES,
    )
    .expect("default difference-map output is valid")
}

fn default_frequency_mode() -> temporal_vision::FrequencyMode {
    temporal_vision::FrequencyMode::NormalizedFrequency
}

fn default_artifact_sampling() -> ArtifactSampling {
    ArtifactSampling::UniformBounded
}

fn default_frame_selector() -> FrameSelector {
    FrameSelector::First
}

pub(crate) fn default_analysis_scale() -> AnalysisScale {
    AnalysisScale::Identity
}

pub(crate) fn default_artifact_tile_limit() -> u8 {
    DEFAULT_ARTIFACT_TILE_LIMIT
}

fn default_artifact_noise_floor() -> u16 {
    DEFAULT_ARTIFACT_NOISE_FLOOR
}

pub(crate) fn default_black_background() -> temporal_vision::Rgb8 {
    DEFAULT_ARTIFACT_BLACK_BACKGROUND
}

fn default_include_orientation() -> bool {
    true
}

pub(crate) fn default_artifact_failure_policy() -> ArtifactFailurePolicy {
    ArtifactFailurePolicy::AllowPartial
}

fn default_motion_decay_peak() -> u16 {
    u16::MAX
}

fn default_motion_decay_half_life() -> u8 {
    1
}

fn default_motion_reference_strength() -> u8 {
    u8::MAX
}

fn default_accent() -> temporal_vision::Rgb8 {
    temporal_vision::Rgb8::new(255, 128, 0)
}

/// Exact browser-agnostic provenance carried across every application/store boundary.
pub type ArtifactManifest =
    temporal_vision::ArtifactManifest<ArtifactId, FrameId, ArtifactMarkerId, GapId>;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "source", content = "id", rename_all = "snake_case")]
pub enum ArtifactMarkerId {
    Interaction(InteractionId),
    Navigation(NavigationId),
    Marker(MarkerId),
    Caller(NonEmptyText),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ArtifactMarker {
    id: ArtifactMarkerId,
    session_time: SessionTime,
    kind: NonEmptyText,
    label: NonEmptyText,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct ArtifactMarkerWire {
    id: ArtifactMarkerId,
    session_time: SessionTime,
    kind: NonEmptyText,
    label: NonEmptyText,
}

impl ArtifactMarker {
    pub const fn new(
        id: ArtifactMarkerId,
        session_time: SessionTime,
        kind: NonEmptyText,
        label: NonEmptyText,
    ) -> Self {
        Self {
            id,
            session_time,
            kind,
            label,
        }
    }

    pub const fn id(&self) -> &ArtifactMarkerId {
        &self.id
    }
    pub const fn session_time(&self) -> SessionTime {
        self.session_time
    }
    pub const fn kind(&self) -> &NonEmptyText {
        &self.kind
    }
    pub const fn label(&self) -> &NonEmptyText {
        &self.label
    }
}

impl<'de> Deserialize<'de> for ArtifactMarker {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = ArtifactMarkerWire::deserialize(deserializer)?;
        Ok(Self::new(wire.id, wire.session_time, wire.kind, wire.label))
    }
}

crate::validation::delegate_json_schema!(ArtifactMarker => ArtifactMarkerWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactFailurePolicy {
    RequireAll,
    AllowPartial,
}

/// Requested integer analysis scale. `FitLimits` is resolved before cache lookup.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "scale", content = "factor", rename_all = "snake_case")]
pub enum AnalysisScale {
    Identity,
    Down(
        #[schemars(range(
            min = MIN_ANALYSIS_DOWNSCALE_FACTOR,
            max = MAX_ANALYSIS_DOWNSCALE_FACTOR
        ))]
        u8,
    ),
    FitLimits,
}

impl AnalysisScale {
    pub fn validate(self) -> Result<()> {
        match self {
            Self::Identity | Self::FitLimits => Ok(()),
            Self::Down(MIN_ANALYSIS_DOWNSCALE_FACTOR..=MAX_ANALYSIS_DOWNSCALE_FACTOR) => Ok(()),
            Self::Down(_) => Err(invalid(
                "analysis downscale factor must be between two and eight",
            )),
        }
    }
}

/// Filmstrip display scaling: preserve source pixels or use an exact integer downscale factor.
#[derive(schemars::JsonSchema)]
#[schemars(rename = "ExplicitAnalysisScale")]
#[allow(dead_code)]
#[serde(tag = "scale", content = "factor", rename_all = "snake_case")]
pub(crate) enum ExplicitAnalysisScale {
    Identity,
    Down(
        #[schemars(range(
            min = MIN_ANALYSIS_DOWNSCALE_FACTOR,
            max = MAX_ANALYSIS_DOWNSCALE_FACTOR
        ))]
        u8,
    ),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArtifactLabelsRequest {
    pub title: NonEmptyText,
    pub source: NonEmptyText,
}

impl ArtifactLabelsRequest {
    pub const fn new(title: NonEmptyText, source: NonEmptyText) -> Self {
        Self { title, source }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NormalizationRequest {
    pub crop: Option<temporal_vision::PixelRect>,
    pub background: temporal_vision::Rgb8,
    pub scale: AnalysisScale,
}

impl NormalizationRequest {
    pub fn new(
        crop: Option<temporal_vision::PixelRect>,
        background: temporal_vision::Rgb8,
        scale: AnalysisScale,
    ) -> Result<Self> {
        scale.validate()?;
        Ok(Self {
            crop,
            background,
            scale,
        })
    }

    fn validate(self) -> Result<()> {
        self.scale.validate()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct OutputLimitsRequest {
    max_width: NonZeroU32,
    max_height: NonZeroU32,
    max_encoded_bytes: NonZeroU64,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct OutputLimitsWire {
    max_width: u32,
    max_height: u32,
    max_encoded_bytes: u64,
}

impl OutputLimitsRequest {
    pub fn new(max_width: u32, max_height: u32, max_encoded_bytes: u64) -> Result<Self> {
        Ok(Self {
            max_width: NonZeroU32::new(max_width)
                .ok_or_else(|| invalid("artifact output width must be non-zero"))?,
            max_height: NonZeroU32::new(max_height)
                .ok_or_else(|| invalid("artifact output height must be non-zero"))?,
            max_encoded_bytes: NonZeroU64::new(max_encoded_bytes)
                .ok_or_else(|| invalid("artifact encoded-byte limit must be non-zero"))?,
        })
    }

    pub const fn max_width(self) -> u32 {
        self.max_width.get()
    }
    pub const fn max_height(self) -> u32 {
        self.max_height.get()
    }
    pub const fn max_encoded_bytes(self) -> u64 {
        self.max_encoded_bytes.get()
    }
}

impl<'de> Deserialize<'de> for OutputLimitsRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(deserializer, |wire: OutputLimitsWire| {
            Self::new(wire.max_width, wire.max_height, wire.max_encoded_bytes)
        })
    }
}

crate::validation::delegate_json_schema!(OutputLimitsRequest => OutputLimitsWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "frame", content = "id", rename_all = "snake_case")]
pub enum FrameSelector {
    First,
    Last,
    Frame(FrameId),
}

temporal_vision::stable_registry! {
    /// Whether an artifact analyzes every retained frame or a bounded uniform subset.
    pub enum ArtifactSampling {
        Exhaustive => "exhaustive",
        UniformBounded => "uniform_bounded",
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct StoryboardRequest {
    pub anchor: SessionTime,
    #[serde(default = "default_artifact_tile_limit")]
    pub tile_limit: u8,
    #[serde(default = "default_artifact_noise_floor")]
    pub noise_floor: u16,
    #[serde(default = "default_normalization")]
    pub normalization: NormalizationRequest,
    #[serde(default = "default_labels")]
    pub labels: ArtifactLabelsRequest,
    #[serde(default = "default_include_orientation")]
    pub include_orientation: bool,
    #[serde(default = "default_output")]
    pub output: OutputLimitsRequest,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct StoryboardRequestWire {
    #[serde(default)]
    anchor: Option<SessionTime>,
    #[serde(default = "default_artifact_tile_limit")]
    #[schemars(range(min = MIN_STORYBOARD_TILE_LIMIT, max = MAX_STORYBOARD_TILE_LIMIT))]
    tile_limit: u8,
    #[serde(default = "default_artifact_noise_floor")]
    noise_floor: u16,
    #[serde(default = "default_normalization")]
    normalization: NormalizationRequest,
    #[serde(default = "default_labels")]
    labels: ArtifactLabelsRequest,
    #[serde(default = "default_include_orientation")]
    include_orientation: bool,
    #[serde(default = "default_output")]
    output: OutputLimitsRequest,
}

impl<'de> Deserialize<'de> for StoryboardRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = StoryboardRequestWire::deserialize(deserializer)?;
        Ok(Self {
            anchor: wire.anchor.unwrap_or(SessionTime::ZERO),
            tile_limit: wire.tile_limit,
            noise_floor: wire.noise_floor,
            normalization: wire.normalization,
            labels: wire.labels,
            include_orientation: wire.include_orientation,
            output: wire.output,
        })
    }
}

crate::validation::delegate_json_schema!(StoryboardRequest => StoryboardRequestWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DifferenceMapRequest {
    #[serde(default = "default_frame_selector")]
    pub reference: FrameSelector,
    #[serde(default = "default_frequency_mode")]
    pub frequency_mode: temporal_vision::FrequencyMode,
    #[serde(default = "default_artifact_sampling")]
    pub sampling: ArtifactSampling,
    #[serde(default)]
    pub repeated_change_separation_nanos: Option<u64>,
    #[serde(default = "default_artifact_noise_floor")]
    pub noise_floor: u16,
    #[serde(default = "default_normalization")]
    pub normalization: NormalizationRequest,
    #[serde(default = "default_black_background")]
    pub canvas_background: temporal_vision::Rgb8,
    #[serde(default = "default_difference_output")]
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegionFilmstripRequest {
    pub region: temporal_vision::RegionDefinition,
    #[serde(default)]
    pub mask: Option<temporal_vision::BinaryMask>,
    #[serde(default)]
    pub anchor: SessionTime,
    #[serde(default = "default_artifact_tile_limit")]
    pub tile_limit: u8,
    #[serde(default)]
    pub locator: Option<FrameId>,
    #[serde(default = "default_black_background")]
    pub background: temporal_vision::Rgb8,
    #[serde(default = "default_black_background")]
    pub padding: temporal_vision::Rgb8,
    #[serde(default = "default_analysis_scale")]
    pub display_scale: AnalysisScale,
    #[serde(default = "default_labels")]
    pub labels: ArtifactLabelsRequest,
    #[serde(default = "default_output")]
    pub output: OutputLimitsRequest,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
struct RegionFilmstripRequestWire {
    region: temporal_vision::RegionDefinition,
    #[serde(default)]
    mask: Option<temporal_vision::BinaryMask>,
    #[serde(default)]
    anchor: Option<SessionTime>,
    #[serde(default = "default_artifact_tile_limit")]
    #[schemars(range(min = MIN_FILMSTRIP_TILE_LIMIT, max = MAX_FILMSTRIP_TILE_LIMIT))]
    tile_limit: u8,
    #[serde(default)]
    locator: Option<FrameId>,
    #[serde(default = "default_black_background")]
    background: temporal_vision::Rgb8,
    #[serde(default = "default_black_background")]
    padding: temporal_vision::Rgb8,
    #[serde(default = "default_analysis_scale")]
    #[schemars(with = "ExplicitAnalysisScale")]
    display_scale: AnalysisScale,
    #[serde(default = "default_labels")]
    labels: ArtifactLabelsRequest,
    #[serde(default = "default_output")]
    output: OutputLimitsRequest,
}

impl<'de> Deserialize<'de> for RegionFilmstripRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let wire = RegionFilmstripRequestWire::deserialize(deserializer)?;
        Ok(Self {
            region: wire.region,
            mask: wire.mask,
            anchor: wire.anchor.unwrap_or(SessionTime::ZERO),
            tile_limit: wire.tile_limit,
            locator: wire.locator,
            background: wire.background,
            padding: wire.padding,
            display_scale: wire.display_scale,
            labels: wire.labels,
            output: wire.output,
        })
    }
}

crate::validation::delegate_json_schema!(RegionFilmstripRequest => RegionFilmstripRequestWire);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MotionHistoryRequest {
    #[serde(default = "default_frame_selector")]
    pub reference: FrameSelector,
    #[serde(default = "default_artifact_sampling")]
    pub sampling: ArtifactSampling,
    #[serde(default = "default_artifact_noise_floor")]
    pub noise_floor: u16,
    #[serde(default = "default_normalization")]
    pub normalization: NormalizationRequest,
    #[serde(default = "default_motion_decay_peak")]
    pub decay_peak: u16,
    #[serde(default = "default_motion_decay_half_life")]
    pub decay_half_life_ranks: u8,
    #[serde(default = "default_motion_reference_strength")]
    pub reference_strength: u8,
    #[serde(default = "default_accent")]
    pub accent: temporal_vision::Rgb8,
    #[serde(default = "default_black_background")]
    pub outline: temporal_vision::Rgb8,
    #[serde(default = "default_labels")]
    pub labels: ArtifactLabelsRequest,
    #[serde(default = "default_output")]
    pub output: OutputLimitsRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "generator", rename_all = "snake_case")]
pub enum ArtifactGeneratorRequest {
    Storyboard(StoryboardRequest),
    DifferenceMap(DifferenceMapRequest),
    RegionFilmstrip(RegionFilmstripRequest),
    MotionHistory(MotionHistoryRequest),
}

impl ArtifactGeneratorRequest {
    fn validate(&self, range: &ResolvedRange) -> Result<()> {
        let validate_selector = |selector: FrameSelector| match selector {
            FrameSelector::First | FrameSelector::Last => Ok(()),
            FrameSelector::Frame(id) if range.frame_ids.contains(&id) => Ok(()),
            FrameSelector::Frame(_) => Err(invalid(
                "artifact reference frame is outside the resolved range",
            )),
        };
        match self {
            Self::Storyboard(request) => {
                if !(MIN_STORYBOARD_TILE_LIMIT..=MAX_STORYBOARD_TILE_LIMIT)
                    .contains(&request.tile_limit)
                {
                    return Err(invalid(
                        "storyboard tile limit must be between three and twelve",
                    ));
                }
                if !range.resolved_range.contains(request.anchor) {
                    return Err(invalid("storyboard anchor is outside the resolved range"));
                }
                request.normalization.validate()
            }
            Self::DifferenceMap(request) => {
                validate_selector(request.reference)?;
                request.normalization.validate()
            }
            Self::RegionFilmstrip(request) => {
                if !(MIN_FILMSTRIP_TILE_LIMIT..=MAX_FILMSTRIP_TILE_LIMIT)
                    .contains(&request.tile_limit)
                {
                    return Err(invalid(
                        "filmstrip tile limit must be between one and twenty-four",
                    ));
                }
                if !range.resolved_range.contains(request.anchor) {
                    return Err(invalid("filmstrip anchor is outside the resolved range"));
                }
                if request
                    .locator
                    .is_some_and(|id| !range.frame_ids.contains(&id))
                {
                    return Err(invalid(
                        "filmstrip locator frame is outside the resolved range",
                    ));
                }
                request.display_scale.validate()?;
                if request.display_scale == AnalysisScale::FitLimits {
                    return Err(invalid("filmstrip display scale must be explicit"));
                }
                if let Some(mask) = &request.mask {
                    let bounds = mask
                        .bounds()
                        .map_err(|error| invalid(error.to_string()))?
                        .ok_or_else(|| {
                            invalid("filmstrip mask must select at least one source pixel")
                        })?;
                    let matching_source_region = matches!(
                        &request.region,
                        temporal_vision::RegionDefinition::FixedSourceImage { rect }
                            if rect.x() == i64::from(bounds.x())
                                && rect.y() == i64::from(bounds.y())
                                && rect.width() == bounds.width()
                                && rect.height() == bounds.height()
                    );
                    if !matching_source_region {
                        return Err(invalid(
                            "filmstrip mask requires its exact fixed source-image bounds",
                        ));
                    }
                }
                Ok(())
            }
            Self::MotionHistory(request) => {
                validate_selector(request.reference)?;
                request.normalization.validate()?;
                if request.decay_half_life_ranks == 0 {
                    return Err(invalid("motion-history half-life ranks must be non-zero"));
                }
                Ok(())
            }
        }
    }

    /// Output kinds in deterministic order. Orientation remains a storyboard output.
    pub fn output_kinds(&self) -> &[temporal_vision::ArtifactKind] {
        use temporal_vision::ArtifactKind;
        const STORYBOARD: &[ArtifactKind] = &[ArtifactKind::Storyboard];
        const STORYBOARD_ORIENTATION: &[ArtifactKind] =
            &[ArtifactKind::Storyboard, ArtifactKind::BeforeDuringAfter];
        const DIFFERENCE: &[ArtifactKind] = &[ArtifactKind::DifferenceMap];
        const FILMSTRIP: &[ArtifactKind] = &[ArtifactKind::RegionFilmstrip];
        const MOTION: &[ArtifactKind] = &[ArtifactKind::MotionHistory];
        match self {
            Self::Storyboard(request) if request.include_orientation => STORYBOARD_ORIENTATION,
            Self::Storyboard(_) => STORYBOARD,
            Self::DifferenceMap(_) => DIFFERENCE,
            Self::RegionFilmstrip(_) => FILMSTRIP,
            Self::MotionHistory(_) => MOTION,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ArtifactGenerationRequest {
    range: ResolvedRange,
    markers: Vec<ArtifactMarker>,
    generators: Vec<ArtifactGeneratorRequest>,
    failure_policy: ArtifactFailurePolicy,
}

#[derive(Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
#[allow(dead_code)]
struct ArtifactGenerationRequestWire {
    range: ResolvedRange,
    #[serde(default)]
    markers: Vec<ArtifactMarker>,
    generators: Vec<ArtifactGeneratorRequest>,
    #[serde(default = "default_artifact_failure_policy")]
    failure_policy: ArtifactFailurePolicy,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactGenerationRequestDeserializeWire {
    range: ResolvedRange,
    #[serde(default)]
    markers: Vec<ArtifactMarker>,
    generators: Vec<serde_json::Value>,
    #[serde(default = "default_artifact_failure_policy")]
    failure_policy: ArtifactFailurePolicy,
}

impl ArtifactGenerationRequest {
    pub fn new(
        range: ResolvedRange,
        markers: Vec<ArtifactMarker>,
        generators: Vec<ArtifactGeneratorRequest>,
        failure_policy: ArtifactFailurePolicy,
    ) -> Result<Self> {
        if generators.is_empty() {
            return Err(invalid(
                "artifact request must contain at least one generator",
            ));
        }
        let mut marker_ids = HashSet::with_capacity(markers.len());
        for marker in &markers {
            if !marker_ids.insert(marker.id.clone()) {
                return Err(invalid("artifact marker identifiers must be unique"));
            }
            if !range.resolved_range.contains(marker.session_time) {
                return Err(invalid("artifact marker is outside the resolved range"));
            }
        }
        let mut generators = generators;
        for generator in &mut generators {
            match generator {
                ArtifactGeneratorRequest::Storyboard(request)
                    if !range.resolved_range.contains(request.anchor)
                        && request.anchor == SessionTime::ZERO =>
                {
                    request.anchor = range.resolved_anchor.effective_time;
                }
                ArtifactGeneratorRequest::RegionFilmstrip(request)
                    if !range.resolved_range.contains(request.anchor)
                        && request.anchor == SessionTime::ZERO =>
                {
                    request.anchor = range.resolved_anchor.effective_time;
                }
                _ => {}
            }
        }
        for generator in &generators {
            generator.validate(&range)?;
        }
        Ok(Self {
            range,
            markers,
            generators,
            failure_policy,
        })
    }

    pub const fn range(&self) -> &ResolvedRange {
        &self.range
    }
    pub fn markers(&self) -> &[ArtifactMarker] {
        &self.markers
    }
    pub fn generators(&self) -> &[ArtifactGeneratorRequest] {
        &self.generators
    }
    pub const fn failure_policy(&self) -> ArtifactFailurePolicy {
        self.failure_policy
    }
}

impl<'de> Deserialize<'de> for ArtifactGenerationRequest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        deserialize_validated(
            deserializer,
            |wire: ArtifactGenerationRequestDeserializeWire| {
                let mut generators = Vec::with_capacity(wire.generators.len());
                for value in wire.generators {
                    if let Some(generator) = value.get("generator").and_then(|value| value.as_str())
                        && matches!(generator, "storyboard" | "region_filmstrip")
                        && let Some(anchor) = value.get("anchor").filter(|value| !value.is_null())
                    {
                        let anchor: SessionTime = serde_json::from_value(anchor.clone())
                            .map_err(|error| invalid(error.to_string()))?;
                        if !wire.range.resolved_range.contains(anchor) {
                            return Err(invalid("artifact anchor is outside the resolved range"));
                        }
                    }
                    generators.push(
                        serde_json::from_value(value)
                            .map_err(|error| invalid(error.to_string()))?,
                    );
                }
                Self::new(wire.range, wire.markers, generators, wire.failure_policy)
            },
        )
    }
}

crate::validation::delegate_json_schema!(ArtifactGenerationRequest => ArtifactGenerationRequestWire);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactCacheDisposition {
    Hit,
    Generated,
    RegeneratedAfterInvalidation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct VisualEpoch {
    pub index: u32,
    pub frame_ids: Vec<FrameId>,
    pub image: PixelDimensions,
    pub viewport: PixelDimensions,
    pub device_scale_factor: DeviceScaleFactor,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactHandle {
    pub artifact_id: ArtifactId,
    pub cache: ArtifactCacheDisposition,
    pub media_type: NonEmptyText,
    pub encoded_byte_len: u64,
    pub manifest: ArtifactManifest,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ArtifactOutcome {
    Available {
        epoch_index: u32,
        generator_index: u32,
        artifact: ArtifactHandle,
    },
    Unavailable {
        epoch_index: u32,
        generator_index: u32,
        artifact_kind: temporal_vision::ArtifactKind,
        error: KrometrailError,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactGenerationResult {
    pub range: ResolvedRange,
    pub epochs: Vec<VisualEpoch>,
    pub outcomes: Vec<ArtifactOutcome>,
}

#[derive(Clone, Default)]
pub struct ArtifactGenerationContext {
    pub deadline: Option<Instant>,
    pub cancellation: Option<Arc<dyn CancellationSignal>>,
    pub epoch_selection: ArtifactEpochSelection,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ArtifactEpochSelection {
    #[default]
    All,
    Anchor(SessionTime),
}

impl ArtifactGenerationContext {
    pub fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .is_some_and(|signal| signal.is_cancelled())
    }
}

pub trait ArtifactGeneration: Send + Sync {
    fn generate(
        &self,
        request: ArtifactGenerationRequest,
        context: ArtifactGenerationContext,
    ) -> PortFuture<'_, Result<ArtifactGenerationResult>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RangeResolutionOptions, SessionId, SessionRange, TargetId, TemporalRangeAnchorKind,
    };

    fn range() -> ResolvedRange {
        ResolvedRange::new(
            SessionId::from_uuid(uuid::Uuid::from_u128(1)),
            TargetId::from_uuid(uuid::Uuid::from_u128(2)),
            TemporalRangeAnchorKind::SessionTime,
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(5)).unwrap(),
            SessionRange::new(SessionTime::from_nanos(1), SessionTime::from_nanos(5)).unwrap(),
            vec![FrameId::from_uuid(uuid::Uuid::from_u128(3))],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }

    fn output() -> OutputLimitsRequest {
        OutputLimitsRequest::new(1024, 1024, 1_000_000).unwrap()
    }
    fn normalization() -> NormalizationRequest {
        NormalizationRequest::new(
            None,
            temporal_vision::Rgb8::new(0, 0, 0),
            AnalysisScale::Identity,
        )
        .unwrap()
    }
    fn labels() -> ArtifactLabelsRequest {
        ArtifactLabelsRequest::new(
            NonEmptyText::new("artifact").unwrap(),
            NonEmptyText::new("fixture").unwrap(),
        )
    }
    fn storyboard() -> ArtifactGeneratorRequest {
        ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(2),
            tile_limit: 3,
            noise_floor: 0,
            normalization: normalization(),
            labels: labels(),
            include_orientation: true,
            output: output(),
        })
    }

    #[test]
    fn validates_request_before_frame_io_and_keeps_orientation_coupled() {
        let request = ArtifactGenerationRequest::new(
            range(),
            vec![],
            vec![storyboard()],
            ArtifactFailurePolicy::RequireAll,
        )
        .unwrap();
        assert_eq!(
            request.generators()[0].output_kinds(),
            &[
                temporal_vision::ArtifactKind::Storyboard,
                temporal_vision::ArtifactKind::BeforeDuringAfter,
            ]
        );
        assert!(
            ArtifactGenerationRequest::new(
                range(),
                vec![],
                vec![],
                ArtifactFailurePolicy::RequireAll,
            )
            .is_err()
        );

        let mut invalid = match storyboard() {
            ArtifactGeneratorRequest::Storyboard(value) => value,
            _ => unreachable!(),
        };
        invalid.tile_limit = 2;
        assert!(
            ArtifactGenerationRequest::new(
                range(),
                vec![],
                vec![ArtifactGeneratorRequest::Storyboard(invalid)],
                ArtifactFailurePolicy::RequireAll,
            )
            .is_err()
        );
    }

    #[test]
    fn serde_rejects_unknown_fields_and_revalidates_nested_values() {
        let request = ArtifactGenerationRequest::new(
            range(),
            vec![],
            vec![storyboard()],
            ArtifactFailurePolicy::AllowPartial,
        )
        .unwrap();
        let mut value = serde_json::to_value(&request).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("natural_anchor".into(), serde_json::json!("latest"));
        assert!(serde_json::from_value::<ArtifactGenerationRequest>(value).is_err());

        let mut value = serde_json::to_value(&request).unwrap();
        value["generators"][0]["tile_limit"] = serde_json::json!(99);
        assert!(serde_json::from_value::<ArtifactGenerationRequest>(value).is_err());

        let mut explicit_zero = serde_json::to_value(request).unwrap();
        explicit_zero["generators"][0]["anchor"] = serde_json::json!(0);
        assert!(serde_json::from_value::<ArtifactGenerationRequest>(explicit_zero).is_err());
    }

    #[test]
    fn direct_generator_defaults_match_the_bundle_policy() {
        let value = serde_json::json!({
            "range": range(),
            "generators": [{ "generator": "storyboard" }]
        });
        let request: ArtifactGenerationRequest = serde_json::from_value(value).unwrap();
        let ArtifactGeneratorRequest::Storyboard(storyboard) = &request.generators()[0] else {
            panic!("expected the storyboard generator");
        };
        assert_eq!(
            storyboard.anchor,
            request.range().resolved_anchor.effective_time
        );
        assert_eq!(storyboard.tile_limit, DEFAULT_ARTIFACT_TILE_LIMIT);
        assert_eq!(storyboard.noise_floor, DEFAULT_ARTIFACT_NOISE_FLOOR);
        assert_eq!(storyboard.normalization.scale, AnalysisScale::FitLimits);
        assert_eq!(storyboard.output.max_width(), DEFAULT_STORYBOARD_MAX_WIDTH);
        assert_eq!(
            storyboard.output.max_height(),
            DEFAULT_STORYBOARD_MAX_HEIGHT
        );
        assert_eq!(
            storyboard.output.max_encoded_bytes(),
            DEFAULT_STORYBOARD_MAX_BYTES
        );
        assert!(request.markers().is_empty());
        assert_eq!(
            request.failure_policy(),
            ArtifactFailurePolicy::AllowPartial
        );
    }

    #[test]
    fn wire_analysis_sampling_defaults_to_uniform_bounded() {
        let difference: DifferenceMapRequest = serde_json::from_value(serde_json::json!({}))
            .expect("difference-map defaults should deserialize");
        let motion: MotionHistoryRequest = serde_json::from_value(serde_json::json!({}))
            .expect("motion-history defaults should deserialize");
        assert_eq!(difference.sampling, ArtifactSampling::UniformBounded);
        assert_eq!(motion.sampling, ArtifactSampling::UniformBounded);
    }

    #[test]
    fn duplicate_and_out_of_range_markers_are_rejected() {
        let id = ArtifactMarkerId::Caller(NonEmptyText::new("same").unwrap());
        let marker = ArtifactMarker::new(
            id,
            SessionTime::from_nanos(2),
            NonEmptyText::new("event").unwrap(),
            NonEmptyText::new("marker").unwrap(),
        );
        assert!(
            ArtifactGenerationRequest::new(
                range(),
                vec![marker.clone(), marker],
                vec![storyboard()],
                ArtifactFailurePolicy::RequireAll,
            )
            .is_err()
        );
        let marker = ArtifactMarker::new(
            ArtifactMarkerId::Caller(NonEmptyText::new("late").unwrap()),
            SessionTime::from_nanos(9),
            NonEmptyText::new("event").unwrap(),
            NonEmptyText::new("marker").unwrap(),
        );
        assert!(
            ArtifactGenerationRequest::new(
                range(),
                vec![marker],
                vec![storyboard()],
                ArtifactFailurePolicy::RequireAll,
            )
            .is_err()
        );
    }
}
