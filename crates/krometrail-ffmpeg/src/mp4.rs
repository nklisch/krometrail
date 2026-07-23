use std::sync::Arc;

use krometrail_core::{MAX_VIDEO_PRESENTATION_SEGMENTS, PixelDimensions};
use sha2::{Digest, Sha256};

use crate::error::{AdapterFailure, AdapterFailureKind, AdapterFailureStage};
use crate::{control::OperationControl, policy::FFMPEG_TIMEBASE_HZ};

const MAX_BOX_COUNT: usize = 1_024;
const MAX_BOX_DEPTH: usize = 12;
const MAX_MP4_SAMPLE_COUNT: usize = MAX_VIDEO_PRESENTATION_SEGMENTS + 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mp4Check {
    OutputBytes,
    ContainerStructure,
    MovieDuration,
    VideoTrackCount,
    AudioTrackCount,
    VideoCodec,
    VideoDimensions,
    TrackTimescale,
    TrackDuration,
    SampleCount,
    SampleDurations,
}

impl Mp4Check {
    pub const fn name(self) -> &'static str {
        match self {
            Self::OutputBytes => "output_bytes",
            Self::ContainerStructure => "container_structure",
            Self::MovieDuration => "movie_duration",
            Self::VideoTrackCount => "video_track_count",
            Self::AudioTrackCount => "audio_track_count",
            Self::VideoCodec => "video_codec",
            Self::VideoDimensions => "video_dimensions",
            Self::TrackTimescale => "track_timescale",
            Self::TrackDuration => "track_duration",
            Self::SampleCount => "sample_count",
            Self::SampleDurations => "sample_durations",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Mp4Property {
    Count(u64),
    Micros(u64),
    Codec([u8; 4]),
    Dimensions { width: u32, height: u32 },
    Sample { index: u16, micros: u64 },
    Bytes(u64),
    Absent,
}

impl std::fmt::Display for Mp4Property {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Count(value) => write!(formatter, "count={value}"),
            Self::Micros(value) => write!(formatter, "micros={value}"),
            Self::Codec(codec) => {
                write!(formatter, "codec=")?;
                if codec.iter().all(|byte| byte.is_ascii_graphic()) {
                    for byte in codec {
                        write!(formatter, "{}", char::from(*byte))?;
                    }
                } else {
                    write!(formatter, "0x")?;
                    for byte in codec {
                        write!(formatter, "{byte:02x}")?;
                    }
                }
                Ok(())
            }
            Self::Dimensions { width, height } => write!(formatter, "{width}x{height}"),
            Self::Sample { index, micros } => write!(formatter, "sample[{index}]={micros}"),
            Self::Bytes(value) => write!(formatter, "bytes={value}"),
            Self::Absent => formatter.write_str("absent"),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OutputValidationDetail {
    pub check: Mp4Check,
    pub expected: Mp4Property,
    pub observed: Mp4Property,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExpectedMp4 {
    pub(crate) canvas: PixelDimensions,
    pub(crate) presentation_duration_micros: u64,
    pub(crate) sample_durations_micros: Arc<[u64]>,
    pub(crate) max_bytes: u64,
}

pub(crate) struct ValidatedMp4 {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) output_hash: temporal_vision::OutputHash,
}

pub(crate) fn validate_mp4(
    bytes: Arc<[u8]>,
    expected: ExpectedMp4,
    control: &OperationControl,
) -> Result<ValidatedMp4, AdapterFailure> {
    control.check(AdapterFailureStage::OutputValidation)?;
    let observed_bytes = bytes.len() as u64;
    if observed_bytes == 0 || observed_bytes > expected.max_bytes {
        let kind = if observed_bytes > expected.max_bytes {
            AdapterFailureKind::OutputOverflow
        } else {
            AdapterFailureKind::InvalidOutput
        };
        return Err(failure(kind)
            .with_observed_bytes(observed_bytes)
            .with_output_check(OutputValidationDetail {
                check: Mp4Check::OutputBytes,
                expected: Mp4Property::Bytes(if observed_bytes == 0 {
                    1
                } else {
                    expected.max_bytes
                }),
                observed: Mp4Property::Bytes(observed_bytes),
            }));
    }

    let mut count = 0;
    let mut top = BoxCursor::new(&bytes, 0)?;
    let mut ftyp_count = 0;
    let mut moov_count = 0;
    let mut media_bytes = 0_u64;
    let mut movie = None;
    while let Some(item) = top.next_box(&mut count, control)? {
        match &item.kind {
            b"ftyp" => {
                ftyp_count += 1;
                if item.payload.len() < 8 {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
            }
            b"moov" => {
                moov_count += 1;
                movie = Some(parse_moov(
                    item.payload,
                    item.depth + 1,
                    &mut count,
                    control,
                )?);
            }
            b"mdat" => {
                media_bytes = media_bytes
                    .checked_add(item.payload.len() as u64)
                    .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
            }
            _ => {}
        }
    }
    if ftyp_count != 1 {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::ContainerStructure,
            expected: Mp4Property::Count(1),
            observed: Mp4Property::Count(ftyp_count),
        }));
    }
    if moov_count != 1 {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::ContainerStructure,
            expected: Mp4Property::Count(1),
            observed: Mp4Property::Count(moov_count),
        }));
    }
    if media_bytes == 0 {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::ContainerStructure,
            expected: Mp4Property::Bytes(1),
            observed: Mp4Property::Bytes(0),
        }));
    }

    let movie = movie.ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    validate_duration(
        movie.timescale,
        movie.duration,
        expected.presentation_duration_micros,
        Mp4Check::MovieDuration,
    )
    .map_err(validation_failure)?;
    if movie.video_tracks != 1 {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::VideoTrackCount,
            expected: Mp4Property::Count(1),
            observed: Mp4Property::Count(movie.video_tracks as u64),
        }));
    }
    if movie.audio_tracks != 0 {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::AudioTrackCount,
            expected: Mp4Property::Count(0),
            observed: Mp4Property::Count(movie.audio_tracks as u64),
        }));
    }
    if !movie
        .video_codec
        .is_some_and(|codec| codec == *b"avc1" || codec == *b"avc3")
    {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::VideoCodec,
            expected: Mp4Property::Codec(*b"avc1"),
            observed: movie
                .video_codec
                .map(Mp4Property::Codec)
                .unwrap_or(Mp4Property::Absent),
        }));
    }
    if movie.video_dimensions != Some(expected.canvas) {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::VideoDimensions,
            expected: Mp4Property::Dimensions {
                width: expected.canvas.width(),
                height: expected.canvas.height(),
            },
            observed: movie
                .video_dimensions
                .map(|dimensions| Mp4Property::Dimensions {
                    width: dimensions.width(),
                    height: dimensions.height(),
                })
                .unwrap_or(Mp4Property::Absent),
        }));
    }
    let observed_sample_durations = movie
        .video_sample_durations
        .as_deref()
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    match_sample_durations(observed_sample_durations, &expected.sample_durations_micros)
        .map_err(validation_failure)?;
    let (track_timescale, track_duration) = movie.video_duration.ok_or_else(|| {
        validation_failure(OutputValidationDetail {
            check: Mp4Check::TrackDuration,
            expected: Mp4Property::Micros(expected.presentation_duration_micros),
            observed: Mp4Property::Absent,
        })
    })?;
    validate_duration(
        track_timescale,
        track_duration,
        expected.presentation_duration_micros,
        Mp4Check::TrackDuration,
    )
    .map_err(validation_failure)?;
    if track_timescale != FFMPEG_TIMEBASE_HZ {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::TrackTimescale,
            expected: Mp4Property::Count(u64::from(FFMPEG_TIMEBASE_HZ)),
            observed: Mp4Property::Count(u64::from(track_timescale)),
        }));
    }

    let mut digest = Sha256::new();
    for chunk in bytes.chunks(64 * 1024) {
        control.check(AdapterFailureStage::OutputValidation)?;
        digest.update(chunk);
    }
    let output_hash = temporal_vision::OutputHash::from_bytes(digest.finalize().into());
    Ok(ValidatedMp4 { bytes, output_hash })
}

struct MovieInfo {
    timescale: u32,
    duration: u64,
    video_tracks: usize,
    audio_tracks: usize,
    video_codec: Option<[u8; 4]>,
    video_dimensions: Option<PixelDimensions>,
    video_duration: Option<(u32, u64)>,
    video_sample_durations: Option<Arc<[u64]>>,
}

fn parse_moov(
    data: &[u8],
    depth: usize,
    count: &mut usize,
    control: &OperationControl,
) -> Result<MovieInfo, AdapterFailure> {
    let mut cursor = BoxCursor::new(data, depth)?;
    let mut movie_duration = None;
    let mut video_tracks = 0;
    let mut audio_tracks = 0;
    let mut video_codec = None;
    let mut video_dimensions = None;
    let mut video_duration = None;
    let mut video_sample_durations = None;
    while let Some(item) = cursor.next_box(count, control)? {
        match &item.kind {
            b"mvhd" => {
                if movie_duration.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                movie_duration = Some(parse_media_header(item.payload)?);
            }
            b"trak" => {
                let track = parse_track(item.payload, item.depth + 1, count, control)?;
                match &track.handler {
                    b"vide" => {
                        video_tracks += 1;
                        if video_tracks > 1 {
                            return Err(failure(AdapterFailureKind::InvalidOutput));
                        }
                        video_codec = track.codec;
                        video_dimensions = track.dimensions;
                        video_duration = track.duration;
                        video_sample_durations = track.sample_durations;
                    }
                    b"soun" => audio_tracks += 1,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let (timescale, duration) = movie_duration
        .filter(|(timescale, duration)| *timescale != 0 && *duration != 0)
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    Ok(MovieInfo {
        timescale,
        duration,
        video_tracks,
        audio_tracks,
        video_codec,
        video_dimensions,
        video_duration,
        video_sample_durations,
    })
}

struct TrackInfo {
    handler: [u8; 4],
    codec: Option<[u8; 4]>,
    dimensions: Option<PixelDimensions>,
    duration: Option<(u32, u64)>,
    sample_durations: Option<Arc<[u64]>>,
}

fn parse_track(
    data: &[u8],
    depth: usize,
    count: &mut usize,
    control: &OperationControl,
) -> Result<TrackInfo, AdapterFailure> {
    let mut cursor = BoxCursor::new(data, depth)?;
    let mut track_dimensions = None;
    let mut media = None;
    while let Some(item) = cursor.next_box(count, control)? {
        match &item.kind {
            b"tkhd" => {
                if track_dimensions.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                track_dimensions = parse_track_dimensions(item.payload)?;
            }
            b"mdia" => {
                if media.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                media = Some(parse_media(item.payload, item.depth + 1, count, control)?);
            }
            _ => {}
        }
    }
    let media = media.ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    if media.handler == *b"vide" && track_dimensions != media.dimensions {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    Ok(TrackInfo {
        handler: media.handler,
        codec: media.codec,
        dimensions: media.dimensions,
        duration: Some(media.duration),
        sample_durations: media.sample_durations,
    })
}

struct MediaInfo {
    handler: [u8; 4],
    codec: Option<[u8; 4]>,
    dimensions: Option<PixelDimensions>,
    duration: (u32, u64),
    sample_durations: Option<Arc<[u64]>>,
}

fn parse_media(
    data: &[u8],
    depth: usize,
    count: &mut usize,
    control: &OperationControl,
) -> Result<MediaInfo, AdapterFailure> {
    let mut cursor = BoxCursor::new(data, depth)?;
    let mut duration = None;
    let mut handler = None;
    let mut sample = None;
    while let Some(item) = cursor.next_box(count, control)? {
        match &item.kind {
            b"mdhd" => {
                if duration.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                duration = Some(parse_media_header(item.payload)?);
            }
            b"hdlr" => {
                if handler.is_some() || item.payload.len() < 12 {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                handler = Some(item.payload[8..12].try_into().expect("four-byte slice"));
            }
            b"minf" => {
                if sample.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                sample = parse_minf(item.payload, item.depth + 1, count, control)?;
            }
            _ => {}
        }
    }
    let duration = duration
        .filter(|(timescale, duration)| *timescale != 0 && *duration != 0)
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    let handler = handler.ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    let sample = sample.unwrap_or_default();
    let (codec, dimensions) = (sample.codec, sample.dimensions);
    if handler == *b"vide" && (codec.is_none() || dimensions.is_none()) {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    Ok(MediaInfo {
        handler,
        codec,
        dimensions,
        duration,
        sample_durations: sample.durations,
    })
}

#[derive(Default)]
struct SampleTable {
    codec: Option<[u8; 4]>,
    dimensions: Option<PixelDimensions>,
    durations: Option<Arc<[u64]>>,
}

fn parse_minf(
    data: &[u8],
    depth: usize,
    count: &mut usize,
    control: &OperationControl,
) -> Result<Option<SampleTable>, AdapterFailure> {
    let mut cursor = BoxCursor::new(data, depth)?;
    let mut sample = None;
    while let Some(item) = cursor.next_box(count, control)? {
        if item.kind == *b"stbl" {
            if sample.is_some() {
                return Err(failure(AdapterFailureKind::InvalidOutput));
            }
            sample = Some(parse_stbl(item.payload, item.depth + 1, count, control)?);
        }
    }
    Ok(sample)
}

fn parse_stbl(
    data: &[u8],
    depth: usize,
    count: &mut usize,
    control: &OperationControl,
) -> Result<SampleTable, AdapterFailure> {
    let mut cursor = BoxCursor::new(data, depth)?;
    let mut description = None;
    let mut durations = None;
    let mut sample_count = None;
    let mut composition_count = None;
    while let Some(item) = cursor.next_box(count, control)? {
        match &item.kind {
            b"stsd" => {
                if description.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                description = Some(parse_stsd(item.payload, item.depth + 1, count, control)?);
            }
            b"stts" => {
                if durations.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                durations = Some(parse_stts(item.payload, control)?);
            }
            b"stsz" => {
                if sample_count.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                sample_count = Some(parse_stsz(item.payload)?);
            }
            b"ctts" => {
                if composition_count.is_some() {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                composition_count = Some(parse_ctts(item.payload, control)?);
            }
            _ => {}
        }
    }
    let (codec, dimensions) =
        description.ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    let durations = durations.ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    let sample_count = sample_count.ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    if durations.len() != sample_count {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::SampleCount,
            expected: Mp4Property::Count(sample_count as u64),
            observed: Mp4Property::Count(durations.len() as u64),
        }));
    }
    if let Some(composition_count) = composition_count
        && composition_count != sample_count
    {
        return Err(validation_failure(OutputValidationDetail {
            check: Mp4Check::SampleCount,
            expected: Mp4Property::Count(sample_count as u64),
            observed: Mp4Property::Count(composition_count as u64),
        }));
    }
    Ok(SampleTable {
        codec,
        dimensions,
        durations: Some(durations),
    })
}

fn parse_stts(data: &[u8], control: &OperationControl) -> Result<Arc<[u64]>, AdapterFailure> {
    if data.len() < 8 || data[0..4] != [0; 4] {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let entries = usize::try_from(read_u32(data, 4)?)
        .map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
    if data.len()
        != 8_usize
            .checked_add(
                entries
                    .checked_mul(8)
                    .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?,
            )
            .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?
    {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let mut durations = Vec::new();
    for index in 0..entries {
        control.check(AdapterFailureStage::OutputValidation)?;
        let offset = 8 + index * 8;
        let run = usize::try_from(read_u32(data, offset)?)
            .map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
        let delta = u64::from(read_u32(data, offset + 4)?);
        if run == 0 || durations.len().saturating_add(run) > MAX_MP4_SAMPLE_COUNT {
            return Err(failure(AdapterFailureKind::InvalidOutput));
        }
        durations.extend(std::iter::repeat_n(delta, run));
    }
    if durations.is_empty() {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    if durations[..durations.len() - 1].contains(&0) {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    Ok(durations.into())
}

fn parse_stsz(data: &[u8]) -> Result<usize, AdapterFailure> {
    if data.len() < 12 || data[0..4] != [0; 4] {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let fixed_size = read_u32(data, 4)?;
    let count = usize::try_from(read_u32(data, 8)?)
        .map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
    if count == 0 || count > MAX_MP4_SAMPLE_COUNT {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let expected = if fixed_size == 0 {
        12_usize
            .checked_add(
                count
                    .checked_mul(4)
                    .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?,
            )
            .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?
    } else {
        12
    };
    if data.len() != expected {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    Ok(count)
}

fn parse_ctts(data: &[u8], control: &OperationControl) -> Result<usize, AdapterFailure> {
    if data.len() < 8 || !matches!(data[0], 0 | 1) || data[1..4] != [0; 3] {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let entries = usize::try_from(read_u32(data, 4)?)
        .map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
    if data.len()
        != 8_usize
            .checked_add(
                entries
                    .checked_mul(8)
                    .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?,
            )
            .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?
    {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let mut total = 0_usize;
    for index in 0..entries {
        control.check(AdapterFailureStage::OutputValidation)?;
        let offset = 8 + index * 8;
        let run = usize::try_from(read_u32(data, offset)?)
            .map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
        let composition_offset = read_u32(data, offset + 4)?;
        if run == 0 || composition_offset != 0 {
            return Err(failure(AdapterFailureKind::InvalidOutput));
        }
        total = total
            .checked_add(run)
            .filter(|value| *value <= MAX_MP4_SAMPLE_COUNT)
            .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    }
    if total == 0 {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    Ok(total)
}

fn parse_stsd(
    data: &[u8],
    depth: usize,
    count: &mut usize,
    control: &OperationControl,
) -> Result<(Option<[u8; 4]>, Option<PixelDimensions>), AdapterFailure> {
    if data.len() < 8 || read_u32(data, 4)? != 1 {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let mut entries = BoxCursor::new(&data[8..], depth)?;
    let entry = entries
        .next_box(count, control)?
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    if entries.next_box(count, control)?.is_some() || entry.payload.len() < 78 {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let width = u32::from(read_u16(entry.payload, 24)?);
    let height = u32::from(read_u16(entry.payload, 26)?);
    let dimensions = PixelDimensions::new(width, height)
        .map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
    let codec = entry.kind;
    if codec != *b"avc1" && codec != *b"avc3" {
        return Ok((Some(codec), Some(dimensions)));
    }
    let mut extensions = BoxCursor::new(&entry.payload[78..], entry.depth + 1)?;
    let mut avcc = 0;
    while let Some(extension) = extensions.next_box(count, control)? {
        if extension.kind == *b"avcC" && !extension.payload.is_empty() {
            avcc += 1;
        }
    }
    if avcc != 1 {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    Ok((Some(codec), Some(dimensions)))
}

fn parse_media_header(data: &[u8]) -> Result<(u32, u64), AdapterFailure> {
    let version = *data
        .first()
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    match version {
        0 if data.len() >= 20 => Ok((read_u32(data, 12)?, u64::from(read_u32(data, 16)?))),
        1 if data.len() >= 32 => Ok((read_u32(data, 20)?, read_u64(data, 24)?)),
        _ => Err(failure(AdapterFailureKind::InvalidOutput)),
    }
}

fn parse_track_dimensions(data: &[u8]) -> Result<Option<PixelDimensions>, AdapterFailure> {
    if data.len() < 8 {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let width_fixed = read_u32(data, data.len() - 8)?;
    let height_fixed = read_u32(data, data.len() - 4)?;
    if width_fixed & 0xffff != 0 || height_fixed & 0xffff != 0 {
        return Err(failure(AdapterFailureKind::InvalidOutput));
    }
    let width = width_fixed >> 16;
    let height = height_fixed >> 16;
    PixelDimensions::new(width, height)
        .map(Some)
        .map_err(|_| failure(AdapterFailureKind::InvalidOutput))
}

fn validate_duration(
    timescale: u32,
    duration: u64,
    expected_micros: u64,
    check: Mp4Check,
) -> Result<(), OutputValidationDetail> {
    let observed_micros = if timescale == 0 {
        0
    } else {
        u64::try_from(
            u128::from(duration)
                .saturating_mul(1_000_000)
                .checked_div(u128::from(timescale))
                .unwrap_or_default(),
        )
        .unwrap_or(u64::MAX)
    };
    let detail = || OutputValidationDetail {
        check,
        expected: Mp4Property::Micros(expected_micros),
        observed: Mp4Property::Micros(observed_micros),
    };
    if timescale == 0 || duration == 0 || expected_micros == 0 {
        return Err(detail());
    }
    const MICROS_PER_SECOND: u128 = 1_000_000;
    let actual = u128::from(duration) * MICROS_PER_SECOND;
    let expected = u128::from(expected_micros) * u128::from(timescale);
    if actual.abs_diff(expected) > MICROS_PER_SECOND {
        return Err(detail());
    }
    Ok(())
}

fn match_sample_durations(
    observed: &[u64],
    expected: &[u64],
) -> Result<(), OutputValidationDetail> {
    if observed.len() != expected.len() {
        return Err(OutputValidationDetail {
            check: Mp4Check::SampleDurations,
            expected: Mp4Property::Count(expected.len() as u64),
            observed: Mp4Property::Count(observed.len() as u64),
        });
    }
    for (index, (&observed_micros, &expected_micros)) in observed.iter().zip(expected).enumerate() {
        let terminal = index + 1 == expected.len();
        if observed_micros != expected_micros && !(terminal && observed_micros == 0) {
            return Err(OutputValidationDetail {
                check: Mp4Check::SampleDurations,
                expected: Mp4Property::Sample {
                    index: u16::try_from(index).expect("bounded MP4 sample index"),
                    micros: expected_micros,
                },
                observed: Mp4Property::Sample {
                    index: u16::try_from(index).expect("bounded MP4 sample index"),
                    micros: observed_micros,
                },
            });
        }
    }
    Ok(())
}

struct Mp4Box<'a> {
    kind: [u8; 4],
    payload: &'a [u8],
    depth: usize,
}

struct BoxCursor<'a> {
    data: &'a [u8],
    position: usize,
    depth: usize,
}

impl<'a> BoxCursor<'a> {
    fn new(data: &'a [u8], depth: usize) -> Result<Self, AdapterFailure> {
        if depth > MAX_BOX_DEPTH {
            return Err(failure(AdapterFailureKind::InvalidOutput));
        }
        Ok(Self {
            data,
            position: 0,
            depth,
        })
    }

    fn next_box(
        &mut self,
        count: &mut usize,
        control: &OperationControl,
    ) -> Result<Option<Mp4Box<'a>>, AdapterFailure> {
        control.check(AdapterFailureStage::OutputValidation)?;
        if self.position == self.data.len() {
            return Ok(None);
        }
        *count = count
            .checked_add(1)
            .filter(|count| *count <= MAX_BOX_COUNT)
            .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
        let remaining = &self.data[self.position..];
        if remaining.len() < 8 {
            return Err(failure(AdapterFailureKind::InvalidOutput));
        }
        let size32 = read_u32(remaining, 0)?;
        let kind = remaining[4..8].try_into().expect("four-byte slice");
        let (size, header) = match size32 {
            0 => return Err(failure(AdapterFailureKind::InvalidOutput)),
            1 => {
                if remaining.len() < 16 {
                    return Err(failure(AdapterFailureKind::InvalidOutput));
                }
                (read_u64(remaining, 8)?, 16_usize)
            }
            value => (u64::from(value), 8_usize),
        };
        let size = usize::try_from(size).map_err(|_| failure(AdapterFailureKind::InvalidOutput))?;
        if size < header || size > remaining.len() {
            return Err(failure(AdapterFailureKind::InvalidOutput));
        }
        let end = self
            .position
            .checked_add(size)
            .filter(|end| *end > self.position && *end <= self.data.len())
            .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
        let payload = &self.data[self.position + header..end];
        self.position = end;
        Ok(Some(Mp4Box {
            kind,
            payload,
            depth: self.depth,
        }))
    }
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, AdapterFailure> {
    let bytes = data
        .get(offset..offset + 2)
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    Ok(u16::from_be_bytes(
        bytes.try_into().expect("two-byte slice"),
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Result<u32, AdapterFailure> {
    let bytes = data
        .get(offset..offset + 4)
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("four-byte slice"),
    ))
}

fn read_u64(data: &[u8], offset: usize) -> Result<u64, AdapterFailure> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| failure(AdapterFailureKind::InvalidOutput))?;
    Ok(u64::from_be_bytes(
        bytes.try_into().expect("eight-byte slice"),
    ))
}

fn failure(kind: AdapterFailureKind) -> AdapterFailure {
    AdapterFailure::new(AdapterFailureStage::OutputValidation, kind)
}

fn validation_failure(detail: OutputValidationDetail) -> AdapterFailure {
    failure(AdapterFailureKind::InvalidOutput).with_output_check(detail)
}

#[cfg(test)]
mod tests {
    use super::*;
    use krometrail_core::{CancellationSignal, PortFuture};
    use std::time::{Duration, Instant};

    struct NeverCancelled;

    impl CancellationSignal for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }
        fn cancelled(&self) -> PortFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    struct AlreadyCancelled;
    impl CancellationSignal for AlreadyCancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
        fn cancelled(&self) -> PortFuture<'_, ()> {
            Box::pin(async {})
        }
    }

    fn control() -> OperationControl {
        OperationControl::new(
            Arc::new(NeverCancelled),
            Instant::now() + Duration::from_secs(10),
        )
    }

    fn validate_mp4(
        bytes: Arc<[u8]>,
        expected: ExpectedMp4,
    ) -> Result<ValidatedMp4, AdapterFailure> {
        super::validate_mp4(bytes, expected, &control())
    }

    fn make_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut value = Vec::with_capacity(payload.len() + 8);
        value.extend_from_slice(&u32::try_from(payload.len() + 8).unwrap().to_be_bytes());
        value.extend_from_slice(kind);
        value.extend_from_slice(payload);
        value
    }

    fn valid_mp4(width: u16, height: u16, micros: u32, terminal_delta: u32) -> Vec<u8> {
        let mut mvhd = vec![0; 20];
        mvhd[12..16].copy_from_slice(&1_000_000_u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&micros.to_be_bytes());

        let mut tkhd = vec![0; 84];
        tkhd[76..80].copy_from_slice(&(u32::from(width) << 16).to_be_bytes());
        tkhd[80..84].copy_from_slice(&(u32::from(height) << 16).to_be_bytes());

        let mut mdhd = vec![0; 20];
        mdhd[12..16].copy_from_slice(&1_000_000_u32.to_be_bytes());
        mdhd[16..20].copy_from_slice(&micros.to_be_bytes());
        let mut hdlr = vec![0; 12];
        hdlr[8..12].copy_from_slice(b"vide");

        let mut avc1_payload = vec![0; 78];
        avc1_payload[24..26].copy_from_slice(&width.to_be_bytes());
        avc1_payload[26..28].copy_from_slice(&height.to_be_bytes());
        avc1_payload.extend(make_box(b"avcC", &[1, 100, 0, 10]));
        let avc1 = make_box(b"avc1", &avc1_payload);
        let mut stsd = vec![0; 8];
        stsd[4..8].copy_from_slice(&1_u32.to_be_bytes());
        stsd.extend(avc1);
        let mut stts = vec![0; 8];
        stts[4..8].copy_from_slice(&3_u32.to_be_bytes());
        for duration in [100_000_u32, 249_999, terminal_delta] {
            stts.extend_from_slice(&1_u32.to_be_bytes());
            stts.extend_from_slice(&duration.to_be_bytes());
        }
        let mut stsz = vec![0; 12];
        stsz[8..12].copy_from_slice(&3_u32.to_be_bytes());
        for size in [1_u32, 1, 1] {
            stsz.extend_from_slice(&size.to_be_bytes());
        }
        let mut stbl_payload = make_box(b"stsd", &stsd);
        stbl_payload.extend(make_box(b"stts", &stts));
        stbl_payload.extend(make_box(b"stsz", &stsz));
        let stbl = make_box(b"stbl", &stbl_payload);
        let minf = make_box(b"minf", &stbl);

        let mut mdia = Vec::new();
        mdia.extend(make_box(b"mdhd", &mdhd));
        mdia.extend(make_box(b"hdlr", &hdlr));
        mdia.extend(minf);
        let mut trak = make_box(b"tkhd", &tkhd);
        trak.extend(make_box(b"mdia", &mdia));
        let mut moov = make_box(b"mvhd", &mvhd);
        moov.extend(make_box(b"trak", &trak));

        let mut result = make_box(b"ftyp", b"isom\0\0\0\0isomavc1");
        result.extend(make_box(b"moov", &moov));
        result.extend(make_box(b"mdat", &[0, 0, 0, 1, 0x65]));
        result
    }

    fn expected() -> ExpectedMp4 {
        ExpectedMp4 {
            canvas: PixelDimensions::new(2, 2).unwrap(),
            presentation_duration_micros: 350_000,
            sample_durations_micros: vec![100_000, 249_999, 1].into(),
            max_bytes: 1_000_000,
        }
    }

    #[test]
    fn validates_structural_h264_video_contract() {
        let bytes: Arc<[u8]> = valid_mp4(2, 2, 350_000, 1).into();
        let validated = validate_mp4(Arc::clone(&bytes), expected()).unwrap();
        assert_eq!(validated.bytes, bytes);
        assert_eq!(
            validated.output_hash.as_bytes(),
            &<[u8; 32]>::from(Sha256::digest(&bytes))
        );
    }

    #[test]
    fn duration_validation_allows_one_tick_of_container_timescale_rounding() {
        assert!(validate_duration(1_000, 1_435, 1_434_473, Mp4Check::MovieDuration).is_ok());
        assert!(validate_duration(1_000, 1_436, 1_434_473, Mp4Check::MovieDuration).is_err());
    }

    #[test]
    fn accepts_muxer_defined_terminal_zero() {
        let validated = validate_mp4(valid_mp4(2, 2, 350_000, 0).into(), expected());
        assert!(validated.is_ok());
    }

    #[test]
    fn rejects_zero_delta_on_non_terminal_samples() {
        let mut bytes = valid_mp4(2, 2, 350_000, 1);
        let offset = bytes
            .windows(4)
            .position(|window| window == 249_999_u32.to_be_bytes())
            .unwrap();
        bytes[offset..offset + 4].copy_from_slice(&0_u32.to_be_bytes());
        assert!(validate_mp4(bytes.into(), expected()).is_err());
    }

    #[test]
    fn retained_real_ffmpeg_fixture_satisfies_the_same_validator() {
        let bytes: Arc<[u8]> = include_bytes!("../tests/fixtures/video/valid-h264.mp4")
            .as_slice()
            .into();
        let validated = validate_mp4(Arc::clone(&bytes), expected()).unwrap();
        assert_eq!(validated.bytes, bytes);
    }

    #[test]
    fn retained_terminal_zero_fixture_satisfies_validator() {
        let bytes: Arc<[u8]> =
            include_bytes!("../tests/fixtures/video/terminal-hold-zero-h264.mp4")
                .as_slice()
                .into();
        let validated = validate_mp4(Arc::clone(&bytes), expected()).unwrap();
        assert_eq!(validated.bytes, bytes);
    }

    #[test]
    fn sample_duration_mismatch_reports_diverging_index() {
        let mut wrong_timeline = expected();
        wrong_timeline.sample_durations_micros = vec![100_000, 250_000, 1].into();
        let Err(failure) = validate_mp4(valid_mp4(2, 2, 350_000, 1).into(), wrong_timeline) else {
            panic!("leading sample mismatch must fail");
        };
        assert_eq!(
            failure.output_check,
            Some(OutputValidationDetail {
                check: Mp4Check::SampleDurations,
                expected: Mp4Property::Sample {
                    index: 1,
                    micros: 250_000,
                },
                observed: Mp4Property::Sample {
                    index: 1,
                    micros: 249_999,
                },
            })
        );
    }

    #[test]
    fn mp4_property_display_is_bounded_and_stable() {
        assert_eq!(Mp4Property::Count(3).to_string(), "count=3");
        assert_eq!(Mp4Property::Micros(350_000).to_string(), "micros=350000");
        assert_eq!(Mp4Property::Codec(*b"avc1").to_string(), "codec=avc1");
        assert_eq!(
            Mp4Property::Codec([0, 1, b'a', b'!']).to_string(),
            "codec=0x00016121"
        );
        assert_eq!(
            Mp4Property::Dimensions {
                width: 2,
                height: 2,
            }
            .to_string(),
            "2x2"
        );
        assert_eq!(
            Mp4Property::Sample {
                index: 2,
                micros: 0,
            }
            .to_string(),
            "sample[2]=0"
        );
        assert_eq!(Mp4Property::Bytes(1_508).to_string(), "bytes=1508");
        assert_eq!(Mp4Property::Absent.to_string(), "absent");
    }

    #[test]
    fn rejects_wrong_dimensions_codec_audio_and_duration() {
        let Err(dimension_failure) = validate_mp4(valid_mp4(4, 2, 350_000, 1).into(), expected())
        else {
            panic!("dimension mismatch must fail");
        };
        assert_eq!(
            dimension_failure.output_check,
            Some(OutputValidationDetail {
                check: Mp4Check::VideoDimensions,
                expected: Mp4Property::Dimensions {
                    width: 2,
                    height: 2,
                },
                observed: Mp4Property::Dimensions {
                    width: 4,
                    height: 2,
                },
            })
        );
        assert!(validate_mp4(valid_mp4(2, 2, 349_000, 1).into(), expected()).is_err());

        let mut wrong_codec = valid_mp4(2, 2, 350_000, 1);
        let offset = wrong_codec
            .windows(4)
            .rposition(|window| window == b"avc1")
            .unwrap();
        wrong_codec[offset..offset + 4].copy_from_slice(b"hvc1");
        let Err(codec_failure) = validate_mp4(wrong_codec.into(), expected()) else {
            panic!("codec mismatch must fail");
        };
        assert_eq!(
            codec_failure.output_check,
            Some(OutputValidationDetail {
                check: Mp4Check::VideoCodec,
                expected: Mp4Property::Codec(*b"avc1"),
                observed: Mp4Property::Codec(*b"hvc1"),
            })
        );

        let mut audio = valid_mp4(2, 2, 350_000, 1);
        let offset = audio
            .windows(4)
            .position(|window| window == b"vide")
            .unwrap();
        audio[offset..offset + 4].copy_from_slice(b"soun");
        assert!(validate_mp4(audio.into(), expected()).is_err());
    }

    #[test]
    fn rejects_truncation_indefinite_boxes_and_declared_overflow() {
        let mut truncated = valid_mp4(2, 2, 350_000, 1);
        truncated.pop();
        assert!(validate_mp4(truncated.into(), expected()).is_err());

        let mut indefinite = valid_mp4(2, 2, 350_000, 1);
        indefinite[0..4].copy_from_slice(&0_u32.to_be_bytes());
        assert!(validate_mp4(indefinite.into(), expected()).is_err());

        let mut oversized = valid_mp4(2, 2, 350_000, 1);
        oversized[0..4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(validate_mp4(oversized.into(), expected()).is_err());
    }

    #[test]
    fn bounded_box_walker_rejects_excessive_nesting_and_box_count() {
        let mut nested = make_box(b"mvhd", &[0; 20]);
        for _ in 0..=MAX_BOX_DEPTH {
            nested = make_box(b"trak", &nested);
        }
        let mut bytes = make_box(b"ftyp", b"isom\0\0\0\0");
        bytes.extend(make_box(b"moov", &nested));
        bytes.extend(make_box(b"mdat", &[1]));
        assert!(validate_mp4(bytes.into(), expected()).is_err());

        let mut many = make_box(b"ftyp", b"isom\0\0\0\0");
        for _ in 0..=MAX_BOX_COUNT {
            many.extend(make_box(b"free", &[]));
        }
        assert!(validate_mp4(many.into(), expected()).is_err());
    }

    #[test]
    fn rejects_sample_count_timeline_and_composition_offset_mismatches() {
        let mut wrong_timeline = expected();
        wrong_timeline.sample_durations_micros = vec![100_000, 250_000, 1].into();
        assert!(validate_mp4(valid_mp4(2, 2, 350_000, 1).into(), wrong_timeline).is_err());

        let mut wrong_count = valid_mp4(2, 2, 350_000, 1);
        let stsz = wrong_count
            .windows(4)
            .position(|window| window == b"stsz")
            .unwrap();
        wrong_count[stsz + 12..stsz + 16].copy_from_slice(&2_u32.to_be_bytes());
        assert!(validate_mp4(wrong_count.into(), expected()).is_err());

        let mut excessive_stts = vec![0; 16];
        excessive_stts[4..8].copy_from_slice(&1_u32.to_be_bytes());
        excessive_stts[8..12].copy_from_slice(
            &u32::try_from(MAX_MP4_SAMPLE_COUNT + 1)
                .unwrap()
                .to_be_bytes(),
        );
        excessive_stts[12..16].copy_from_slice(&1_u32.to_be_bytes());
        assert!(parse_stts(&excessive_stts, &control()).is_err());

        let mut nonzero_ctts = vec![0; 16];
        nonzero_ctts[4..8].copy_from_slice(&1_u32.to_be_bytes());
        nonzero_ctts[8..12].copy_from_slice(&3_u32.to_be_bytes());
        nonzero_ctts[12..16].copy_from_slice(&1_u32.to_be_bytes());
        assert!(parse_ctts(&nonzero_ctts, &control()).is_err());
    }

    #[test]
    fn cancellation_is_observed_during_output_validation() {
        let control = OperationControl::new(
            Arc::new(AlreadyCancelled),
            Instant::now() + Duration::from_secs(10),
        );
        let Err(failure) =
            super::validate_mp4(valid_mp4(2, 2, 350_000, 1).into(), expected(), &control)
        else {
            panic!("cancelled validation must fail");
        };
        assert_eq!(failure.kind, AdapterFailureKind::Cancelled);
    }
}
