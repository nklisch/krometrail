//! Browser-agnostic temporal visual analysis contracts.

macro_rules! stable_registry {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($variant:ident => $wire:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, schemars::JsonSchema)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $wire),+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let value = <String as serde::Deserialize>::deserialize(deserializer)?;
                match value.as_str() {
                    $($wire => Ok(Self::$variant),)+
                    _ => Err(serde::de::Error::unknown_variant(
                        &value,
                        &[$($wire),+],
                    )),
                }
            }
        }
    };
}

mod artifact;
mod difference_map;
mod encode;
mod error;
mod filmstrip;
mod frame;
mod geometry;
mod measure;
mod motion_history;
mod normalize;
mod pair_analysis;
mod provenance;
mod render;
mod select;
mod sequence;

pub use artifact::{EncodedImage, GeneratedArtifact};
pub use difference_map::{
    DifferenceMapArtifact, DifferenceMapLimits, DifferenceMapParameters, FrequencyMode,
    TimePalette, render_difference_map, render_difference_map_with_context,
};
pub use error::{ErrorCode, Result, VisionError};
pub use filmstrip::{
    FilmstripTileLimit, FilmstripTilePlan, PaddingInsets, RationalScale, RegionCoordinateSpace,
    RegionDefinition, RegionFilmstripArtifact, RegionFilmstripLabels, RegionFilmstripParameters,
    RegionFilmstripPlan, RegionFilmstripRenderLimits, SignedPixelRect, ViewportMapping,
    generate_region_filmstrip, plan_region_filmstrip,
};
pub use frame::{BorrowedFrame, Frame, OwnedFrame, PixelDimensions, PixelFormat, Timestamp};
pub use geometry::{BinaryMask, FrameRegion, PixelRect};
pub use measure::{
    ChangedPixelProportion, ComparisonOutcome, FrameComparison, MeasurementParameters,
    MeasurementVector, measure_adjacent, measure_pair,
};
pub use motion_history::{
    MotionDecay, MotionHistoryArtifact, MotionHistoryParameters, MotionHistoryPlan,
    build_motion_history_plan, generate_motion_history, generate_motion_history_with_context,
};
pub use normalize::{
    IntegerScale, NormalizationParameters, NormalizedFrame, NormalizedSequence, ProcessingLimits,
    Rgb8, normalize_sequence,
};
pub use pair_analysis::{
    PairAnalysisContext, build_pair_analysis_context_for_consumers, pair_analysis_memory_bytes,
};
pub use provenance::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, EvidenceClass, FiniteNumber,
    GeneratorDescriptor, NormalizationKind, NormalizationStep, OutputHash, ParameterValue,
    Parameters, generator_descriptor,
};
pub use render::{
    ArtifactLabels, RenderLimits, StoryboardArtifacts, StoryboardParameters, generate_storyboard,
    generate_storyboard_with_context,
};
pub use select::{
    OmittedAnchor, SelectedFrame, SelectionReason, StoryboardSelection, StoryboardTileLimit,
    StoryboardVisualSummary, VisualChangeMoment, select_storyboard_frames,
};
pub use sequence::{
    BorrowedFrameSequence, DeclaredGap, FrameSequence, Marker, OwnedFrameSequence, TimeRange,
};
