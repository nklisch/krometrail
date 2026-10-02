use png::Decoder;
use serde::{Deserialize, Serialize};
use serde_json::json;
use temporal_vision::*;

// The new render boundary must not require Display or an integer frame identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Id(u32);

fn selection() -> StoryboardSelection<Id> {
    serde_json::from_value(json!({
        "selected_frames": [
            {"frame_id": 1, "frame_index": 1, "timestamp": 1, "reasons": ["pre_anchor"]},
            {"frame_id": 4, "frame_index": 4, "timestamp": 4, "reasons": ["post_anchor", "marker_boundary"]},
            {"frame_id": 7, "frame_index": 7, "timestamp": 7, "reasons": ["gap_boundary"]},
            {"frame_id": 8, "frame_index": 8, "timestamp": 8, "reasons": ["final_frame"]}
        ],
        "omitted_anchors": [{"frame_index": 3, "reason": "first_change"}],
        "before_index": 1, "during_index": 4, "after_index": 8,
        "continuity_segment_count": 2,
        "visual_summary": {"first_change": null, "peak_baseline_change": null, "peak_adjacent_changed_area": null}
    })).unwrap()
}

fn frame(id: Id, width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> OwnedFrame<Id> {
    let timestamp = Timestamp::from_nanos(u64::from(id.0));
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&pixel(x, y));
        }
    }
    Frame::new(
        id,
        timestamp,
        PixelDimensions::new(width, height).unwrap(),
        PixelFormat::Rgba8SrgbStraight,
        pixels.into_boxed_slice(),
    )
    .unwrap()
}

fn decode(image: &EncodedImage) -> Vec<u8> {
    let mut reader = Decoder::new(image.bytes()).read_info().unwrap();
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb);
    assert_eq!(
        (info.width, info.height),
        (image.dimensions().width(), image.dimensions().height())
    );
    pixels.truncate(info.buffer_size());
    pixels
}

fn pixel_at(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 3] {
    let index = (y as usize * width as usize + x as usize) * 3;
    pixels[index..index + 3].try_into().unwrap()
}

#[test]
fn renders_original_detail_loads_only_selected_in_order_and_keeps_provenance() {
    let source = FrameSequence::new(
        (0..9)
            .map(|id| frame(Id(id), 1, 1, |_, _| [0, 0, 0, 255]))
            .collect(),
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap();
    let normalized = normalize_sequence(
        &source,
        NormalizationParameters::new(
            Rgb8::new(0, 0, 0),
            None,
            IntegerScale::IDENTITY,
            ProcessingLimits::default(),
        ),
    )
    .unwrap();
    let selected = select_storyboard_frames(
        &source,
        &normalized,
        Timestamp::from_nanos(4),
        StoryboardTileLimit::new(3).unwrap(),
        MeasurementParameters::new(0),
    )
    .unwrap();
    let mut loaded = Vec::new();
    let pattern = |x, y| {
        if (x + y) % 2 == 0 {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        }
    };
    let presented = render_storyboard_from_selection(&selected, 36, |id| {
        loaded.push(id.clone());
        Ok(frame(id.clone(), 12, 12, pattern))
    })
    .unwrap();
    assert_eq!(
        loaded,
        selected
            .selected_frames()
            .iter()
            .map(|f| f.frame_id().clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(presented.selection(), &selected);
    assert_eq!(presented.tiles().len(), selected.selected_frames().len());
    let pixels = decode(presented.image());
    for tile in presented.tiles() {
        let rect = tile.rect();
        assert_eq!((rect.width(), rect.height()), (12, 12));
        for y in 0..12 {
            for x in 0..12 {
                assert_eq!(
                    pixel_at(&pixels, 36, rect.x() + x, rect.y() + y),
                    pattern(x, y)[..3]
                );
            }
        }
    }
    // The downscaled analysis is one black pixel; the pattern can only come from originals.
    assert!(
        normalized
            .frames()
            .iter()
            .all(|f| f.linear_rgb16() == [0, 0, 0])
    );
}

#[test]
fn mixed_sizes_exact_rectangles_and_padding_match_every_montage_pixel() {
    let selected = selection();
    let presented = render_storyboard_from_selection(&selected, 36, |id| {
        let (width, height) = match id.0 {
            1 => (8, 4),
            4 => (4, 8),
            7 => (6, 6),
            8 => (2, 10),
            _ => panic!("unselected"),
        };
        Ok(frame(id.clone(), width, height, |_, _| {
            [id.0 as u8, 50, 100, 255]
        }))
    })
    .unwrap();
    assert_eq!(presented.selection(), &selected); // Includes noncontiguous source indices and omissions.
    assert_eq!(
        presented.image().dimensions(),
        PixelDimensions::new(36, 24).unwrap()
    );
    let expected = [(2, 4, 8, 4), (16, 2, 4, 8), (27, 3, 6, 6), (5, 13, 2, 10)];
    for ((tile, selected), (x, y, w, h)) in presented
        .tiles()
        .iter()
        .zip(selected.selected_frames())
        .zip(expected)
    {
        assert_eq!(tile.frame_id(), selected.frame_id());
        assert_eq!(tile.rect(), PixelRect::new(x, y, w, h).unwrap());
    }
    let pixels = decode(presented.image());
    for y in 0..24 {
        for x in 0..36 {
            let expected = presented
                .tiles()
                .iter()
                .find(|tile| {
                    let r = tile.rect();
                    x >= r.x()
                        && x < r.right_exclusive().unwrap()
                        && y >= r.y()
                        && y < r.bottom_exclusive().unwrap()
                })
                .map_or([0, 0, 0], |tile| [tile.frame_id().0 as u8, 50, 100]);
            assert_eq!(pixel_at(&pixels, 36, x, y), expected, "at {x}, {y}");
        }
    }
}

#[test]
fn resizing_preserves_aspect_and_uses_original_pixel_centers_with_straight_alpha() {
    let selected = selection();
    let presented = render_storyboard_from_selection(&selected, 19, |id| {
        Ok(frame(id.clone(), 12, 6, |x, y| {
            [x as u8 * 10, y as u8 * 20, 255, 128]
        }))
    })
    .unwrap();
    assert_eq!(
        presented.image().dimensions(),
        PixelDimensions::new(18, 12).unwrap()
    );
    let pixels = decode(presented.image());
    for tile in presented.tiles() {
        let r = tile.rect();
        assert_eq!((r.width(), r.height()), (6, 3));
        for y in 0..3 {
            for x in 0..6 {
                assert_eq!(
                    pixel_at(&pixels, 18, r.x() + x, r.y() + y),
                    [(2 * x + 1) as u8 * 5, (2 * y + 1) as u8 * 10, 128]
                );
            }
        }
    }
    let second = render_storyboard_from_selection(&selected, 19, |id| {
        Ok(frame(id.clone(), 12, 6, |x, y| {
            [x as u8 * 10, y as u8 * 20, 255, 128]
        }))
    })
    .unwrap();
    assert_eq!(presented, second);
}

#[test]
fn refuses_invalid_limits_and_wrong_identity_and_preserves_loader_errors() {
    let selected = selection();
    for (edge, code) in [
        (0, ErrorCode::InvalidParameter),
        (2, ErrorCode::ResourceLimitExceeded),
        (10_000, ErrorCode::ResourceLimitExceeded),
        (u32::MAX, ErrorCode::ResourceLimitExceeded),
    ] {
        let error = render_storyboard_from_selection(&selected, edge, |_| {
            panic!("invalid canvas must not load")
        })
        .unwrap_err();
        assert_eq!(error.code, code);
    }
    let mut loaded = Vec::new();
    let error = render_storyboard_from_selection(&selected, 36, |id| {
        loaded.push(id.clone());
        Ok(frame(Id(99), 1, 1, |_, _| [0; 4]))
    })
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::IncompatibleFrame);
    assert_eq!(error.index, Some(0));
    assert_eq!(loaded, [Id(1)]);
    let failure = VisionError {
        code: ErrorCode::InvalidMask,
        message: "caller failure".into(),
        index: Some(42),
    };
    loaded.clear();
    let error = render_storyboard_from_selection(&selected, 36, |id| {
        loaded.push(id.clone());
        if id.0 == 4 {
            Err(failure.clone())
        } else {
            Ok(frame(id.clone(), 1, 1, |_, _| [0; 4]))
        }
    })
    .unwrap_err();
    assert_eq!(error, failure);
    assert_eq!(loaded, [Id(1), Id(4)]);
}

#[test]
fn every_constructible_pixel_format_is_supported_and_unknown_formats_are_refused() {
    // PixelFormat has exactly one variant; no unsupported OwnedFrame can cross
    // its validated boundary. Keep this test exhaustive if the registry grows.
    assert!(serde_json::from_value::<PixelFormat>(json!("rgb8_linear")).is_err());
    assert_eq!(PixelFormat::ALL, &[PixelFormat::Rgba8SrgbStraight]);
    for format in PixelFormat::ALL {
        let presented = render_storyboard_from_selection(&selection(), 36, |id| {
            Frame::new(
                id.clone(),
                Timestamp::ZERO,
                PixelDimensions::new(1, 1).unwrap(),
                *format,
                Box::<[u8]>::from([13, 27, 91, 255]),
            )
        })
        .unwrap();
        let pixels = decode(presented.image());
        for tile in presented.tiles() {
            assert_eq!(
                pixel_at(&pixels, 36, tile.rect().x(), tile.rect().y()),
                [13, 27, 91]
            );
        }
    }
}

#[test]
fn one_pixel_cells_fit_one_through_twelve_selected_frames() {
    for count in 1_usize..=12 {
        let mut wire = serde_json::to_value(selection()).unwrap();
        wire["selected_frames"] = json!(
            (0..count)
                .map(|id| json!({
                    "frame_id": id, "frame_index": id, "timestamp": id,
                    "reasons": ["temporal_coverage"]
                }))
                .collect::<Vec<_>>()
        );
        wire["before_index"] = json!(0);
        wire["during_index"] = json!(0);
        wire["after_index"] = json!(count - 1);
        wire["omitted_anchors"] = json!([]);
        wire["continuity_segment_count"] = json!(1);
        let selection: StoryboardSelection<Id> = serde_json::from_value(wire).unwrap();
        let columns = count.min(3);
        let rows = count.div_ceil(columns);
        let edge = columns.max(rows) as u32;
        let presented = render_storyboard_from_selection(&selection, edge, |id| {
            let (width, height) = if id.0 % 2 == 0 { (1, 7) } else { (7, 1) };
            Ok(frame(id.clone(), width, height, |_, _| [31, 63, 127, 255]))
        })
        .unwrap();
        assert_eq!(presented.tiles().len(), count);
        let pixels = decode(presented.image());
        for (index, tile) in presented.tiles().iter().enumerate() {
            assert_eq!(
                tile.rect(),
                PixelRect::new((index % columns) as u32, (index / columns) as u32, 1, 1).unwrap()
            );
            assert_eq!(
                pixel_at(&pixels, columns as u32, tile.rect().x(), tile.rect().y()),
                [31, 63, 127]
            );
        }
    }
}

fn present() -> PresentedStoryboard<Id> {
    render_storyboard_from_selection(&selection(), 36, |id| {
        Ok(frame(id.clone(), 6, 6, |_, _| [20, 40, 60, 255]))
    })
    .unwrap()
}

fn manifest(presented: &PresentedStoryboard<Id>) -> ArtifactManifest<u8, Id, u8, u8> {
    let source = FrameSequence::new(
        (0..9)
            .map(|id| {
                Frame::new(
                    Id(id),
                    Timestamp::from_nanos(u64::from(id)),
                    PixelDimensions::new(1, 1).unwrap(),
                    PixelFormat::Rgba8SrgbStraight,
                    Box::<[u8]>::from([0, 0, 0, 255]),
                )
                .unwrap()
            })
            .collect(),
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap();
    ArtifactManifest::from_storyboard_sequence(
        1,
        ArtifactKind::Storyboard,
        EvidenceClass::SourceDerived,
        AlgorithmDescriptor::new("original-storyboard", "v1").unwrap(),
        &source,
        presented
            .selection()
            .selected_frames()
            .iter()
            .map(|f| f.frame_id().clone())
            .collect(),
        presented.selection().clone(),
        Vec::new(),
        Parameters::empty(),
        presented.image().dimensions(),
        presented.output_hash(),
    )
    .unwrap()
}

#[test]
fn artifact_manifest_carries_rectangles_and_validates_them_on_read() {
    let presented = present();
    let manifest = manifest(&presented);
    let before = serde_json::to_value(&manifest).unwrap();
    let expected_tiles = presented.tiles().to_vec();
    let artifact = presented.into_artifact(manifest).unwrap();
    assert_eq!(
        artifact.manifest().presentation_tiles(),
        Some(expected_tiles.as_slice())
    );
    let wire = serde_json::to_value(artifact.manifest()).unwrap();
    assert_eq!(
        wire["presentation_tiles"],
        serde_json::to_value(expected_tiles).unwrap()
    );
    let restored: ArtifactManifest<u8, Id, u8, u8> =
        serde_json::from_str(&wire.to_string()).unwrap();
    assert_eq!(&restored, artifact.manifest());
    let mut without_tiles = wire.clone();
    without_tiles
        .as_object_mut()
        .unwrap()
        .remove("presentation_tiles");
    assert_eq!(without_tiles, before); // Adding presentation preserves all caller provenance.
    for change in 0..6 {
        let mut invalid = wire.clone();
        match change {
            0 => invalid["presentation_tiles"][0]["frame_id"] = json!(99),
            1 => invalid["presentation_tiles"][0]["rect"]["x"] = json!(36),
            2 => invalid["presentation_tiles"][0]["rect"]["width"] = json!(0),
            3 => {
                invalid["presentation_tiles"][1]["rect"] =
                    invalid["presentation_tiles"][0]["rect"].clone()
            }
            4 => {
                invalid["presentation_tiles"].as_array_mut().unwrap().pop();
            }
            5 => invalid["presentation_tiles"][0]["rect"]["unknown"] = json!(1),
            _ => unreachable!(),
        }
        assert!(
            serde_json::from_str::<ArtifactManifest<u8, Id, u8, u8>>(&invalid.to_string()).is_err()
        );
    }
}

#[test]
fn attaching_a_manifest_requires_exact_image_and_selection() {
    for field in ["output_hash", "output_dimensions", "storyboard_selection"] {
        let presented = present();
        let mut wire = serde_json::to_value(manifest(&presented)).unwrap();
        match field {
            "output_hash" => wire[field] = json!("00".repeat(32)),
            "output_dimensions" => wire[field]["width"] = json!(35),
            "storyboard_selection" => wire[field]["omitted_anchors"] = json!([]),
            _ => unreachable!(),
        }
        let manifest =
            serde_json::from_str::<ArtifactManifest<u8, Id, u8, u8>>(&wire.to_string()).unwrap();
        assert_eq!(
            presented.into_artifact(manifest).unwrap_err().code,
            ErrorCode::InvalidManifest
        );
    }
}
