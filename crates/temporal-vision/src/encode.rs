use std::io::{self, Write};

use png::{BitDepth, ColorType, Compression, Encoder, FilterType};
use sha2::{Digest, Sha256};

use crate::{ErrorCode, OutputHash, PixelDimensions, Result, VisionError};

pub(crate) fn encode_png(
    dimensions: PixelDimensions,
    rgb8: &[u8],
    max_encoded_bytes: usize,
) -> Result<(Vec<u8>, OutputHash)> {
    let expected = dimensions
        .pixel_count()?
        .checked_mul(3)
        .ok_or_else(encoding_limit_error)?;
    if rgb8.len() != expected {
        return Err(VisionError::new(
            ErrorCode::InvalidParameter,
            "RGB8 canvas length does not match its dimensions",
        ));
    }

    let mut output = BoundedWriter::new(max_encoded_bytes);
    {
        let mut encoder = Encoder::new(&mut output, dimensions.width(), dimensions.height());
        encoder.set_color(ColorType::Rgb);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_compression(Compression::Best);
        encoder.set_filter(FilterType::NoFilter);
        let mut writer = encoder.write_header().map_err(|_| encoding_limit_error())?;
        writer
            .write_image_data(rgb8)
            .map_err(|_| encoding_limit_error())?;
        writer.finish().map_err(|_| encoding_limit_error())?;
    }
    let bytes = output.into_inner();
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    Ok((bytes, OutputHash::from_bytes(digest)))
}

struct BoundedWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.bytes
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let next_len = self
            .bytes
            .len()
            .checked_add(buffer.len())
            .ok_or_else(limit_io_error)?;
        if next_len > self.limit {
            return Err(limit_io_error());
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn limit_io_error() -> io::Error {
    io::Error::other("encoded image exceeds configured byte limit")
}

fn encoding_limit_error() -> VisionError {
    VisionError::new(
        ErrorCode::ResourceLimitExceeded,
        "encoded PNG exceeds configured output limits",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_is_deterministic_and_bounded() {
        let dimensions = PixelDimensions::new(1, 1).unwrap();
        let first = encode_png(dimensions, &[1, 2, 3], 1_024).unwrap();
        let second = encode_png(dimensions, &[1, 2, 3], 1_024).unwrap();
        assert_eq!(first, second);
        assert_eq!(&first.0[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(
            encode_png(dimensions, &[1, 2, 3], 8).unwrap_err().code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}
