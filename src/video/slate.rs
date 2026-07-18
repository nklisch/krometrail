use std::{io::Cursor, sync::Arc};

use image::{ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};
use krometrail_core::{
    ErrorCode, KrometrailError, NonEmptyText, PixelDimensions, Result, SessionRange,
};

const LABEL: &str = "CAPTURE GAP";
pub(crate) const GAP_SLATE_MIN_WIDTH: u32 = 264;
pub(crate) const GAP_SLATE_MIN_HEIGHT: u32 = 18;

pub(crate) fn render_gap_slate(
    canvas: PixelDimensions,
    source_range: SessionRange,
) -> Result<Arc<[u8]>> {
    if canvas.width() < GAP_SLATE_MIN_WIDTH || canvas.height() < GAP_SLATE_MIN_HEIGHT {
        return Err(KrometrailError::new(
            ErrorCode::ResourceLimitExceeded,
            NonEmptyText::new(
                "capture-gap video canvas is too small for its label and source-time interval",
            )
            .expect("static gap-slate limit error is non-empty"),
        ));
    }
    let mut image = RgbaImage::new(canvas.width(), canvas.height());
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let stripe = ((x / 12) + (y / 12)) % 2 == 0;
        *pixel = if stripe {
            Rgba([38, 18, 54, 255])
        } else {
            Rgba([97, 39, 112, 255])
        };
    }
    let subtitle = format!(
        "{}-{} NS",
        source_range.start().as_nanos(),
        source_range.end().as_nanos()
    );
    draw_centered(
        &mut image,
        LABEL,
        canvas.height().saturating_div(2).saturating_sub(9),
    );
    draw_centered(
        &mut image,
        &subtitle,
        canvas.height().saturating_div(2).saturating_add(2),
    );

    let mut encoded = Cursor::new(Vec::new());
    PngEncoder::new(&mut encoded)
        .write_image(
            image.as_raw(),
            canvas.width(),
            canvas.height(),
            image::ExtendedColorType::Rgba8,
        )
        .map_err(|_| slate_error())?;
    Ok(Arc::from(encoded.into_inner()))
}

fn draw_centered(image: &mut RgbaImage, text: &str, y: u32) {
    let width = text.chars().count() as u32 * 6;
    if width > image.width() || y.saturating_add(7) > image.height() {
        return;
    }
    let mut x = (image.width() - width) / 2;
    for character in text.chars() {
        draw_glyph(image, x, y, glyph(character));
        x += 6;
    }
}

fn draw_glyph(image: &mut RgbaImage, x: u32, y: u32, rows: [u8; 7]) {
    for (row, bits) in rows.into_iter().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) != 0 {
                image.put_pixel(x + column, y + row as u32, Rgba([255, 245, 138, 255]));
            }
        }
    }
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0e],
        'N' => [0x11, 0x19, 0x19, 0x15, 0x13, 0x13, 0x11],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x04, 0x04, 0x04, 0x04, 0x0e],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        '-' => [0, 0, 0, 0x1f, 0, 0, 0],
        _ => [0; 7],
    }
}

fn slate_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ArtifactGenerationFailed,
        NonEmptyText::new("could not encode deterministic capture-gap slate")
            .expect("static slate error is non-empty"),
    )
}
