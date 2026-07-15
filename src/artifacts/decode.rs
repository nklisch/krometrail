use std::io::Cursor;

use image::{
    ColorType, DynamicImage, ImageDecoder, ImageFormat as DecoderFormat, ImageReader, Limits,
};
use krometrail_core::{EncodedFrame, ErrorCode, KrometrailError, NonEmptyText, Result};
use temporal_vision::{OwnedFrame, PixelDimensions, PixelFormat, Timestamp};

pub(crate) const DECODER_PROFILE: &str =
    "image-0.25.9-forced-jpeg-png-rgba8-straight-no-orientation-v1";
pub(crate) const DECODER_ALGORITHM_VERSION: &str = "krometrail-decode-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecodeLimits {
    pub max_dimension: u32,
    pub max_pixels: usize,
    pub max_rgba_bytes: usize,
    pub max_decoder_alloc: u64,
}

impl DecodeLimits {
    pub(crate) const fn new(
        max_dimension: u32,
        max_pixels: usize,
        max_rgba_bytes: usize,
        max_decoder_alloc: u64,
    ) -> Self {
        Self {
            max_dimension,
            max_pixels,
            max_rgba_bytes,
            max_decoder_alloc,
        }
    }
}

pub(crate) fn decode_frame(
    frame: &EncodedFrame,
    limits: DecodeLimits,
) -> Result<OwnedFrame<krometrail_core::FrameId>> {
    #[cfg(test)]
    super::perf_counters::record_decode();
    let metadata = frame.metadata();
    let width = metadata.image().width();
    let height = metadata.image().height();
    let (pixel_count, rgba_len) = preflight(width, height, limits)?;
    let format = match metadata.format() {
        krometrail_core::ImageFormat::Jpeg => DecoderFormat::Jpeg,
        krometrail_core::ImageFormat::Png => DecoderFormat::Png,
    };
    let mut reader = ImageReader::with_format(Cursor::new(frame.bytes()), format);
    let mut decoder_limits = Limits::default();
    decoder_limits.max_image_width = Some(limits.max_dimension);
    decoder_limits.max_image_height = Some(limits.max_dimension);
    decoder_limits.max_alloc = Some(limits.max_decoder_alloc);
    reader.limits(decoder_limits);
    let decoder = reader
        .into_decoder()
        .map_err(|_| decode_error("encoded frame header is invalid for its declared format"))?;
    let decoded_dimensions = decoder.dimensions();
    if decoded_dimensions != (width, height) {
        return Err(decode_error(
            "decoded image dimensions contradict persisted frame metadata",
        ));
    }
    let color = decoder.color_type();
    if !matches!(
        color,
        ColorType::L8 | ColorType::La8 | ColorType::Rgb8 | ColorType::Rgba8
    ) {
        return Err(decode_error(
            "encoded frame precision or color representation is unsupported",
        ));
    }
    if metadata.format() == krometrail_core::ImageFormat::Jpeg && color.has_alpha() {
        return Err(decode_error("JPEG screencast input must be opaque"));
    }
    // The explicit preflight remains authoritative even when a codec's max_alloc support is
    // best-effort. No orientation, profile transform, or format sniffing is requested here.
    let decoded = DynamicImage::from_decoder(decoder)
        .map_err(|_| decode_error("encoded frame payload is malformed or truncated"))?;
    let rgba = expand_straight_rgba(decoded, pixel_count, rgba_len)?;
    temporal_vision::Frame::new(
        metadata.id(),
        Timestamp::from_nanos(metadata.session_time().as_nanos()),
        PixelDimensions::new(width, height).map_err(vision_error)?,
        PixelFormat::Rgba8SrgbStraight,
        rgba.into_boxed_slice(),
    )
    .map_err(vision_error)
}

fn preflight(width: u32, height: u32, limits: DecodeLimits) -> Result<(usize, usize)> {
    if width > limits.max_dimension || height > limits.max_dimension {
        return Err(limit_error(
            "encoded frame dimensions exceed the configured limit",
        ));
    }
    let pixels_u64 = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or_else(|| limit_error("encoded frame dimensions overflow the pixel count"))?;
    let pixels = usize::try_from(pixels_u64)
        .map_err(|_| limit_error("encoded frame pixel count exceeds this platform"))?;
    if pixels > limits.max_pixels {
        return Err(limit_error(
            "encoded frame pixel count exceeds the configured limit",
        ));
    }
    let rgba_len = pixels
        .checked_mul(4)
        .ok_or_else(|| limit_error("decoded RGBA byte length overflows"))?;
    if rgba_len > limits.max_rgba_bytes
        || u64::try_from(rgba_len).unwrap_or(u64::MAX) > limits.max_decoder_alloc
    {
        return Err(limit_error(
            "decoded RGBA bytes exceed the configured limit",
        ));
    }
    Ok((pixels, rgba_len))
}

fn expand_straight_rgba(
    decoded: DynamicImage,
    pixel_count: usize,
    rgba_len: usize,
) -> Result<Vec<u8>> {
    let mut rgba = Vec::with_capacity(rgba_len);
    match decoded {
        DynamicImage::ImageLuma8(image) => {
            for value in image.into_raw() {
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        DynamicImage::ImageLumaA8(image) => {
            for pair in image.into_raw().chunks_exact(2) {
                rgba.extend_from_slice(&[pair[0], pair[0], pair[0], pair[1]]);
            }
        }
        DynamicImage::ImageRgb8(image) => {
            for triple in image.into_raw().chunks_exact(3) {
                rgba.extend_from_slice(&[triple[0], triple[1], triple[2], 255]);
            }
        }
        DynamicImage::ImageRgba8(image) => rgba = image.into_raw(),
        _ => return Err(decode_error("decoder returned unsupported image precision")),
    }
    if rgba.len() != rgba_len || rgba.len() / 4 != pixel_count {
        return Err(decode_error(
            "decoder returned an unexpected pixel payload length",
        ));
    }
    Ok(rgba)
}

fn vision_error(error: temporal_vision::VisionError) -> KrometrailError {
    decode_error(format!("decoded frame is invalid: {}", error.message))
}

fn decode_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new(message).expect("decoder errors are non-empty"),
    )
}

fn limit_error(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("decoder limit errors are non-empty"),
    )
}
