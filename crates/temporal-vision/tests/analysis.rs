use std::num::{NonZeroU8, NonZeroUsize};

use serde::{Deserialize, Serialize};
use temporal_vision::{
    BinaryMask, ChangedPixelProportion, ComparisonOutcome, DeclaredGap, ErrorCode, Frame,
    FrameRegion, FrameSequence, IntegerScale, Marker, MeasurementParameters, NormalizationKind,
    NormalizationParameters, ParameterValue, PixelDimensions, PixelFormat, PixelRect,
    ProcessingLimits, Rgb8, TimeRange, Timestamp, measure_adjacent, measure_pair,
    normalize_sequence,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FrameId(&'static str);

#[derive(Clone, Debug, Eq, PartialEq)]
struct MarkerId(&'static str);

#[derive(Clone, Debug, Eq, PartialEq)]
struct GapId(&'static str);

fn borrowed_frame<'a>(
    id: &'static str,
    timestamp: u64,
    dimensions: PixelDimensions,
    pixels: &'a [u8],
) -> Frame<FrameId, &'a [u8]> {
    Frame::new(
        FrameId(id),
        Timestamp::from_nanos(timestamp),
        dimensions,
        PixelFormat::Rgba8SrgbStraight,
        pixels,
    )
    .unwrap()
}

#[test]
fn borrowed_alpha_pixels_normalize_to_owned_repeatable_linear_rgb16() {
    let dimensions = PixelDimensions::new(2, 1).unwrap();
    let source = [255, 0, 0, 0, 255, 255, 255, 128];
    let sequence = FrameSequence::new(
        vec![borrowed_frame("a", 10, dimensions, &source)],
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        None,
        None,
    )
    .unwrap();
    let parameters = NormalizationParameters::new(
        Rgb8::new(0, 128, 0),
        Some(PixelRect::new(1, 0, 1, 1).unwrap()),
        IntegerScale::up(NonZeroU8::new(2).unwrap()).unwrap(),
        ProcessingLimits::default(),
    );

    let first = normalize_sequence(&sequence, parameters).unwrap();
    let second = normalize_sequence(&sequence, parameters).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.source_dimensions(), dimensions);
    assert_eq!(first.source_crop(), PixelRect::new(1, 0, 1, 1).unwrap());
    assert_eq!(first.dimensions(), PixelDimensions::new(2, 2).unwrap());
    assert_eq!(first.frames()[0].id(), &FrameId("a"));
    assert!(
        first.frames()[0]
            .linear_rgb16()
            .chunks_exact(3)
            .all(|pixel| pixel == [32_896, 39_941, 32_896])
    );
    assert_eq!(
        first
            .normalization_steps()
            .iter()
            .map(|step| step.kind())
            .collect::<Vec<_>>(),
        vec![
            NormalizationKind::ColorSpaceConversion,
            NormalizationKind::AlphaCompositing,
            NormalizationKind::FixedCrop,
            NormalizationKind::IntegerScaling,
        ]
    );
    assert_eq!(
        serde_json::to_vec(first.normalization_steps()).unwrap(),
        serde_json::to_vec(second.normalization_steps()).unwrap()
    );
}

#[test]
fn scaling_transforms_pixels_regions_and_masks_without_admitting_excluded_samples() {
    let dimensions = PixelDimensions::new(2, 2).unwrap();
    let source = [
        255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
    ];
    let full_mask = BinaryMask::new(dimensions, [0xf0]).unwrap();
    let full_region = FrameRegion::new(PixelRect::new(0, 0, 2, 2).unwrap(), dimensions).unwrap();
    let sequence = FrameSequence::new(
        vec![borrowed_frame("a", 10, dimensions, &source)],
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        Some(full_region),
        Some(full_mask),
    )
    .unwrap();
    let downscaled = normalize_sequence(
        &sequence,
        NormalizationParameters::new(
            Rgb8::new(17, 31, 47),
            None,
            IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    assert_eq!(downscaled.dimensions(), PixelDimensions::new(1, 1).unwrap());
    assert_eq!(downscaled.frames()[0].linear_rgb16(), [32_768; 3]);
    assert_eq!(downscaled.analysis_mask().unwrap().bits(), [0x80]);
    assert_eq!(downscaled.analysis_pixel_count(), 1);

    let partial_mask = BinaryMask::new(dimensions, [0xe0]).unwrap();
    let partial = FrameSequence::new(
        vec![borrowed_frame("a", 10, dimensions, &source)],
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        None,
        Some(partial_mask),
    )
    .unwrap();
    assert_eq!(
        normalize_sequence(
            &partial,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
                ProcessingLimits::default(),
            ),
        )
        .unwrap_err()
        .code,
        ErrorCode::EmptyAnalysisDomain
    );

    let row_dimensions = PixelDimensions::new(2, 1).unwrap();
    let row = [0_u8, 0, 0, 255, 255, 255, 255, 255];
    let masked_row = FrameSequence::new(
        vec![borrowed_frame("row", 10, row_dimensions, &row)],
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        None,
        Some(BinaryMask::new(row_dimensions, [0x80]).unwrap()),
    )
    .unwrap();
    let upscaled = normalize_sequence(
        &masked_row,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::up(NonZeroU8::new(2).unwrap()).unwrap(),
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    assert_eq!(upscaled.analysis_mask().unwrap().bits(), [0xcc]);
    assert_eq!(upscaled.analysis_pixel_count(), 4);
}

#[test]
fn exact_measurements_and_gap_boundaries_share_one_public_kernel() {
    let dimensions = PixelDimensions::new(2, 2).unwrap();
    let black = [0_u8; 16];
    let changed = [255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 255];
    let later = changed;
    let gap = DeclaredGap::new(
        GapId("loss"),
        TimeRange::new(Timestamp::from_nanos(25), Timestamp::from_nanos(25)).unwrap(),
        "known loss",
        None,
    )
    .unwrap();
    let sequence = FrameSequence::new(
        vec![
            borrowed_frame("before", 10, dimensions, &black),
            borrowed_frame("changed", 20, dimensions, &changed),
            borrowed_frame("later", 30, dimensions, &later),
        ],
        Vec::<Marker<MarkerId>>::new(),
        vec![gap],
        None,
        None,
    )
    .unwrap();
    let normalized = normalize_sequence(
        &sequence,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();

    let parameters = MeasurementParameters::new(0);
    let exact = measure_pair(&normalized, 0, 1, parameters).unwrap();
    assert_eq!(exact.earlier_frame_index(), 0);
    assert_eq!(exact.later_frame_index(), 1);
    assert_eq!(exact.elapsed_nanos(), 10);
    let ComparisonOutcome::Measured(vector) = exact.outcome() else {
        panic!("the first pair does not cross the declared gap")
    };
    assert_eq!(vector.absolute_pixel_difference(), 196_605);
    assert_eq!(
        vector.changed_pixel_proportion(),
        serde_json::from_value::<ChangedPixelProportion>(serde_json::json!({
            "changed": 3,
            "compared": 4
        }))
        .unwrap()
    );
    assert_eq!(
        vector.changed_region_bounds(),
        Some(PixelRect::new(0, 0, 2, 2).unwrap())
    );
    assert_eq!(vector.mean_luminance_difference(), 16_384);
    assert_eq!(vector.mean_color_difference(), 16_384);
    assert_eq!(vector.perceptual_frame_distance(), 32_767);

    let repeated = measure_pair(&normalized, 0, 1, parameters).unwrap();
    assert_eq!(exact, repeated);
    assert_eq!(
        serde_json::to_vec(&exact).unwrap(),
        serde_json::to_vec(&repeated).unwrap()
    );

    let adjacent = measure_adjacent(&normalized, parameters).unwrap();
    assert_eq!(adjacent.len(), 2);
    assert!(matches!(
        adjacent[0].outcome(),
        ComparisonOutcome::Measured(_)
    ));
    assert!(matches!(
        adjacent[1].outcome(),
        ComparisonOutcome::GapBoundary { declared_gap_count }
            if declared_gap_count.get() == 1
    ));
    assert_eq!(adjacent[1].elapsed_nanos(), 10);
    assert!(matches!(
        measure_pair(&normalized, 0, 2, parameters)
            .unwrap()
            .outcome(),
        ComparisonOutcome::GapBoundary { .. }
    ));

    let mut all_steps = normalized.normalization_steps().to_vec();
    all_steps.push(parameters.provenance_step().unwrap());
    let threshold_step = all_steps.last().unwrap();
    assert_eq!(threshold_step.kind(), NormalizationKind::Thresholding);
    assert_eq!(
        threshold_step.parameters().get("noise_floor"),
        Some(&ParameterValue::Unsigned(0))
    );
    assert_eq!(
        threshold_step.parameters().get("comparison"),
        Some(&ParameterValue::Text(
            "weighted_square > noise_floor^2 * weight_sum".into()
        ))
    );
    let first_json = serde_json::to_vec(&all_steps).unwrap();
    let second_json = serde_json::to_vec(&all_steps).unwrap();
    assert_eq!(first_json, second_json);
}

#[test]
fn threshold_mask_and_limits_fail_or_measure_at_exact_boundaries() {
    let dimensions = PixelDimensions::new(2, 1).unwrap();
    let before = [0_u8, 0, 0, 255, 0, 0, 0, 255];
    let after = [1_u8, 1, 1, 255, 255, 255, 255, 255];
    let sequence = FrameSequence::new(
        vec![
            borrowed_frame("before", 10, dimensions, &before),
            borrowed_frame("after", 10, dimensions, &after),
        ],
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        None,
        Some(BinaryMask::new(dimensions, [0x80]).unwrap()),
    )
    .unwrap();
    let normalized = normalize_sequence(
        &sequence,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    let delta = normalized.frames()[1].linear_rgb16()[0];
    assert_eq!(delta, 20);
    let at_floor_comparison =
        measure_pair(&normalized, 0, 1, MeasurementParameters::new(delta)).unwrap();
    let ComparisonOutcome::Measured(at_floor) = at_floor_comparison.outcome() else {
        panic!("no gap is declared")
    };
    assert_eq!(at_floor.changed_pixel_proportion().changed(), 0);
    assert_eq!(at_floor.changed_pixel_proportion().compared(), 1);
    let over_floor_comparison =
        measure_pair(&normalized, 0, 1, MeasurementParameters::new(delta - 1)).unwrap();
    let ComparisonOutcome::Measured(over_floor) = over_floor_comparison.outcome() else {
        panic!("no gap is declared")
    };
    assert_eq!(over_floor.changed_pixel_proportion().changed(), 1);
    assert_eq!(
        over_floor.changed_region_bounds(),
        Some(PixelRect::new(0, 0, 1, 1).unwrap())
    );
    assert_eq!(
        measure_pair(&normalized, 1, 0, MeasurementParameters::default())
            .unwrap_err()
            .code,
        ErrorCode::InvalidParameter
    );
    assert_eq!(
        measure_pair(&normalized, 0, 2, MeasurementParameters::default())
            .unwrap_err()
            .code,
        ErrorCode::InvalidParameter
    );

    let one = NonZeroUsize::new(1).unwrap();
    for limits in [
        ProcessingLimits::new(
            one,
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(24).unwrap(),
        ),
        ProcessingLimits::new(
            NonZeroUsize::new(2).unwrap(),
            one,
            NonZeroUsize::new(24).unwrap(),
        ),
        ProcessingLimits::new(
            NonZeroUsize::new(2).unwrap(),
            NonZeroUsize::new(2).unwrap(),
            one,
        ),
    ] {
        assert_eq!(
            normalize_sequence(
                &sequence,
                NormalizationParameters::new(
                    Rgb8::new(0, 0, 0),
                    None,
                    IntegerScale::IDENTITY,
                    limits,
                ),
            )
            .unwrap_err()
            .code,
            ErrorCode::ResourceLimitExceeded
        );
    }
    assert_eq!(
        IntegerScale::down(NonZeroU8::new(9).unwrap())
            .unwrap_err()
            .code,
        ErrorCode::InvalidScale
    );
    assert_eq!(
        normalize_sequence(
            &sequence,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                None,
                IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
                ProcessingLimits::default(),
            ),
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidScale
    );
    assert_eq!(
        normalize_sequence(
            &sequence,
            NormalizationParameters::new(
                Rgb8::new(0, 0, 0),
                Some(PixelRect::new(1, 0, 2, 1).unwrap()),
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidRegion
    );

    // The public contract stays deterministic object data rather than floating metrics.
    let serialized = serde_json::to_value(over_floor).unwrap();
    assert!(!contains_json_number_with_fraction(&serialized));
}

fn contains_json_number_with_fraction(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(number) => number.as_i64().is_none() && number.as_u64().is_none(),
        serde_json::Value::Array(values) => values.iter().any(contains_json_number_with_fraction),
        serde_json::Value::Object(values) => {
            values.values().any(contains_json_number_with_fraction)
        }
        _ => false,
    }
}
