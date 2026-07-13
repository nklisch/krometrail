//! Browser-agnostic temporal visual analysis contracts.

macro_rules! stable_registry {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($variant:ident => $wire:literal),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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

mod error;
mod frame;
mod geometry;
mod measure;
mod normalize;
mod provenance;
mod sequence;

pub use error::{ErrorCode, Result, VisionError};
pub use frame::{BorrowedFrame, Frame, OwnedFrame, PixelDimensions, PixelFormat, Timestamp};
pub use geometry::{BinaryMask, FrameRegion, PixelRect};
pub use measure::{
    ChangedPixelProportion, ComparisonOutcome, FrameComparison, MeasurementParameters,
    MeasurementVector, measure_adjacent, measure_pair,
};
pub use normalize::{
    IntegerScale, NormalizationParameters, NormalizedFrame, NormalizedSequence, ProcessingLimits,
    Rgb8, normalize_sequence,
};
pub use provenance::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, EvidenceClass, FiniteNumber,
    NormalizationKind, NormalizationStep, OutputHash, ParameterValue, Parameters,
};
pub use sequence::{
    BorrowedFrameSequence, DeclaredGap, FrameSequence, Marker, OwnedFrameSequence, TimeRange,
};
