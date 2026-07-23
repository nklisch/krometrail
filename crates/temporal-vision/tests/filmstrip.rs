use std::num::{NonZeroU8, NonZeroU32, NonZeroUsize};

use png::{ColorType, Decoder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use temporal_vision::{
    ArtifactKind, ArtifactManifest, BinaryMask, DeclaredGap, ErrorCode, EvidenceClass,
    FilmstripTileLimit, Frame, FrameRegion, FrameSequence, IntegerScale, Marker, ParameterValue,
    PixelDimensions, PixelFormat, PixelRect, RationalScale, RegionDefinition,
    RegionFilmstripLabels, RegionFilmstripParameters, RegionFilmstripRenderLimits, Rgb8,
    SignedPixelRect, TimeRange, Timestamp, ViewportMapping, generate_region_filmstrip,
    plan_region_filmstrip,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FrameId(String);

impl std::fmt::Display for FrameId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MarkerId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct GapId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ArtifactId(String);

type Source = FrameSequence<FrameId, MarkerId, GapId, Box<[u8]>>;

fn fixture() -> Source {
    let dimensions = PixelDimensions::new(4, 4).unwrap();
    let frames = (0_u8..5)
        .map(|frame| {
            let mut pixels = Vec::with_capacity(dimensions.rgba8_byte_len().unwrap());
            for y in 0_u8..4 {
                for x in 0_u8..4 {
                    pixels.extend_from_slice(&[frame * 40 + x * 5, y * 30, 100 + frame, 255]);
                }
            }
            Frame::new(
                FrameId(format!("f{frame}")),
                Timestamp::from_nanos(u64::from(frame) * 10_000_000),
                dimensions,
                PixelFormat::Rgba8SrgbStraight,
                pixels.into_boxed_slice(),
            )
            .unwrap()
        })
        .collect();
    let markers = vec![
        Marker::new(
            MarkerId("action".into()),
            Timestamp::from_nanos(20_000_000),
            "action",
            "open panel",
        )
        .unwrap(),
    ];
    let gaps = vec![
        DeclaredGap::new(
            GapId("loss".into()),
            TimeRange::new(
                Timestamp::from_nanos(25_000_000),
                Timestamp::from_nanos(25_000_000),
            )
            .unwrap(),
            "capture saturation",
            None,
        )
        .unwrap(),
    ];
    FrameSequence::new(frames, markers, gaps, None, None).unwrap()
}

fn rect(x: i64, y: i64, width: u32, height: u32) -> SignedPixelRect {
    SignedPixelRect::new(
        x,
        y,
        NonZeroU32::new(width).unwrap(),
        NonZeroU32::new(height).unwrap(),
    )
    .unwrap()
}

fn request(
    region: RegionDefinition,
    scale: IntegerScale,
    limits: RegionFilmstripRenderLimits,
) -> RegionFilmstripParameters {
    RegionFilmstripParameters::new(
        region,
        Timestamp::from_nanos(20_000_000),
        FilmstripTileLimit::new(3).unwrap(),
        Rgb8::new(1, 2, 3),
        Rgb8::new(7, 11, 13),
        scale,
        RegionFilmstripLabels::new("Panel region over time", "Synthetic viewport").unwrap(),
        limits,
    )
}

#[test]
fn planning_preserves_fixed_coordinate_semantics_padding_thinning_and_locator_choice() {
    let source = fixture();
    let source_region = RegionDefinition::FixedSourceImage {
        rect: rect(-1, 1, 4, 4),
    };
    let plan = plan_region_filmstrip(
        &source,
        source_region,
        Timestamp::from_nanos(20_000_000),
        FilmstripTileLimit::new(3).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(
        plan.tiles()
            .iter()
            .map(|tile| tile.frame_index())
            .collect::<Vec<_>>(),
        [0, 2, 4]
    );
    assert_eq!(plan.locator_frame_index(), 2);
    assert_eq!(plan.omitted_frame_count(), 2);
    assert_eq!(
        plan.tiles()[0].source_rect(),
        Some(PixelRect::new(0, 1, 3, 3).unwrap())
    );
    let padding = plan.tiles()[0].padding();
    assert_eq!(
        (
            padding.left(),
            padding.top(),
            padding.right(),
            padding.bottom()
        ),
        (1, 0, 0, 1)
    );
    assert!(!plan.tiles()[0].gap_after());
    assert!(plan.tiles()[1].gap_after());
    assert_eq!(plan.tiles()[0].anchor_offset_nanos(), -20_000_000);
    assert_eq!(plan.tiles()[2].anchor_offset_nanos(), 20_000_000);

    let mapping = ViewportMapping::new(
        PixelDimensions::new(2, 2).unwrap(),
        RationalScale::new(NonZeroU32::new(2).unwrap(), NonZeroU32::MIN),
        RationalScale::new(NonZeroU32::new(2).unwrap(), NonZeroU32::MIN),
    );
    let viewport_region = RegionDefinition::FixedViewport {
        rect: rect(-1, 1, 3, 2),
        mapping,
    };
    let viewport = plan_region_filmstrip(
        &source,
        viewport_region,
        Timestamp::from_nanos(20_000_000),
        FilmstripTileLimit::DEFAULT,
        Some(4),
    )
    .unwrap();
    assert_eq!(viewport.locator_frame_index(), 4);
    assert_eq!(viewport.resolved_source_region(), rect(-2, 2, 6, 4));
    assert_eq!(
        viewport.tiles()[0].source_rect(),
        Some(PixelRect::new(0, 2, 4, 2).unwrap())
    );
    let padding = viewport.tiles()[0].padding();
    assert_eq!(
        (
            padding.left(),
            padding.top(),
            padding.right(),
            padding.bottom()
        ),
        (2, 0, 0, 2)
    );
    assert_ne!(
        serde_json::to_vec(&source_region).unwrap(),
        serde_json::to_vec(&viewport_region).unwrap()
    );

    let fully_outside = plan_region_filmstrip(
        &source,
        RegionDefinition::FixedSourceImage {
            rect: rect(10, 10, 2, 3),
        },
        Timestamp::from_nanos(20_000_000),
        FilmstripTileLimit::new(1).unwrap(),
        None,
    )
    .unwrap();
    assert_eq!(fully_outside.tiles()[0].source_rect(), None);
    assert_eq!(
        fully_outside.tile_source_dimensions(),
        PixelDimensions::new(2, 3).unwrap()
    );
    assert!(serde_json::from_str::<FilmstripTileLimit>("25").is_err());
}

#[test]
fn explicit_anchor_outside_source_range_stays_rejected() {
    let error = plan_region_filmstrip(
        &fixture(),
        RegionDefinition::FixedSourceImage {
            rect: rect(0, 0, 1, 1),
        },
        Timestamp::from_nanos(50_000_000),
        FilmstripTileLimit::DEFAULT,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidParameter);
    assert_eq!(
        error.message.as_ref(),
        "filmstrip anchor lies outside the source range"
    );
}

#[test]
fn generator_renders_traceable_padding_locator_gaps_and_deterministic_manifest() {
    let source = fixture();
    let region = RegionDefinition::FixedSourceImage {
        rect: rect(-1, 1, 4, 4),
    };
    let first = generate_region_filmstrip(
        ArtifactId("filmstrip-a".into()),
        &source,
        request(
            region,
            IntegerScale::IDENTITY,
            RegionFilmstripRenderLimits::default(),
        ),
    )
    .unwrap();
    let second = generate_region_filmstrip(
        ArtifactId("filmstrip-a".into()),
        &source,
        request(
            region,
            IntegerScale::IDENTITY,
            RegionFilmstripRenderLimits::default(),
        ),
    )
    .unwrap();
    assert_eq!(first, second);
    assert_eq!(first.plan().tiles().len(), 3);
    assert_eq!(
        first.manifest().selected_frame_ids(),
        &[
            FrameId("f0".into()),
            FrameId("f2".into()),
            FrameId("f4".into())
        ]
    );
    assert_eq!(first.manifest().source_frame_count(), 5);
    // A filmstrip examines the frames it renders. Frames 1 and 3 back no tile
    // and are referenced nowhere in the output, so they contributed nothing and
    // are omitted evidence — not `analyzed - selected`.
    assert_eq!(first.manifest().analyzed_frame_count(), 3);
    assert_eq!(first.manifest().omitted_frame_count(), 2);
    assert_eq!(
        first.manifest().analyzed_frame_ids(),
        first.manifest().selected_frame_ids(),
        "every analyzed frame of a filmstrip is a rendered or referenced one"
    );
    assert_eq!(
        first.manifest().artifact_kind(),
        ArtifactKind::RegionFilmstrip
    );
    assert_eq!(
        first.manifest().evidence_class(),
        EvidenceClass::SourceDerived
    );
    assert_eq!(first.manifest().algorithm().name(), "region-filmstrip");
    assert_eq!(first.manifest().algorithm().version(), "1.0.0");
    assert_eq!(first.manifest().region(), None);
    assert_eq!(
        first.manifest().parameters().get("tracking_method"),
        Some(&ParameterValue::Text("none".into()))
    );
    assert_eq!(
        first.manifest().parameters().get("coordinate_space"),
        Some(&ParameterValue::Text("source_image".into()))
    );
    assert_eq!(
        first.manifest().parameters().get("gap_warning_count"),
        Some(&ParameterValue::Unsigned(1))
    );

    let bytes = first.image().bytes();
    assert_eq!(first.image().media_type(), "image/png");
    assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
    let digest: [u8; 32] = Sha256::digest(bytes).into();
    assert_eq!(first.manifest().output_hash().as_bytes(), &digest);
    let manifest_json = serde_json::to_vec(first.manifest()).unwrap();
    let decoded_manifest: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> =
        serde_json::from_slice(&manifest_json).unwrap();
    assert_eq!(&decoded_manifest, first.manifest());

    let (dimensions, pixels) = decode_rgb(bytes);
    assert_eq!(dimensions, PixelDimensions::new(744, 356).unwrap());
    assert_eq!(dimensions, first.image().dimensions());
    // First crop image starts at x=306/y=76. Missing left/bottom pixels are
    // generated padding; source pixel (0,1) remains exact beside it.
    assert_eq!(rgb_at(&pixels, dimensions, 306, 79), [7, 11, 13]);
    assert_eq!(rgb_at(&pixels, dimensions, 307, 76), [0, 30, 100]);
    assert!(region_has_color(
        &pixels,
        dimensions,
        306,
        76,
        1,
        4,
        [255, 196, 64]
    ));
    // Locator edge chevrons and the gap separator are visible patterns, while
    // header glyphs prove the semantic bands were rendered separately.
    assert!(region_has_color(
        &pixels,
        dimensions,
        12,
        76,
        200,
        200,
        [255, 196, 64]
    ));
    assert!(region_has_color(
        &pixels,
        dimensions,
        550,
        76,
        12,
        60,
        [255, 196, 64]
    ));
    assert!(region_has_color(
        &pixels,
        dimensions,
        0,
        0,
        300,
        64,
        [244, 247, 250]
    ));

    assert_eq!(
        first.manifest().output_hash().to_string(),
        "de1a2f3b43834d0bbc08e4b3065e343d4f425df8cbe249dc09a9884c03aa9eae"
    );

    let in_bounds = generate_region_filmstrip(
        ArtifactId("in-bounds".into()),
        &source,
        request(
            RegionDefinition::FixedSourceImage {
                rect: rect(1, 1, 2, 2),
            },
            IntegerScale::up(NonZeroU8::new(2).unwrap()).unwrap(),
            RegionFilmstripRenderLimits::default(),
        ),
    )
    .unwrap();
    assert_eq!(
        in_bounds.manifest().region(),
        Some(FrameRegion::new(PixelRect::new(1, 1, 2, 2).unwrap(), source.dimensions()).unwrap())
    );

    // A locator frame outside the crop strip is still a source behind the
    // combined artifact and therefore belongs in the manifest subsequence.
    let explicit_locator = generate_region_filmstrip(
        ArtifactId("explicit-locator".into()),
        &source,
        request(
            RegionDefinition::FixedSourceImage {
                rect: rect(1, 1, 2, 2),
            },
            IntegerScale::IDENTITY,
            RegionFilmstripRenderLimits::default(),
        )
        .with_locator_frame_index(1),
    )
    .unwrap();
    assert_eq!(
        explicit_locator.manifest().selected_frame_ids(),
        &[
            FrameId("f0".into()),
            FrameId("f1".into()),
            FrameId("f2".into()),
            FrameId("f4".into())
        ]
    );
    // The explicit locator is referenced by the output even though it backs no
    // tile, so it counts as analyzed. Only f3 contributed nothing.
    assert_eq!(explicit_locator.manifest().analyzed_frame_count(), 4);
    assert_eq!(explicit_locator.manifest().omitted_frame_count(), 1);
    // The parameter block and the manifest now count the same population: a
    // filmstrip's omitted frames are the source frames it never looked at.
    assert_eq!(
        explicit_locator
            .manifest()
            .parameters()
            .get("omitted_frame_count"),
        Some(&ParameterValue::Unsigned(1))
    );
    assert_eq!(
        explicit_locator
            .manifest()
            .parameters()
            .get("strip_omitted_frame_count"),
        Some(&ParameterValue::Unsigned(2))
    );
    assert_eq!(explicit_locator.plan().omitted_frame_count(), 2);
}

#[test]
fn fractional_bounds_and_canonical_viewport_mapping_round_outward_without_overflow() {
    let outward = SignedPixelRect::from_outward_f64_bounds(-0.25, 1.1, 2.25, 4.2).unwrap();
    assert_eq!(
        (outward.x(), outward.y(), outward.width(), outward.height()),
        (-1, 1, 4, 4)
    );
    assert!(SignedPixelRect::from_outward_f64_bounds(0.0, 0.0, f64::INFINITY, 1.0).is_err());
    assert!(
        SignedPixelRect::from_outward_f64_bounds(
            i64::MAX as f64,
            0.0,
            (i64::MAX as f64) + 1.0,
            1.0,
        )
        .is_err()
    );
    assert!(
        SignedPixelRect::from_outward_f64_bounds(0.0, 0.0, u32::MAX as f64 + 2.0, 1.0).is_err()
    );

    let mapping = ViewportMapping::for_source(
        PixelDimensions::new(6, 4).unwrap(),
        PixelDimensions::new(8, 6).unwrap(),
    );
    assert_eq!(
        (
            mapping.scale_x().numerator(),
            mapping.scale_x().denominator()
        ),
        (4, 3)
    );
    assert_eq!(
        (
            mapping.scale_y().numerator(),
            mapping.scale_y().denominator()
        ),
        (3, 2)
    );
}

#[test]
fn fixed_mask_bounds_pixels_legend_manifest_and_identity_are_deterministic() {
    let source = fixture();
    // Selected source pixels are (1,1), (2,1), and (1,2). The fourth pixel in
    // their 2x2 bounds remains visibly excluded in every tile.
    let mask = BinaryMask::new(PixelDimensions::new(4, 4).unwrap(), [0x06, 0x40]).unwrap();
    assert_eq!(
        mask.bounds().unwrap(),
        Some(PixelRect::new(1, 1, 2, 2).unwrap())
    );
    let parameters = request(
        RegionDefinition::FixedSourceImage {
            rect: rect(0, 0, 4, 4),
        },
        IntegerScale::IDENTITY,
        RegionFilmstripRenderLimits::default(),
    )
    .with_mask(mask.clone())
    .unwrap();
    let first = generate_region_filmstrip(ArtifactId("masked".into()), &source, parameters.clone())
        .unwrap();
    let second =
        generate_region_filmstrip(ArtifactId("masked".into()), &source, parameters).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.plan().resolved_source_region(), rect(1, 1, 2, 2));
    assert_eq!(first.manifest().mask(), Some(&mask));
    assert_eq!(
        first.manifest().region(),
        Some(FrameRegion::new(PixelRect::new(1, 1, 2, 2).unwrap(), source.dimensions()).unwrap())
    );
    assert!(matches!(
        first.manifest().parameters().get("mask"),
        Some(ParameterValue::Object(_))
    ));
    assert!(
        first
            .manifest()
            .normalization()
            .iter()
            .any(|step| step.algorithm_version() == "fixed-binary-mask-v1")
    );

    let (dimensions, pixels) = decode_rgb(first.image().bytes());
    assert_eq!(rgb_at(&pixels, dimensions, 307, 76), [5, 30, 100]);
    assert_eq!(rgb_at(&pixels, dimensions, 308, 77), [7, 11, 13]);
    // The dedicated mask legend occupies the final header line; this band is
    // muted in the no-mask rendering and warning-colored only for a mask.
    assert!(region_has_color(
        &pixels,
        dimensions,
        0,
        49,
        600,
        15,
        [255, 196, 64]
    ));

    let changed_mask = BinaryMask::new(PixelDimensions::new(4, 4).unwrap(), [0x06, 0x60]).unwrap();
    let changed = generate_region_filmstrip(
        ArtifactId("masked".into()),
        &source,
        request(
            RegionDefinition::FixedSourceImage {
                rect: rect(0, 0, 4, 4),
            },
            IntegerScale::IDENTITY,
            RegionFilmstripRenderLimits::default(),
        )
        .with_mask(changed_mask)
        .unwrap(),
    )
    .unwrap();
    assert_ne!(
        first.manifest().parameters(),
        changed.manifest().parameters()
    );
    assert_ne!(first.image().bytes(), changed.image().bytes());

    assert!(
        request(
            RegionDefinition::FixedSourceImage {
                rect: rect(0, 0, 4, 4),
            },
            IntegerScale::IDENTITY,
            RegionFilmstripRenderLimits::default(),
        )
        .with_mask(BinaryMask::new(PixelDimensions::new(2, 2).unwrap(), [0]).unwrap())
        .is_err()
    );
    let wrong_dimensions = BinaryMask::new(PixelDimensions::new(2, 2).unwrap(), [0x80]).unwrap();
    assert_eq!(
        generate_region_filmstrip(
            ArtifactId("wrong-mask".into()),
            &source,
            request(
                RegionDefinition::FixedSourceImage {
                    rect: rect(0, 0, 4, 4),
                },
                IntegerScale::IDENTITY,
                RegionFilmstripRenderLimits::default(),
            )
            .with_mask(wrong_dimensions)
            .unwrap(),
        )
        .unwrap_err()
        .code,
        ErrorCode::InvalidMask
    );
}

#[test]
fn viewport_fully_outside_and_boundary_failures_remain_explicit_and_bounded() {
    let source = fixture();
    let mapping = ViewportMapping::new(
        PixelDimensions::new(2, 2).unwrap(),
        RationalScale::new(NonZeroU32::new(2).unwrap(), NonZeroU32::MIN),
        RationalScale::new(NonZeroU32::new(2).unwrap(), NonZeroU32::MIN),
    );
    let artifact = generate_region_filmstrip(
        ArtifactId("outside".into()),
        &source,
        request(
            RegionDefinition::FixedViewport {
                rect: rect(5, 5, 2, 2),
                mapping,
            },
            IntegerScale::IDENTITY,
            RegionFilmstripRenderLimits::default(),
        ),
    )
    .unwrap();
    assert!(
        artifact
            .plan()
            .tiles()
            .iter()
            .all(|tile| tile.source_rect().is_none())
    );
    assert_eq!(artifact.manifest().region(), None);
    assert_eq!(
        artifact.manifest().parameters().get("tracking_method"),
        Some(&ParameterValue::Text("none".into()))
    );
    let (dimensions, pixels) = decode_rgb(artifact.image().bytes());
    // Padding remains visually explicit: both declared color and warning hatch.
    assert!(region_has_color(
        &pixels,
        dimensions,
        300,
        70,
        20,
        20,
        [7, 11, 13]
    ));
    assert!(region_has_color(
        &pixels,
        dimensions,
        300,
        70,
        20,
        20,
        [255, 196, 64]
    ));

    let contradictory = ViewportMapping::new(
        PixelDimensions::new(3, 2).unwrap(),
        mapping.scale_x(),
        mapping.scale_y(),
    );
    let bad_mapping = plan_region_filmstrip(
        &source,
        RegionDefinition::FixedViewport {
            rect: rect(0, 0, 1, 1),
            mapping: contradictory,
        },
        Timestamp::from_nanos(20_000_000),
        FilmstripTileLimit::DEFAULT,
        None,
    )
    .unwrap_err();
    assert_eq!(bad_mapping.code, ErrorCode::InvalidScale);

    let invalid_downscale = generate_region_filmstrip(
        ArtifactId("bad-scale".into()),
        &source,
        request(
            RegionDefinition::FixedSourceImage {
                rect: rect(0, 0, 3, 4),
            },
            IntegerScale::down(NonZeroU8::new(2).unwrap()).unwrap(),
            RegionFilmstripRenderLimits::default(),
        ),
    )
    .unwrap_err();
    assert_eq!(invalid_downscale.code, ErrorCode::InvalidScale);

    let defaults = RegionFilmstripRenderLimits::default();
    let bounded = [
        RegionFilmstripRenderLimits::new(
            NonZeroU32::new(300).unwrap(),
            NonZeroU32::new(defaults.max_height()).unwrap(),
            NonZeroUsize::new(defaults.max_canvas_bytes()).unwrap(),
            NonZeroUsize::new(defaults.max_encoded_bytes()).unwrap(),
        ),
        RegionFilmstripRenderLimits::new(
            NonZeroU32::new(defaults.max_width()).unwrap(),
            NonZeroU32::new(100).unwrap(),
            NonZeroUsize::new(defaults.max_canvas_bytes()).unwrap(),
            NonZeroUsize::new(defaults.max_encoded_bytes()).unwrap(),
        ),
        RegionFilmstripRenderLimits::new(
            NonZeroU32::new(defaults.max_width()).unwrap(),
            NonZeroU32::new(defaults.max_height()).unwrap(),
            NonZeroUsize::new(100).unwrap(),
            NonZeroUsize::new(defaults.max_encoded_bytes()).unwrap(),
        ),
        RegionFilmstripRenderLimits::new(
            NonZeroU32::new(defaults.max_width()).unwrap(),
            NonZeroU32::new(defaults.max_height()).unwrap(),
            NonZeroUsize::new(defaults.max_canvas_bytes()).unwrap(),
            NonZeroUsize::new(8).unwrap(),
        ),
        defaults.with_max_source_frames(NonZeroUsize::new(1).unwrap()),
    ];
    for limits in bounded {
        assert_eq!(
            generate_region_filmstrip(
                ArtifactId("bounded".into()),
                &source,
                request(
                    RegionDefinition::FixedSourceImage {
                        rect: rect(0, 0, 2, 2)
                    },
                    IntegerScale::IDENTITY,
                    limits,
                ),
            )
            .unwrap_err()
            .code,
            ErrorCode::ResourceLimitExceeded
        );
    }
}

fn decode_rgb(bytes: &[u8]) -> (PixelDimensions, Vec<u8>) {
    let mut reader = Decoder::new(bytes).read_info().unwrap();
    let mut output = vec![0_u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut output).unwrap();
    assert_eq!(info.color_type, ColorType::Rgb);
    output.truncate(info.buffer_size());
    (
        PixelDimensions::new(info.width, info.height).unwrap(),
        output,
    )
}

fn rgb_at(pixels: &[u8], dimensions: PixelDimensions, x: u32, y: u32) -> [u8; 3] {
    let index = (y as usize * dimensions.width() as usize + x as usize) * 3;
    [pixels[index], pixels[index + 1], pixels[index + 2]]
}

#[allow(clippy::too_many_arguments)]
fn region_has_color(
    pixels: &[u8],
    dimensions: PixelDimensions,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    color: [u8; 3],
) -> bool {
    (y..y + height)
        .any(|row| (x..x + width).any(|column| rgb_at(pixels, dimensions, column, row) == color))
}
