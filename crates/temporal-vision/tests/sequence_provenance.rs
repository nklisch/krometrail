//! Provenance-bearing `FrameSequence` wire contracts.
//!
//! Covers the authorized design boundary in
//! `.work/active/features/epic-a-grade-reliability-sequence-provenance.md`:
//! complete source provenance survives the one current wire representation,
//! every duplicate/mismatch/order/range problem is rejected at both the
//! constructor and the wire boundary, annotations are validated against the
//! effective declared range, and artifact manifests stay traceable across
//! round-trip. All enumeration is deterministic; no property dependency.

use serde_json::{Value, json};
use temporal_vision::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, BinaryMask, DeclaredGap,
    DifferenceMapLimits, DifferenceMapParameters, ErrorCode, EvidenceClass, Frame, FrameRegion,
    FrameSequence, FrequencyMode, IntegerScale, Marker, MeasurementParameters,
    NormalizationParameters, OutputHash, OwnedFrameSequence, Parameters, PixelDimensions,
    PixelFormat, PixelRect, ProcessingLimits, Rgb8, SourceProvenance, TimePalette, TimeRange,
    Timestamp, normalize_sequence, render_difference_map,
};

type Seq = OwnedFrameSequence<u8, u8, u8>;

fn frame(id: u8, timestamp: u64) -> Frame<u8, Box<[u8]>> {
    Frame::new(
        id,
        Timestamp::from_nanos(timestamp),
        PixelDimensions::new(1, 1).unwrap(),
        PixelFormat::Rgba8SrgbStraight,
        vec![0, 0, 0, 255].into_boxed_slice(),
    )
    .unwrap()
}

fn nanos(value: u64) -> Timestamp {
    Timestamp::from_nanos(value)
}

fn range(start: u64, end: u64) -> TimeRange {
    TimeRange::new(nanos(start), nanos(end)).unwrap()
}

/// Five retained source frames (ids `0..5`, timestamps `0..=40`); the decoded
/// subset keeps the frames at `indices` over the declared range.
fn provenance_sequence(indices: &[usize], source_range: (u64, u64)) -> Seq {
    let sources: Vec<Frame<u8, Box<[u8]>>> =
        (0..5).map(|id| frame(id, u64::from(id) * 10)).collect();
    let decoded: Vec<Frame<u8, Box<[u8]>>> = indices
        .iter()
        .map(|index| sources[*index].clone())
        .collect();
    FrameSequence::new(
        decoded,
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap()
    .with_source_provenance(
        (0..5).collect(),
        indices.to_vec(),
        range(source_range.0, source_range.1),
    )
    .unwrap()
}

fn canonical_wire() -> Value {
    serde_json::to_value(provenance_sequence(&[1, 3], (0, 40))).unwrap()
}

fn decode_wire(wire: Value) -> Result<Seq, serde_json::Error> {
    serde_json::from_value(wire)
}

fn replace(wire: &Value, key: &str, value: Value) -> Value {
    let mut wire = wire.clone();
    wire.as_object_mut().unwrap().insert(key.into(), value);
    wire
}

#[test]
fn round_trip_preserves_complete_source_provenance() {
    let marker = Marker::new(7_u8, nanos(20), "nav", "between decoded frames").unwrap();
    let region = FrameRegion::new(
        PixelRect::new(0, 0, 1, 1).unwrap(),
        PixelDimensions::new(1, 1).unwrap(),
    )
    .unwrap();
    let mask = BinaryMask::new(PixelDimensions::new(1, 1).unwrap(), [0x80]).unwrap();
    let sequence = FrameSequence::new(
        vec![frame(1, 10), frame(3, 30)],
        vec![marker],
        Vec::<DeclaredGap<u8>>::new(),
        Some(region),
        Some(mask),
    )
    .unwrap()
    .with_source_provenance(vec![0, 1, 2, 3, 4], vec![1, 3], range(0, 40))
    .unwrap();

    // Silent-loss reproduction: the wire must name the complete retained
    // source, the decoded subset positions, and the declared source range.
    let wire = serde_json::to_value(&sequence).unwrap();
    assert_eq!(wire["source_frame_ids"], json!([0, 1, 2, 3, 4]));
    assert_eq!(wire["source_indices"], json!([1, 3]));
    assert_eq!(wire["source_range"], json!({ "start": 0, "end": 40 }));

    let restored: Seq = serde_json::from_value(wire).unwrap();
    assert_eq!(restored, sequence);
    assert_eq!(restored.source_frame_ids(), &[0, 1, 2, 3, 4]);
    assert_eq!(restored.source_indices(), Some(&[1, 3][..]));
    assert_eq!(restored.source_frame_count(), 5);
    assert_eq!(restored.range(), range(0, 40));
    assert_eq!(restored.markers(), sequence.markers());
    assert_eq!(restored.region(), sequence.region());
    assert_eq!(restored.mask(), sequence.mask());
    // One current representation: re-encoding is byte-identical.
    assert_eq!(
        serde_json::to_string(&restored).unwrap(),
        serde_json::to_string(&sequence).unwrap()
    );
}

#[test]
fn constructor_rejects_duplicate_source_ids_at_any_position() {
    let sequence = FrameSequence::new(
        vec![frame(3, 10)],
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap();
    let attach = |ids: Vec<u8>, indices: Vec<usize>| {
        sequence
            .clone()
            .with_source_provenance(ids, indices, range(0, 10))
    };

    // Nonadjacent duplicate: the formerly accepted `[1, 3, 1]` shape.
    let error = attach(vec![1, 3, 1], vec![1]).unwrap_err();
    assert_eq!(error.code, ErrorCode::DuplicateIdentifier);
    assert_eq!(error.index, Some(2));
    // Adjacent duplicate stays rejected too.
    let error = attach(vec![3, 3], vec![0]).unwrap_err();
    assert_eq!(error.code, ErrorCode::DuplicateIdentifier);
    assert_eq!(error.index, Some(1));

    // Unique provenance still attaches, identically on repeat attachment.
    let attached = attach(vec![1, 3], vec![1]).unwrap();
    assert_eq!(attached.source_frame_ids(), &[1, 3]);
    assert_eq!(attached.source_indices(), Some(&[1][..]));
    assert!(attach(vec![1, 3], vec![1]).is_ok());
}

#[test]
fn wire_rejects_malformed_provenance() {
    let wire = canonical_wire();
    let expect_rejected = |label: &str, wire: Value, needle: &str| {
        let error = decode_wire(wire);
        assert!(
            error.is_err(),
            "{label}: malformed provenance wire was accepted"
        );
        if !needle.is_empty() {
            let message = error.unwrap_err().to_string();
            assert!(
                message.contains(needle),
                "{label}: rejected as \"{message}\""
            );
        }
    };

    // `source_frame_ids` is required: the historical provenance-dropping
    // payload is not silently repaired into the current representation.
    let mut without_ids = wire.clone();
    without_ids
        .as_object_mut()
        .unwrap()
        .remove("source_frame_ids");
    assert!(
        decode_wire(without_ids).is_err(),
        "missing source_frame_ids must be rejected"
    );
    let mut legacy = wire.clone();
    for key in ["source_frame_ids", "source_indices", "source_range"] {
        legacy.as_object_mut().unwrap().remove(key);
    }
    assert!(
        decode_wire(legacy).is_err(),
        "legacy wire without provenance must be rejected"
    );

    // `source_indices` and `source_range` are ordinary optional members:
    // omission and explicit null are the same value. On a full-source
    // payload, omitting both decodes to the plain full-source sequence.
    let full_source = serde_json::to_value(provenance_sequence(&[0, 1, 2, 3, 4], (0, 50))).unwrap();
    let plain = FrameSequence::new(
        (0..5)
            .map(|id| frame(id, u64::from(id) * 10))
            .collect::<Vec<_>>(),
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap();
    for (label, mut payload) in [
        ("omitted", full_source.clone()),
        ("nulled", full_source.clone()),
    ] {
        let object = payload.as_object_mut().unwrap();
        for key in ["source_indices", "source_range"] {
            if label == "omitted" {
                object.remove(key);
            } else {
                object.insert(key.into(), Value::Null);
            }
        }
        let decoded: Seq = serde_json::from_value(payload)
            .unwrap_or_else(|error| panic!("full-source wire with {label} members: {error}"));
        assert_eq!(
            decoded, plain,
            "{label} optional members decode as the plain full-source shape"
        );
    }

    // The same omissions on a sparse payload are rejected: by pairing while
    // one member remains, and by full-source identity when neither does.
    let sparse_without_range = {
        let mut payload = wire.clone();
        payload.as_object_mut().unwrap().remove("source_range");
        payload
    };
    expect_rejected(
        "sparse indices without range",
        sparse_without_range,
        "together",
    );
    expect_rejected(
        "sparse null indices with range",
        replace(&wire, "source_indices", Value::Null),
        "together",
    );
    let sparse_without_either = {
        let mut payload = wire.clone();
        for key in ["source_indices", "source_range"] {
            payload.as_object_mut().unwrap().remove(key);
        }
        payload
    };
    expect_rejected(
        "sparse payload with both optional members omitted",
        sparse_without_either,
        "complete source",
    );

    // Duplicate source identifiers at any distance.
    expect_rejected(
        "nonadjacent duplicate ids",
        replace(&wire, "source_frame_ids", json!([9, 1, 9, 3])),
        "unique",
    );
    expect_rejected(
        "adjacent duplicate ids",
        replace(&wire, "source_frame_ids", json!([9, 1, 3, 3])),
        "unique",
    );
    // Decoded-subset order, bounds, and identity.
    expect_rejected(
        "decreasing indices",
        replace(&wire, "source_indices", json!([3, 1])),
        "strictly increasing",
    );
    expect_rejected(
        "tied indices",
        replace(&wire, "source_indices", json!([1, 1])),
        "strictly increasing",
    );
    expect_rejected(
        "out-of-bounds index",
        replace(&wire, "source_indices", json!([1, 5])),
        "outside",
    );
    expect_rejected(
        "identity mismatch",
        replace(&wire, "source_frame_ids", json!([1, 9, 2, 3])),
        "match",
    );
    // A sequence without source indices is its own complete source: its ids
    // must be exactly the decoded ids.
    let mut replaced = wire.clone();
    replaced["source_indices"] = Value::Null;
    replaced["source_range"] = Value::Null;
    replaced["source_frame_ids"] = json!([1, 9, 2, 3]);
    expect_rejected("full-source identity replaced", replaced, "complete source");
    // The declared range must contain the decoded frame range.
    expect_rejected(
        "range excludes first decoded frame",
        replace(&wire, "source_range", json!({ "start": 11, "end": 40 })),
        "contain",
    );
    expect_rejected(
        "range excludes last decoded frame",
        replace(&wire, "source_range", json!({ "start": 0, "end": 29 })),
        "contain",
    );
    // The provenance shape is all-or-nothing.
    expect_rejected(
        "indices without range",
        replace(&wire, "source_range", Value::Null),
        "together",
    );
    expect_rejected(
        "range without indices",
        replace(&wire, "source_indices", Value::Null),
        "together",
    );
    // Unknown members are unvalidated claims riding along with the evidence.
    expect_rejected(
        "unknown field",
        replace(&wire, "analyzed_frame_ids", json!([1, 3])),
        "",
    );
}

#[test]
fn annotations_validate_against_the_declared_source_range() {
    // Before provenance exists, the constructor authority is the decoded
    // range: a marker before the decoded span is out of range.
    let early = Marker::new(1_u8, nanos(5), "nav", "before decoded span").unwrap();
    let error = FrameSequence::new(
        vec![frame(1, 10), frame(3, 30)],
        vec![early],
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::AnnotationOutOfRange);

    // A marker between the decoded span and the declared-range edge describes
    // a real source-timeline event: supported through the wire, and preserved
    // alongside a gap in the same wider-than-decoded span.
    let mut wire = canonical_wire();
    wire["markers"] =
        json!([{ "id": 7, "timestamp": 35, "kind": "nav", "label": "after decoded span" }]);
    wire["gaps"] = json!([{
        "id": 1,
        "range": { "start": 32, "end": 38 },
        "reason": "loss",
        "estimated_missing_frames": null,
    }]);
    let restored: Seq = serde_json::from_value(wire).unwrap();
    assert_eq!(restored.markers()[0].timestamp(), nanos(35));
    assert_eq!(restored.gaps()[0].range(), range(32, 38));
    let reencoded: Seq = serde_json::from_value(serde_json::to_value(&restored).unwrap()).unwrap();
    assert_eq!(reencoded, restored);

    // Beyond the declared range, annotations are out of range again.
    let mut wire = canonical_wire();
    wire["markers"] =
        json!([{ "id": 7, "timestamp": 41, "kind": "nav", "label": "past the source range" }]);
    assert!(decode_wire(wire).is_err());
    let mut wire = canonical_wire();
    wire["gaps"] = json!([{
        "id": 1,
        "range": { "start": 41, "end": 45 },
        "reason": "loss",
        "estimated_missing_frames": null,
    }]);
    assert!(decode_wire(wire).is_err());

    // The same reattachment guard covers declared gaps.
    let mut wire = canonical_wire();
    wire["gaps"] = json!([{
        "id": 1,
        "range": { "start": 32, "end": 38 },
        "reason": "loss",
        "estimated_missing_frames": null,
    }]);
    let gap_only: Seq = serde_json::from_value(wire).unwrap();
    let error = gap_only
        .clone()
        .with_source_provenance(vec![0, 1, 2, 3, 4], vec![1, 3], range(0, 31))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::AnnotationOutOfRange);
    assert!(
        gap_only
            .with_source_provenance(vec![0, 1, 2, 3, 4], vec![1, 3], range(0, 40))
            .is_ok()
    );
}

#[test]
fn complete_input_constructor_matches_the_wire() {
    // A marker and a gap between the decoded span and the declared-range edge
    // are constructible in memory through the same validating authority the
    // wire uses, and both paths agree.
    let sources: Vec<Frame<u8, Box<[u8]>>> =
        (0..5).map(|id| frame(id, u64::from(id) * 10)).collect();
    let marker = Marker::new(9_u8, nanos(35), "nav", "after decoded span").unwrap();
    let gap = DeclaredGap::new(4_u8, range(32, 38), "capture loss", None).unwrap();
    let built = FrameSequence::with_provenance(
        vec![sources[1].clone(), sources[3].clone()],
        vec![marker],
        vec![gap],
        None,
        None,
        SourceProvenance {
            frame_ids: vec![0, 1, 2, 3, 4],
            indices: Some(vec![1, 3]),
            range: Some(range(0, 40)),
        },
    )
    .unwrap();
    let wire = serde_json::to_value(&built).unwrap();
    let decoded: Seq = serde_json::from_value(wire).unwrap();
    assert_eq!(decoded, built);

    // Explicit complete-source indices are the same Some/Some shape and let
    // the caller declare a wider source time range.
    let complete = FrameSequence::with_provenance(
        (0..5).map(|id| frame(id, u64::from(id) * 10)).collect(),
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
        SourceProvenance {
            frame_ids: vec![0, 1, 2, 3, 4],
            indices: Some(vec![0, 1, 2, 3, 4]),
            range: Some(range(0, 50)),
        },
    )
    .unwrap();
    assert_eq!(complete.source_indices(), Some(&[0, 1, 2, 3, 4][..]));
    assert_eq!(complete.range(), range(0, 50));
    assert_eq!(complete.source_frame_count(), 5);

    // The full-source bundle mirrors new().
    let plain = FrameSequence::with_provenance(
        vec![sources[1].clone(), sources[3].clone()],
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
        SourceProvenance {
            frame_ids: vec![1, 3],
            indices: None,
            range: None,
        },
    )
    .unwrap();
    let via_new = FrameSequence::new(
        vec![sources[1].clone(), sources[3].clone()],
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap();
    assert_eq!(plain, via_new);

    // A full-source bundle whose ids drift from the decoded frames is
    // rejected, exactly as on the wire.
    let error = FrameSequence::with_provenance(
        vec![sources[1].clone(), sources[3].clone()],
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
        SourceProvenance {
            frame_ids: vec![1, 9, 2, 3],
            indices: None,
            range: None,
        },
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidParameter);

    // Reattaching narrower provenance that strands the broader marker fails
    // through the shared validators.
    let error = built
        .with_source_provenance(vec![0, 1, 2, 3, 4], vec![1, 3], range(0, 31))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::AnnotationOutOfRange);
}

#[test]
fn generator_output_stays_equivalent_across_sequence_round_trip() {
    // Real generator behavior, not just manifest construction: a decimated,
    // broader-annotated sequence renders identical bytes and an identical
    // manifest before and after the wire.
    let sources: Vec<Frame<u8, Box<[u8]>>> = (0..5)
        .map(|id| {
            Frame::new(
                id,
                Timestamp::from_nanos(u64::from(id) * 10),
                PixelDimensions::new(2, 1).unwrap(),
                PixelFormat::Rgba8SrgbStraight,
                vec![id * 40, 200, 120, 255, 255 - id * 40, 60, 90, 255].into_boxed_slice(),
            )
            .unwrap()
        })
        .collect();
    let marker = Marker::new(9_u8, nanos(35), "nav", "after decoded span").unwrap();
    let gap = DeclaredGap::new(4_u8, range(32, 38), "capture loss", None).unwrap();
    let sequence = FrameSequence::with_provenance(
        vec![sources[1].clone(), sources[3].clone()],
        vec![marker],
        vec![gap],
        None,
        None,
        SourceProvenance {
            frame_ids: vec![0, 1, 2, 3, 4],
            indices: Some(vec![1, 3]),
            range: Some(range(0, 40)),
        },
    )
    .unwrap();

    let render = |sequence: &Seq| {
        let normalized = normalize_sequence(
            sequence,
            NormalizationParameters::new(
                Rgb8::new(7, 9, 11),
                None,
                IntegerScale::IDENTITY,
                ProcessingLimits::default(),
            ),
        )
        .unwrap();
        render_difference_map(
            3_u8,
            sequence,
            &normalized,
            DifferenceMapParameters::new(
                0,
                FrequencyMode::Count,
                TimePalette::Spectral,
                Some(nanos(20)),
                MeasurementParameters::new(0),
                Rgb8::new(7, 9, 11),
                DifferenceMapLimits::default(),
            ),
        )
        .unwrap()
    };

    let original = render(&sequence);
    assert_eq!(original.manifest().source_frame_count(), 5);
    assert_eq!(original.manifest().analyzed_frame_count(), 2);
    assert_eq!(original.manifest().omitted_frame_count(), 3);
    assert_eq!(original.manifest().range(), range(0, 40));
    assert_eq!(original.manifest().markers()[0].timestamp(), nanos(35));
    assert_eq!(original.manifest().gaps()[0].range(), range(32, 38));

    let restored: Seq = serde_json::from_value(serde_json::to_value(&sequence).unwrap()).unwrap();
    let regenerated = render(&restored);
    assert_eq!(regenerated.manifest(), original.manifest());
    assert_eq!(regenerated.image().bytes(), original.image().bytes());
    assert_eq!(regenerated, original);
}

#[test]
fn eq_only_identifiers_need_no_hash_or_ord() {
    // The provenance contract must not accidentally require Hash or Ord on
    // caller identifier types: Eq alone constructs, round-trips, reattaches,
    // and rejects duplicates.
    #[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
    struct EqOnlyId(u8);

    let eq_frame = |id: EqOnlyId, timestamp: u64| {
        Frame::new(
            id,
            Timestamp::from_nanos(timestamp),
            PixelDimensions::new(1, 1).unwrap(),
            PixelFormat::Rgba8SrgbStraight,
            vec![0, 0, 0, 255].into_boxed_slice(),
        )
        .unwrap()
    };
    let sources: Vec<Frame<EqOnlyId, Box<[u8]>>> = (0..5)
        .map(|id| eq_frame(EqOnlyId(id), u64::from(id) * 10))
        .collect();
    let sequence = FrameSequence::with_provenance(
        vec![sources[1].clone(), sources[3].clone()],
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
        SourceProvenance {
            frame_ids: (0..5).map(EqOnlyId).collect(),
            indices: Some(vec![1, 3]),
            range: Some(range(0, 40)),
        },
    )
    .unwrap();
    let restored: FrameSequence<EqOnlyId, u8, u8, Box<[u8]>> =
        serde_json::from_value(serde_json::to_value(&sequence).unwrap()).unwrap();
    assert_eq!(restored, sequence);

    let reattached = restored
        .clone()
        .with_source_provenance((0..5).map(EqOnlyId).collect(), vec![1, 3], range(0, 40))
        .unwrap();
    assert_eq!(reattached.source_frame_count(), 5);
    let error = restored
        .with_source_provenance(
            vec![EqOnlyId(1), EqOnlyId(3), EqOnlyId(1)],
            vec![0, 2],
            range(0, 40),
        )
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::DuplicateIdentifier);
}

#[test]
fn every_decoded_subset_round_trips_with_declared_ranges() {
    // Deterministic enumeration: every nonempty decoded subset of a
    // five-frame source (31 subsets, carrying a tied-timestamp pair) against
    // four declared-range shapes — 124 round-trip cases: tight, and extended
    // past each decoded endpoint.
    let sources: Vec<Frame<u8, Box<[u8]>>> = (0..5)
        .map(|id| frame(id, [10, 10, 20, 30, 40][id as usize]))
        .collect();
    for mask in 1_u32..32 {
        let indices: Vec<usize> = (0..5).filter(|index| mask & (1 << index) != 0).collect();
        let decoded: Vec<Frame<u8, Box<[u8]>>> = indices
            .iter()
            .map(|index| sources[*index].clone())
            .collect();
        let first = sources[indices[0]].timestamp().as_nanos();
        let last = sources[*indices.last().unwrap()].timestamp().as_nanos();
        let shapes = [
            (first, last),
            (first.saturating_sub(5), last + 5),
            (first, last + 5),
            (first.saturating_sub(5), last),
        ];
        for (start, end) in shapes {
            let sequence = FrameSequence::new(
                decoded.clone(),
                Vec::<Marker<u8>>::new(),
                Vec::<DeclaredGap<u8>>::new(),
                None,
                None,
            )
            .unwrap()
            .with_source_provenance((0..5).collect(), indices.clone(), range(start, end))
            .unwrap();

            let wire = serde_json::to_value(&sequence).unwrap();
            assert_eq!(wire["source_frame_ids"], json!([0, 1, 2, 3, 4]));
            assert_eq!(wire["source_indices"], json!(indices));
            assert_eq!(wire["source_range"], json!({ "start": start, "end": end }));

            let restored: Seq = serde_json::from_value(wire)
                .unwrap_or_else(|error| panic!("subset {indices:?} range {start}..{end}: {error}"));
            assert_eq!(
                restored, sequence,
                "subset {indices:?} range {start}..{end} lost fidelity"
            );
            assert_eq!(restored.range(), range(start, end));
            assert_eq!(restored.source_frame_count(), 5);
            assert_eq!(restored.source_indices(), Some(indices.as_slice()));
            assert_eq!(restored.source_frame_ids(), &[0, 1, 2, 3, 4]);
        }
    }
}

#[test]
fn constructor_rejects_every_invalid_provenance_relationship() {
    let sequence = FrameSequence::new(
        vec![frame(1, 10), frame(3, 30)],
        Vec::<Marker<u8>>::new(),
        Vec::<DeclaredGap<u8>>::new(),
        None,
        None,
    )
    .unwrap();
    let attach = |ids: &[u8], indices: &[usize], source_range: (u64, u64)| {
        sequence.clone().with_source_provenance(
            ids.to_vec(),
            indices.to_vec(),
            range(source_range.0, source_range.1),
        )
    };
    let ids = [8_u8, 1, 9, 3];
    assert!(attach(&ids, &[1, 3], (0, 40)).is_ok());

    // Every out-of-bounds mapped index.
    assert_eq!(
        attach(&ids, &[1, 4], (0, 40)).unwrap_err().code,
        ErrorCode::InvalidParameter
    );
    // Identity mismatch: decoded frames must name their declared source ids.
    assert_eq!(
        attach(&[8, 9, 1, 3], &[1, 3], (0, 40)).unwrap_err().code,
        ErrorCode::InvalidParameter
    );
    // Every index-order violation.
    assert_eq!(
        attach(&ids, &[3, 1], (0, 40)).unwrap_err().code,
        ErrorCode::OutOfOrder
    );
    assert_eq!(
        attach(&ids, &[1, 1], (0, 40)).unwrap_err().code,
        ErrorCode::OutOfOrder
    );
    // Every duplicate position: copy each id into every other position,
    // covering adjacent and nonadjacent duplicates alike.
    for position in 0..4 {
        for copy_of in 0..4 {
            if position == copy_of {
                continue;
            }
            let mut mutated = ids;
            mutated[position] = ids[copy_of];
            let error = attach(&mutated, &[1, 3], (0, 40)).unwrap_err();
            assert_eq!(
                error.code,
                ErrorCode::DuplicateIdentifier,
                "ids {mutated:?}"
            );
            assert_eq!(error.index, Some(position.max(copy_of)));
        }
    }
    // Declared-range containment per decoded endpoint.
    assert_eq!(
        attach(&ids, &[1, 3], (11, 40)).unwrap_err().code,
        ErrorCode::InvalidParameter
    );
    assert_eq!(
        attach(&ids, &[1, 3], (0, 29)).unwrap_err().code,
        ErrorCode::InvalidParameter
    );
    // Wrong decoded-subset size.
    assert_eq!(
        attach(&ids, &[1], (0, 40)).unwrap_err().code,
        ErrorCode::InvalidParameter
    );
    assert_eq!(
        attach(&ids, &[1, 2, 3], (0, 40)).unwrap_err().code,
        ErrorCode::InvalidParameter
    );
}

#[test]
fn manifests_stay_traceable_across_sequence_round_trip() {
    // Decimated decoded subset (source ⊃ analyzed). The region-filmstrip kind
    // carries no sampling-disclosure requirement, so plain from_sequence
    // applies.
    let manifest_for = |sequence: &Seq| {
        ArtifactManifest::from_sequence(
            11_u8,
            ArtifactKind::RegionFilmstrip,
            EvidenceClass::SourceDerived,
            AlgorithmDescriptor::new("fixture-filmstrip", "1").unwrap(),
            sequence,
            sequence.frames().iter().map(Frame::id).cloned().collect(),
            Vec::new(),
            Parameters::empty(),
            PixelDimensions::new(1, 1).unwrap(),
            OutputHash::from_bytes([0xcd; 32]),
        )
        .unwrap()
    };

    let sequence = provenance_sequence(&[1, 3], (0, 40));
    let manifest = manifest_for(&sequence);
    assert_eq!(manifest.source_frame_count(), 5);
    assert_eq!(manifest.analyzed_frame_count(), 2);
    assert_eq!(manifest.omitted_frame_count(), 3);
    // serde_json::Value round-trips cannot borrow strings, so exercise the
    // output hash's string form through text, the way persisted manifests
    // travel.
    let encoded_manifest = serde_json::to_string(&manifest).unwrap();
    let decoded_manifest: ArtifactManifest<u8, u8, u8, u8> =
        serde_json::from_str(&encoded_manifest).unwrap();
    assert_eq!(decoded_manifest, manifest);

    // The round-tripped sequence regenerates a manifest traceable to the same
    // five source frames, not just its decoded subset.
    let restored: Seq = serde_json::from_value(serde_json::to_value(&sequence).unwrap()).unwrap();
    assert_eq!(manifest_for(&restored), manifest);

    // Complete-source provenance over a wider declared range: nothing is
    // decimated, and the manifest range is the declared one.
    let full = provenance_sequence(&[0, 1, 2, 3, 4], (0, 50));
    let full_manifest = manifest_for(&full);
    assert_eq!(full_manifest.omitted_frame_count(), 0);
    assert_eq!(full_manifest.range(), range(0, 50));
    let restored_full: Seq = serde_json::from_value(serde_json::to_value(&full).unwrap()).unwrap();
    assert_eq!(manifest_for(&restored_full), full_manifest);
}
