use crate::{ArtifactManifest, PixelDimensions};

/// One deterministic encoded image held entirely in memory.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedImage {
    dimensions: PixelDimensions,
    bytes: Box<[u8]>,
}

impl EncodedImage {
    pub(crate) fn new(dimensions: PixelDimensions, bytes: Vec<u8>) -> Self {
        Self {
            dimensions,
            bytes: bytes.into_boxed_slice(),
        }
    }

    pub const fn media_type(&self) -> &'static str {
        "image/png"
    }

    pub const fn dimensions(&self) -> PixelDimensions {
        self.dimensions
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Encoded artifact bytes and the provenance that describes those exact bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedArtifact<ArtifactId, FrameId, MarkerId, GapId> {
    image: EncodedImage,
    manifest: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId>,
}

impl<A, F, M, G> GeneratedArtifact<A, F, M, G> {
    pub(crate) fn new(image: EncodedImage, manifest: ArtifactManifest<A, F, M, G>) -> Self {
        Self { image, manifest }
    }

    pub const fn image(&self) -> &EncodedImage {
        &self.image
    }

    pub const fn manifest(&self) -> &ArtifactManifest<A, F, M, G> {
        &self.manifest
    }
}
