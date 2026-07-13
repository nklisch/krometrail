//! Small, bounded image-header readers for capture ingestion.
//!
//! The recorder deliberately keeps encoded bytes opaque. These readers obtain only dimensions so
//! the recording contract can describe a frame without adding a pixel decoder or image library.

use krometrail_core::{ImageFormat, PixelDimensions, Result};

const HEADER_SCAN_LIMIT: usize = 64 * 1024;

pub(crate) fn dimensions(format: ImageFormat, bytes: &[u8]) -> Result<PixelDimensions> {
    match format {
        ImageFormat::Png => png_dimensions(bytes),
        ImageFormat::Jpeg => jpeg_dimensions(bytes),
    }
}

fn png_dimensions(bytes: &[u8]) -> Result<PixelDimensions> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.len() < 24 || &bytes[..8] != SIGNATURE {
        return Err(invalid_header("invalid PNG signature or missing IHDR"));
    }
    if u32::from_be_bytes(bytes[8..12].try_into().expect("checked PNG length")) != 13
        || &bytes[12..16] != b"IHDR"
    {
        return Err(invalid_header("PNG does not begin with a fixed IHDR"));
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("checked PNG width"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("checked PNG height"));
    PixelDimensions::new(width, height)
        .map_err(|_| invalid_header("PNG IHDR dimensions must be non-zero"))
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<PixelDimensions> {
    if bytes.len() < 2 || bytes[..2] != [0xff, 0xd8] {
        return Err(invalid_header("invalid JPEG SOI"));
    }

    let limit = bytes.len().min(HEADER_SCAN_LIMIT);
    let mut cursor = 2;
    while cursor < limit {
        if bytes[cursor] != 0xff {
            return Err(invalid_header("malformed JPEG marker prefix"));
        }
        while cursor < limit && bytes[cursor] == 0xff {
            cursor += 1;
        }
        if cursor >= limit {
            break;
        }
        let marker = bytes[cursor];
        cursor += 1;

        // Fill bytes and restart markers do not carry a segment length. A bare EOI before a SOF
        // is an incomplete header, not a valid dimension source.
        if marker == 0xd9 {
            break;
        }
        if marker == 0x01 || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        if cursor.checked_add(2).is_none() || cursor + 2 > limit {
            break;
        }
        let segment_length = u16::from_be_bytes(
            bytes[cursor..cursor + 2]
                .try_into()
                .expect("checked JPEG segment length"),
        ) as usize;
        if segment_length < 2 {
            return Err(invalid_header("JPEG segment length is too small"));
        }
        let segment_end = cursor
            .checked_add(segment_length)
            .ok_or_else(|| invalid_header("JPEG segment length overflow"))?;
        if segment_end > limit {
            break;
        }

        if is_start_of_frame(marker) {
            if segment_length < 7 {
                return Err(invalid_header("JPEG SOF segment is too small"));
            }
            let height = u16::from_be_bytes(
                bytes[cursor + 3..cursor + 5]
                    .try_into()
                    .expect("checked JPEG height"),
            ) as u32;
            let width = u16::from_be_bytes(
                bytes[cursor + 5..cursor + 7]
                    .try_into()
                    .expect("checked JPEG width"),
            ) as u32;
            return PixelDimensions::new(width, height)
                .map_err(|_| invalid_header("JPEG SOF dimensions must be non-zero"));
        }
        cursor = segment_end;
    }
    Err(invalid_header("no JPEG SOF marker within 64 KiB"))
}

const fn is_start_of_frame(marker: u8) -> bool {
    matches!(
        marker,
        0xc0 | 0xc1 | 0xc2 | 0xc3 | 0xc5 | 0xc6 | 0xc7 | 0xc9 | 0xca | 0xcb | 0xcd | 0xce | 0xcf
    )
}

fn invalid_header(detail: &'static str) -> krometrail_core::KrometrailError {
    krometrail_core::KrometrailError::new(
        krometrail_core::ErrorCode::CaptureRejected,
        krometrail_core::NonEmptyText::new(detail).expect("static header error is non-empty"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_png_ihdr_without_decoding_pixels() {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&13_u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&640_u32.to_be_bytes());
        png.extend_from_slice(&480_u32.to_be_bytes());
        assert_eq!(dimensions(ImageFormat::Png, &png).unwrap().width(), 640);
        assert!(dimensions(ImageFormat::Png, b"\x89PNG\r\n\x1a\n").is_err());
        assert!(dimensions(ImageFormat::Png, &png[..23]).is_err());
    }

    #[test]
    fn walks_checked_jpeg_segments_to_sof() {
        let mut jpeg = vec![0xff, 0xd8, 0xff, 0xe0, 0, 4, 1, 2];
        jpeg.extend_from_slice(&[0xff, 0xc0, 0, 8, 8, 1, 0x2c, 2, 0x80, 1, 1]);
        let size = dimensions(ImageFormat::Jpeg, &jpeg).unwrap();
        assert_eq!((size.width(), size.height()), (640, 300));
        assert!(dimensions(ImageFormat::Jpeg, &[0xff, 0xd8, 0xff, 0xc0, 0, 1]).is_err());
        assert!(dimensions(ImageFormat::Jpeg, &[0xff, 0xd8, 0xff, 0xd9]).is_err());
    }

    #[test]
    fn bounds_jpeg_header_scan() {
        let mut jpeg = vec![0xff, 0xd8];
        jpeg.extend(std::iter::repeat_n(0xff, HEADER_SCAN_LIMIT));
        assert!(dimensions(ImageFormat::Jpeg, &jpeg).is_err());
    }
}
