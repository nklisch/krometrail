use std::{collections::BTreeMap, num::NonZeroU64};

use serde::{Deserialize, Serialize};
use temporal_vision::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, BinaryMask, DeclaredGap, EvidenceClass,
    FiniteNumber, Frame, FrameRegion, FrameSequence, Marker, NormalizationKind, NormalizationStep,
    OutputHash, ParameterValue, Parameters, PixelDimensions, PixelFormat, PixelRect, TimeRange,
    Timestamp, generator_descriptor,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct FrameId([u8; 16]);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MarkerId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct GapId(String);
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct ArtifactId([u8; 16]);

fn dimensions() -> PixelDimensions {
    PixelDimensions::new(2, 2).unwrap()
}

fn frame(id: u8, timestamp: u64) -> Frame<FrameId, Box<[u8]>> {
    Frame::new(
        FrameId([id; 16]),
        Timestamp::from_nanos(timestamp),
        dimensions(),
        PixelFormat::Rgba8SrgbStraight,
        vec![id; 16].into_boxed_slice(),
    )
    .unwrap()
}

#[test]
fn browser_free_consumer_builds_deterministic_complete_manifest() {
    let pixels_a = [1_u8; 16];
    let pixels_b = [2_u8; 16];
    let pixels_c = [3_u8; 16];
    let frames = vec![
        Frame::new(
            FrameId([1; 16]),
            Timestamp::from_nanos(10),
            dimensions(),
            PixelFormat::Rgba8SrgbStraight,
            pixels_a.as_slice(),
        )
        .unwrap(),
        Frame::new(
            FrameId([2; 16]),
            Timestamp::from_nanos(10),
            dimensions(),
            PixelFormat::Rgba8SrgbStraight,
            pixels_b.as_slice(),
        )
        .unwrap(),
        Frame::new(
            FrameId([3; 16]),
            Timestamp::from_nanos(30),
            dimensions(),
            PixelFormat::Rgba8SrgbStraight,
            pixels_c.as_slice(),
        )
        .unwrap(),
    ];
    let markers = vec![
        Marker::new(
            MarkerId("marker-b".into()),
            Timestamp::from_nanos(10),
            "caller.action",
            "second by id, first by declaration",
        )
        .unwrap(),
        Marker::new(
            MarkerId("marker-a".into()),
            Timestamp::from_nanos(10),
            "caller.state",
            "first by id, second by declaration",
        )
        .unwrap(),
    ];
    let gaps = vec![
        DeclaredGap::new(
            GapId("known-loss".into()),
            TimeRange::new(Timestamp::from_nanos(10), Timestamp::from_nanos(30)).unwrap(),
            "caller declared capture loss",
            NonZeroU64::new(1),
        )
        .unwrap(),
    ];
    let region = FrameRegion::new(PixelRect::new(0, 0, 1, 2).unwrap(), dimensions()).unwrap();
    let mask = BinaryMask::new(dimensions(), [0xa0]).unwrap();
    let sequence = FrameSequence::new(frames, markers, gaps, Some(region), Some(mask)).unwrap();

    assert_eq!(sequence.frames()[0].pixels().as_ptr(), pixels_a.as_ptr());
    assert_eq!(sequence.frames()[0].id(), &FrameId([1; 16]));
    assert_eq!(sequence.frames()[1].id(), &FrameId([2; 16]));
    assert_eq!(sequence.markers()[0].id(), &MarkerId("marker-b".into()));

    let owned = sequence.to_owned();
    assert_eq!(owned.frames()[0].pixels(), pixels_a);
    assert_ne!(owned.frames()[0].pixels().as_ptr(), pixels_a.as_ptr());

    let mut nested = BTreeMap::new();
    nested.insert("enabled".into(), ParameterValue::Bool(true));
    let mut parameter_values = BTreeMap::new();
    parameter_values.insert(
        "alpha".into(),
        ParameterValue::Number(FiniteNumber::new(-0.0).unwrap()),
    );
    parameter_values.insert("options".into(), ParameterValue::Object(nested));
    let parameters = Parameters::new(parameter_values).unwrap();
    let normalization = vec![
        NormalizationStep::new(
            NormalizationKind::Thresholding,
            "threshold-v1",
            Parameters::empty(),
        )
        .unwrap(),
    ];
    let manifest = ArtifactManifest::from_sequence(
        ArtifactId([9; 16]),
        ArtifactKind::Storyboard,
        EvidenceClass::SourceDerived,
        AlgorithmDescriptor::new("synthetic-storyboard", "1.0.0").unwrap(),
        &sequence,
        vec![FrameId([1; 16]), FrameId([3; 16])],
        normalization,
        parameters,
        PixelDimensions::new(4, 4).unwrap(),
        OutputHash::from_bytes([0xab; 32]),
    )
    .unwrap();

    assert_eq!(manifest.source_frame_count(), 3);
    assert_eq!(manifest.omitted_frame_count(), 1);
    assert_eq!(manifest.markers()[0].id(), &MarkerId("marker-b".into()));
    assert_eq!(manifest.mask().unwrap().bits(), &[0xa0]);

    let first = serde_json::to_vec(&manifest).unwrap();
    let second = serde_json::to_vec(&manifest).unwrap();
    assert_eq!(first, second);
    let decoded: ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId> =
        serde_json::from_slice(&first).unwrap();
    assert_eq!(decoded, manifest);
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), first);
}

#[test]
fn descriptor_version_isolates_storyboard_and_orientation_cache_identity() {
    assert_eq!(
        generator_descriptor(ArtifactKind::Storyboard).version,
        "1.1.0"
    );
    assert_eq!(
        generator_descriptor(ArtifactKind::BeforeDuringAfter).version,
        "1.1.0"
    );
    assert_eq!(
        generator_descriptor(ArtifactKind::DifferenceMap).version,
        "v1"
    );
    assert_eq!(
        generator_descriptor(ArtifactKind::RegionFilmstrip).version,
        "1.0.0"
    );
    assert_eq!(
        generator_descriptor(ArtifactKind::MotionHistory).version,
        "1.0.0"
    );
}

#[test]
fn malformed_sequence_inputs_fail_instead_of_being_repaired() {
    assert!(
        Frame::new(
            FrameId([1; 16]),
            Timestamp::ZERO,
            dimensions(),
            PixelFormat::Rgba8SrgbStraight,
            vec![0_u8; 15].into_boxed_slice(),
        )
        .is_err()
    );

    assert!(
        FrameSequence::new(
            vec![frame(1, 1), frame(1, 2)],
            Vec::<Marker<MarkerId>>::new(),
            Vec::<DeclaredGap<GapId>>::new(),
            None,
            None,
        )
        .is_err()
    );
    assert!(
        FrameSequence::new(
            vec![frame(1, 2), frame(2, 1)],
            Vec::<Marker<MarkerId>>::new(),
            Vec::<DeclaredGap<GapId>>::new(),
            None,
            None,
        )
        .is_err()
    );

    let markers = vec![
        Marker::new(MarkerId("later".into()), Timestamp::from_nanos(2), "k", "l").unwrap(),
        Marker::new(
            MarkerId("earlier".into()),
            Timestamp::from_nanos(1),
            "k",
            "l",
        )
        .unwrap(),
    ];
    assert!(
        FrameSequence::new(
            vec![frame(1, 1), frame(2, 2)],
            markers,
            Vec::<DeclaredGap<GapId>>::new(),
            None,
            None,
        )
        .is_err()
    );

    let gaps = vec![
        DeclaredGap::new(
            GapId("one".into()),
            TimeRange::new(Timestamp::from_nanos(1), Timestamp::from_nanos(3)).unwrap(),
            "loss",
            None,
        )
        .unwrap(),
        DeclaredGap::new(
            GapId("two".into()),
            TimeRange::new(Timestamp::from_nanos(2), Timestamp::from_nanos(3)).unwrap(),
            "loss",
            None,
        )
        .unwrap(),
    ];
    assert!(
        FrameSequence::new(
            vec![frame(1, 1), frame(2, 3)],
            Vec::<Marker<MarkerId>>::new(),
            gaps,
            None,
            None,
        )
        .is_err()
    );

    assert!(PixelRect::new(u32::MAX, 0, 2, 1).is_err());
    assert!(BinaryMask::new(PixelDimensions::new(3, 3).unwrap(), [0xff, 0x01]).is_err());
    assert!(FiniteNumber::new(f64::INFINITY).is_err());
}

#[test]
fn selected_order_and_persisted_manifest_counts_are_validated() {
    let sequence = FrameSequence::new(
        vec![frame(1, 1), frame(2, 2), frame(3, 3)],
        Vec::<Marker<MarkerId>>::new(),
        Vec::<DeclaredGap<GapId>>::new(),
        None,
        None,
    )
    .unwrap();
    assert!(
        ArtifactManifest::from_sequence(
            ArtifactId([9; 16]),
            ArtifactKind::DifferenceMap,
            EvidenceClass::SourceDerived,
            AlgorithmDescriptor::new("difference", "1").unwrap(),
            &sequence,
            vec![FrameId([2; 16]), FrameId([1; 16])],
            vec![],
            Parameters::empty(),
            dimensions(),
            OutputHash::from_bytes([0; 32]),
        )
        .is_err()
    );

    let manifest = ArtifactManifest::from_sequence(
        ArtifactId([9; 16]),
        ArtifactKind::DifferenceMap,
        EvidenceClass::SourceDerived,
        AlgorithmDescriptor::new("difference", "1").unwrap(),
        &sequence,
        vec![FrameId([1; 16])],
        vec![],
        Parameters::empty(),
        dimensions(),
        OutputHash::from_bytes([0; 32]),
    )
    .unwrap();
    let mut malformed = serde_json::to_value(manifest).unwrap();
    malformed["source_frame_count"] = serde_json::json!(99);
    assert!(
        serde_json::from_value::<ArtifactManifest<ArtifactId, FrameId, MarkerId, GapId>>(malformed)
            .is_err()
    );
}
