//! Test-only observations of retained benchmark images.
//!
//! This is deliberately a small, fixture-owned pixel predicate layer. It does not run in the
//! product, add browser-visible labels, or create replacement evidence. A predicate that cannot
//! establish its geometry or image input returns `Unknown` rather than guessing.

use image::{DynamicImage, GenericImageView, ImageReader, Rgba};
use temporal_evaluation::{CaseDefinition, CaseFamily, Rect, VIEWPORT_HEIGHT, VIEWPORT_WIDTH};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameGeometry {
    pub width: u32,
    pub height: u32,
    pub device_scale_factor_milli: u16,
}

impl FrameGeometry {
    pub const CANONICAL: Self = Self {
        width: VIEWPORT_WIDTH,
        height: VIEWPORT_HEIGHT,
        device_scale_factor_milli: 1_000,
    };
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnknownReason {
    Decode,
    ViewportMismatch,
    ScaleMismatch,
    GeometryMismatch,
    PredicateUncertain,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FixtureStateObservation {
    Baseline,
    Changed,
    Final,
    Stable,
    Unknown(UnknownReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SubjectObservation {
    None,
    Position { x: u32, y: u32 },
    Present(bool),
    Geometry { width: u32, height: u32 },
    Unknown(UnknownReason),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TemporalFixtureObservation {
    pub case_id: String,
    pub state: FixtureStateObservation,
    pub subject: SubjectObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FixtureSequenceObservation {
    pub frames: Vec<TemporalFixtureObservation>,
    pub state_order: Vec<FixtureStateObservation>,
    pub movement: MovementSequenceObservation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MovementSequenceObservation {
    NotApplicable,
    Monotonic,
    Reversal,
    Teleport,
    Stable,
    Unknown(UnknownReason),
}

/// Decode one retained source payload and apply the exact canonical viewport/scale contract.
/// `bytes` must be the encoded payload returned by the production `FrameSource`.
pub fn observe_fixture_frame(
    bytes: &[u8],
    definition: &CaseDefinition,
) -> TemporalFixtureObservation {
    observe_fixture_frame_with_geometry(bytes, definition, FrameGeometry::CANONICAL)
}

pub fn observe_fixture_frame_with_geometry(
    bytes: &[u8],
    definition: &CaseDefinition,
    geometry: FrameGeometry,
) -> TemporalFixtureObservation {
    observe_fixture_frame_with_expected_geometry(
        bytes,
        definition,
        geometry,
        FrameGeometry::CANONICAL,
    )
}

/// Observe a frame against a lane's expected CSS viewport/scale. High-DPI screencasts may carry
/// physical pixels at a larger image size; they are reduced to the declared CSS viewport for the
/// fixture predicate, while the original encoded frame and observed metadata remain authoritative
/// in the production store and manifest.
pub fn observe_fixture_frame_with_expected_geometry(
    bytes: &[u8],
    definition: &CaseDefinition,
    geometry: FrameGeometry,
    expected: FrameGeometry,
) -> TemporalFixtureObservation {
    let unknown = |reason: UnknownReason| TemporalFixtureObservation {
        case_id: definition.case_id.clone(),
        state: FixtureStateObservation::Unknown(reason.clone()),
        subject: SubjectObservation::Unknown(reason),
    };
    if geometry.width == 0
        || geometry.height == 0
        || expected.width == 0
        || expected.height == 0
        || geometry.device_scale_factor_milli == 0
        || expected.device_scale_factor_milli == 0
    {
        return unknown(UnknownReason::ViewportMismatch);
    }
    if geometry.device_scale_factor_milli != expected.device_scale_factor_milli {
        return unknown(UnknownReason::ScaleMismatch);
    }
    if !valid_geometry(definition.affected_region) {
        return unknown(UnknownReason::GeometryMismatch);
    }
    let image = match ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|reader| reader.decode().ok())
    {
        Some(image) => image,
        None => return unknown(UnknownReason::Decode),
    };
    if image.dimensions() != (geometry.width, geometry.height) {
        return unknown(UnknownReason::ViewportMismatch);
    }
    let image = if image.dimensions() == (expected.width, expected.height) {
        image
    } else {
        image.resize_exact(
            expected.width,
            expected.height,
            image::imageops::FilterType::Nearest,
        )
    };
    classify(&image, definition)
}

/// Classify an ordered set of retained frames. The first and last observed stable states are
/// named baseline/final only when the pixels support that ordering; missing or uncertain frames
/// remain unknown. This is intentionally not a timing model.
pub fn observe_fixture_sequence(
    frames: &[&[u8]],
    definition: &CaseDefinition,
) -> FixtureSequenceObservation {
    observe_fixture_sequence_with_expected_geometry(
        frames,
        definition,
        FrameGeometry::CANONICAL,
        FrameGeometry::CANONICAL,
    )
}

pub fn observe_fixture_sequence_with_expected_geometry(
    frames: &[&[u8]],
    definition: &CaseDefinition,
    geometry: FrameGeometry,
    expected: FrameGeometry,
) -> FixtureSequenceObservation {
    let raw = frames
        .iter()
        .map(|bytes| {
            observe_fixture_frame_with_expected_geometry(bytes, definition, geometry, expected)
        })
        .collect::<Vec<_>>();
    let mut observations = raw.clone();
    let stable_indices = raw
        .iter()
        .enumerate()
        .filter_map(|(index, observation)| {
            matches!(
                observation.state,
                FixtureStateObservation::Baseline
                    | FixtureStateObservation::Stable
                    | FixtureStateObservation::Final
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    if let (Some(first), Some(last)) = (stable_indices.first(), stable_indices.last()) {
        observations[*first].state = FixtureStateObservation::Baseline;
        observations[*last].state = FixtureStateObservation::Final;
        for index in stable_indices
            .iter()
            .copied()
            .filter(|index| index != first && index != last)
        {
            observations[index].state = FixtureStateObservation::Final;
        }
    }

    let state_order = observations
        .iter()
        .map(|observation| observation.state.clone())
        .fold(Vec::new(), |mut states, state| {
            if !matches!(state, FixtureStateObservation::Unknown(_))
                && states.last() != Some(&state)
            {
                states.push(state);
            }
            states
        });
    let movement = movement_sequence(&observations, definition.family);
    FixtureSequenceObservation {
        frames: observations,
        state_order,
        movement,
    }
}

fn classify(image: &DynamicImage, definition: &CaseDefinition) -> TemporalFixtureObservation {
    let (state, subject) = match definition.case_id.as_str() {
        "movement-reversal/basic" | "stable/smooth-panel" => {
            match find_panel_left(image, definition.affected_region) {
                Some(x) => {
                    let baseline = definition.affected_region.x;
                    let final_x = baseline + 240;
                    let state = if x == baseline {
                        FixtureStateObservation::Baseline
                    } else if x == final_x {
                        FixtureStateObservation::Final
                    } else {
                        FixtureStateObservation::Changed
                    };
                    (
                        state,
                        SubjectObservation::Position {
                            x,
                            y: definition.affected_region.y,
                        },
                    )
                }
                None => uncertain(),
            }
        }
        "flicker/visibility" => classify_sample(
            sample_center(image, definition.affected_region),
            &[rgb(0xfa, 0xfb, 0xfd)],
            &[rgb(0xff, 0xff, 0xff)],
            SubjectObservation::Present(true),
        ),
        "flicker/color" => classify_sample(
            sample_center(image, definition.affected_region),
            &[rgb(0xfa, 0xfb, 0xfd)],
            &[rgb(0xff, 0xf4, 0xe5)],
            SubjectObservation::Present(true),
        ),
        "flicker/text" => {
            let count = dark_pixels(image, definition.affected_region);
            match count {
                Some(count) if count <= 45 => (
                    FixtureStateObservation::Stable,
                    SubjectObservation::Present(true),
                ),
                Some(count) if count >= 46 => (
                    FixtureStateObservation::Changed,
                    SubjectObservation::Present(true),
                ),
                Some(_) | None => uncertain(),
            }
        }
        "layout/width" => classify_sample(
            sample(
                image,
                definition.affected_region.x + definition.affected_region.width - 39,
                definition.affected_region.y + 59,
            ),
            &[rgb(0xf5, 0xf8, 0xfc)],
            &[rgb(0xff, 0xff, 0xff)],
            SubjectObservation::Geometry {
                width: 640,
                height: 160,
            },
        ),
        "layout/content-shift" => classify_sample(
            sample(
                image,
                definition.affected_region.x + 10,
                definition.affected_region.y + 37,
            ),
            &[rgb(0xf5, 0xf8, 0xfc)],
            &[rgb(0xe8, 0xee, 0xf6)],
            SubjectObservation::Geometry {
                width: 640,
                height: 202,
            },
        ),
        "layout/scroll-position" => {
            match horizontal_line_count(image, definition.affected_region) {
                Some(count) if count >= 3 => (
                    FixtureStateObservation::Stable,
                    SubjectObservation::Geometry {
                        width: 320,
                        height: 120,
                    },
                ),
                Some(count) if count <= 2 => (
                    FixtureStateObservation::Changed,
                    SubjectObservation::Geometry {
                        width: 320,
                        height: 120,
                    },
                ),
                Some(_) | None => uncertain(),
            }
        }
        "dom-opaque/path-reversal" | "dom-opaque/teleport" | "dom-opaque/sprite" => {
            match canvas_subject(image, definition.affected_region) {
                Some((x, y, red)) => {
                    let state = match definition.case_id.as_str() {
                        "dom-opaque/sprite" if red => FixtureStateObservation::Changed,
                        "dom-opaque/sprite" => FixtureStateObservation::Stable,
                        "dom-opaque/path-reversal" if x == 80 => FixtureStateObservation::Baseline,
                        "dom-opaque/path-reversal" if x >= 304 => FixtureStateObservation::Final,
                        "dom-opaque/path-reversal" => FixtureStateObservation::Changed,
                        "dom-opaque/teleport" if x == 80 => FixtureStateObservation::Baseline,
                        "dom-opaque/teleport" if x >= 304 => FixtureStateObservation::Final,
                        "dom-opaque/teleport" => FixtureStateObservation::Changed,
                        _ => FixtureStateObservation::Unknown(UnknownReason::PredicateUncertain),
                    };
                    (state, SubjectObservation::Position { x, y })
                }
                None if definition.case_id == "dom-opaque/teleport" => (
                    FixtureStateObservation::Changed,
                    SubjectObservation::Present(false),
                ),
                None => uncertain(),
            }
        }
        "stable/loading-indicator" => match blue_pixels(image, definition.affected_region) {
            Some(count) if count > 8 => (
                FixtureStateObservation::Changed,
                SubjectObservation::Present(true),
            ),
            Some(_) => (
                FixtureStateObservation::Stable,
                SubjectObservation::Present(false),
            ),
            None => uncertain(),
        },
        "stable/caret" => {
            let caret_region = Rect {
                x: definition.affected_region.x + 45,
                y: definition.affected_region.y + 4,
                width: 20,
                height: definition.affected_region.height.saturating_sub(8),
            };
            match dark_columns(image, caret_region) {
                Some(count) if count >= 1 => (
                    FixtureStateObservation::Stable,
                    SubjectObservation::Present(true),
                ),
                Some(0) => (
                    FixtureStateObservation::Changed,
                    SubjectObservation::Present(false),
                ),
                Some(_) | None => uncertain(),
            }
        }
        _ => uncertain(),
    };
    TemporalFixtureObservation {
        case_id: definition.case_id.clone(),
        state,
        subject,
    }
}

fn classify_sample(
    actual: Option<Rgba<u8>>,
    stable: &[Rgba<u8>],
    changed: &[Rgba<u8>],
    subject: SubjectObservation,
) -> (FixtureStateObservation, SubjectObservation) {
    match actual {
        Some(pixel) if stable.iter().any(|candidate| close(pixel, *candidate)) => {
            (FixtureStateObservation::Stable, subject)
        }
        Some(pixel) if changed.iter().any(|candidate| close(pixel, *candidate)) => {
            (FixtureStateObservation::Changed, subject)
        }
        Some(_) => uncertain(),
        None => uncertain(),
    }
}

fn movement_sequence(
    observations: &[TemporalFixtureObservation],
    family: CaseFamily,
) -> MovementSequenceObservation {
    if !matches!(
        family,
        CaseFamily::MovementReversal | CaseFamily::DomOpaqueMotion | CaseFamily::StableControl
    ) {
        return MovementSequenceObservation::NotApplicable;
    }
    let positions = observations
        .iter()
        .filter_map(|observation| match observation.subject {
            SubjectObservation::Position { x, .. } => Some(x),
            _ => None,
        })
        .collect::<Vec<_>>();
    if positions.len() < 2 {
        return MovementSequenceObservation::Unknown(UnknownReason::PredicateUncertain);
    }
    let mut direction_changes = 0;
    let mut previous_sign = 0_i8;
    let mut large_jumps = false;
    for pair in positions.windows(2) {
        let delta = pair[1] as i64 - pair[0] as i64;
        if delta.unsigned_abs() > 100 {
            large_jumps = true;
        }
        let sign = delta.signum() as i8;
        if sign != 0 {
            if previous_sign != 0 && sign != previous_sign {
                direction_changes += 1;
            }
            previous_sign = sign;
        }
    }
    if direction_changes > 0 {
        MovementSequenceObservation::Reversal
    } else if large_jumps && matches!(family, CaseFamily::DomOpaqueMotion) {
        MovementSequenceObservation::Teleport
    } else if positions.windows(2).all(|pair| pair[0] <= pair[1]) {
        if positions.iter().all(|position| *position == positions[0]) {
            MovementSequenceObservation::Stable
        } else {
            MovementSequenceObservation::Monotonic
        }
    } else {
        MovementSequenceObservation::Unknown(UnknownReason::PredicateUncertain)
    }
}

fn find_panel_left(image: &DynamicImage, rect: Rect) -> Option<u32> {
    // The border's vertical run is a geometry-synchronized predicate, not a DOM lookup.
    (rect.x..rect.x + rect.width).find(|x| {
        (rect.y..rect.y + rect.height)
            .filter(|y| image.get_pixel(*x, *y).0[0] == 0xb7)
            .count()
            >= 35
    })
}

fn canvas_subject(image: &DynamicImage, rect: Rect) -> Option<(u32, u32, bool)> {
    let mut blue = Vec::new();
    let mut red = Vec::new();
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let pixel = image.get_pixel(x, y);
            if close(pixel, rgb(0x46, 0x79, 0xb7)) {
                blue.push((x, y));
            }
            if close(pixel, rgb(0xd4, 0x6b, 0x42)) {
                red.push((x, y));
            }
        }
    }
    let (pixels, is_red) = if !red.is_empty() {
        (&red, true)
    } else if !blue.is_empty() {
        (&blue, false)
    } else {
        return None;
    };
    let x = pixels.iter().map(|(x, _)| *x).sum::<u32>() / pixels.len() as u32;
    let y = pixels.iter().map(|(_, y)| *y).sum::<u32>() / pixels.len() as u32;
    Some((x.saturating_sub(rect.x), y.saturating_sub(rect.y), is_red))
}

fn horizontal_line_count(image: &DynamicImage, rect: Rect) -> Option<u32> {
    let mut runs = 0;
    let mut in_run = false;
    for y in rect.y..rect.y + rect.height {
        let hit = (rect.x..rect.x + rect.width)
            .any(|x| close(image.get_pixel(x, y), rgb(0xd2, 0xdc, 0xe7)));
        if hit && !in_run {
            runs += 1;
        }
        in_run = hit;
    }
    Some(runs)
}

fn dark_pixels(image: &DynamicImage, rect: Rect) -> Option<u32> {
    if !valid_rect(rect) {
        return None;
    }
    Some(
        (rect.y..rect.y + rect.height)
            .flat_map(|y| (rect.x..rect.x + rect.width).map(move |x| image.get_pixel(x, y)))
            .filter(|pixel| {
                let [red, green, blue, alpha] = pixel.0;
                alpha > 200 && red < 100 && green < 120 && blue < 150
            })
            .count() as u32,
    )
}

fn dark_columns(image: &DynamicImage, rect: Rect) -> Option<u32> {
    if !valid_rect(rect) {
        return None;
    }
    Some(
        (rect.x..rect.x + rect.width)
            .filter(|x| {
                (rect.y..rect.y + rect.height)
                    .filter(|y| {
                        let [red, green, blue, alpha] = image.get_pixel(*x, *y).0;
                        alpha > 200 && red < 100 && green < 120 && blue < 150
                    })
                    .count()
                    >= 8
            })
            .count() as u32,
    )
}

fn blue_pixels(image: &DynamicImage, rect: Rect) -> Option<u32> {
    if !valid_rect(rect) {
        return None;
    }
    Some(
        (rect.y..rect.y + rect.height)
            .flat_map(|y| (rect.x..rect.x + rect.width).map(move |x| image.get_pixel(x, y)))
            .filter(|pixel| close(*pixel, rgb(0x46, 0x79, 0xb7)))
            .count() as u32,
    )
}

fn sample_center(image: &DynamicImage, rect: Rect) -> Option<Rgba<u8>> {
    sample(image, rect.x + rect.width / 2, rect.y + rect.height / 2)
}

fn sample(image: &DynamicImage, x: u32, y: u32) -> Option<Rgba<u8>> {
    (x < image.width() && y < image.height()).then(|| image.get_pixel(x, y))
}

fn rgb(red: u8, green: u8, blue: u8) -> Rgba<u8> {
    Rgba([red, green, blue, 255])
}

fn close(left: Rgba<u8>, right: Rgba<u8>) -> bool {
    left.0
        .iter()
        .zip(right.0)
        .all(|(left, right)| left.abs_diff(right) <= 8)
}

fn valid_geometry(rect: Rect) -> bool {
    valid_rect(rect)
        && rect.x + rect.width <= VIEWPORT_WIDTH
        && rect.y + rect.height <= VIEWPORT_HEIGHT
}

fn valid_rect(rect: Rect) -> bool {
    rect.width > 0
        && rect.height > 0
        && rect.x < VIEWPORT_WIDTH
        && rect.y < VIEWPORT_HEIGHT
        && rect.x + rect.width <= VIEWPORT_WIDTH
        && rect.y + rect.height <= VIEWPORT_HEIGHT
}

fn uncertain() -> (FixtureStateObservation, SubjectObservation) {
    (
        FixtureStateObservation::Unknown(UnknownReason::PredicateUncertain),
        SubjectObservation::Unknown(UnknownReason::PredicateUncertain),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, ImageEncoder, RgbaImage, codecs::png::PngEncoder};
    use temporal_evaluation::BenchmarkDefinition;

    fn png(image: RgbaImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(
                image.as_raw(),
                image.width(),
                image.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        bytes
    }

    fn canvas_frame(x: u32, red: bool) -> Vec<u8> {
        let mut image = ImageBuffer::from_pixel(800, 450, Rgba([255, 255, 255, 255]));
        for y in 0..160 {
            for local_x in 0..320 {
                image.put_pixel(401 + local_x, 241 + y, Rgba([245, 248, 252, 255]));
            }
        }
        for y in 64..96 {
            for local_x in x.saturating_sub(16)..=(x + 16).min(319) {
                if red || ((local_x as i64 - x as i64).pow(2) + (y as i64 - 80).pow(2) <= 256) {
                    image.put_pixel(
                        401 + local_x,
                        241 + y,
                        if red {
                            Rgba([212, 107, 66, 255])
                        } else {
                            Rgba([70, 121, 183, 255])
                        },
                    );
                }
            }
        }
        png(image)
    }

    #[test]
    fn retained_bytes_classify_canvas_reversal_and_sequence_order() {
        let definition = BenchmarkDefinition::canonical();
        let case = definition.case("dom-opaque/path-reversal").unwrap();
        let frames = [
            canvas_frame(80, false),
            canvas_frame(180, false),
            canvas_frame(160, false),
            canvas_frame(320, false),
        ];
        let sequence =
            observe_fixture_sequence(&frames.iter().map(Vec::as_slice).collect::<Vec<_>>(), case);
        assert_eq!(sequence.movement, MovementSequenceObservation::Reversal);
        assert_eq!(sequence.frames[0].state, FixtureStateObservation::Baseline);
        assert_eq!(sequence.frames[3].state, FixtureStateObservation::Final);

        let teleport = definition.case("dom-opaque/teleport").unwrap();
        let teleport_frames = [
            canvas_frame(80, false),
            png(ImageBuffer::from_pixel(
                800,
                450,
                Rgba([245, 248, 252, 255]),
            )),
            canvas_frame(320, false),
        ];
        let teleport_sequence = observe_fixture_sequence(
            &teleport_frames
                .iter()
                .map(Vec::as_slice)
                .collect::<Vec<_>>(),
            teleport,
        );
        assert_eq!(
            teleport_sequence.movement,
            MovementSequenceObservation::Teleport
        );
        assert_eq!(
            teleport_sequence.frames[1].state,
            FixtureStateObservation::Changed
        );
    }

    #[test]
    fn pixel_predicates_cover_flicker_layout_and_stable_observables() {
        let definition = BenchmarkDefinition::canonical();
        let flicker = definition.case("flicker/color").unwrap();
        let mut changed = ImageBuffer::from_pixel(800, 450, Rgba([255, 255, 255, 255]));
        changed.put_pixel(481, 133, Rgba([255, 244, 229, 255]));
        assert_eq!(
            observe_fixture_frame(&png(changed), flicker).state,
            FixtureStateObservation::Changed
        );

        let layout = definition.case("layout/width").unwrap();
        let mut narrow = ImageBuffer::from_pixel(800, 450, Rgba([255, 255, 255, 255]));
        narrow.put_pixel(650, 300, Rgba([255, 255, 255, 255]));
        assert_eq!(
            observe_fixture_frame(&png(narrow), layout).state,
            FixtureStateObservation::Changed
        );

        let content_shift = definition.case("layout/content-shift").unwrap();
        let mut notice = ImageBuffer::from_pixel(800, 450, Rgba([245, 248, 252, 255]));
        notice.put_pixel(59, 260, Rgba([232, 238, 246, 255]));
        assert_eq!(
            observe_fixture_frame(&png(notice), content_shift).state,
            FixtureStateObservation::Changed
        );

        let stable = definition.case("stable/loading-indicator").unwrap();
        let mut spinner = ImageBuffer::from_pixel(800, 450, Rgba([255, 255, 255, 255]));
        for index in 0..9 {
            spinner.put_pixel(385 + index, 120, Rgba([70, 121, 183, 255]));
        }
        assert_eq!(
            observe_fixture_frame(&png(spinner), stable).state,
            FixtureStateObservation::Changed
        );
    }

    #[test]
    fn decode_viewport_scale_geometry_and_predicate_uncertainty_are_unknown() {
        let definition = BenchmarkDefinition::canonical();
        let case = definition.case("flicker/color").unwrap();
        assert!(matches!(
            observe_fixture_frame(b"not-an-image", case).state,
            FixtureStateObservation::Unknown(UnknownReason::Decode)
        ));
        let bytes = png(ImageBuffer::from_pixel(
            800,
            450,
            Rgba([255, 255, 255, 255]),
        ));
        assert!(matches!(
            observe_fixture_frame_with_geometry(
                &bytes,
                case,
                FrameGeometry {
                    device_scale_factor_milli: 2_000,
                    ..FrameGeometry::CANONICAL
                }
            )
            .state,
            FixtureStateObservation::Unknown(UnknownReason::ScaleMismatch)
        ));
        assert!(matches!(
            observe_fixture_frame_with_geometry(
                &bytes,
                case,
                FrameGeometry {
                    width: 799,
                    ..FrameGeometry::CANONICAL
                }
            )
            .state,
            FixtureStateObservation::Unknown(UnknownReason::ViewportMismatch)
        ));
    }
}
