use krometrail_core::{
    CaptureGap, ErrorCode, FrameId, GapId, KrometrailError, MAX_VIDEO_PRESENTATION_SEGMENTS,
    MINIMUM_VISIBLE_FRAME_NANOS, MODEL_GAP_HOLD_NANOS, MODEL_MEANINGFUL_HOLD_NANOS, NonEmptyText,
    PresentationRange, PresentationTime, Result, SessionRange, SessionTime, TERMINAL_HOLD_NANOS,
    VideoPlanInput, VideoPresentationPlan, VideoPresentationPolicy, VideoPresentationSegment,
    VideoSegmentSource, VideoTimingBasis,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct CoalescedGap {
    gap_ids: Vec<GapId>,
    source_range: SessionRange,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DraftSegment {
    source: VideoSegmentSource,
    duration_nanos: u64,
    timing_basis: VideoTimingBasis,
}

#[allow(dead_code)] // Consumed by the retained-generation service in the next dependency layer.
pub(super) fn build_presentation_plan(input: VideoPlanInput) -> Result<VideoPresentationPlan> {
    let frames = input.frames();
    let first_time = frames
        .first()
        .expect("validated video plan input has frames")
        .session_time();
    let last_time = frames
        .last()
        .expect("validated video plan input has frames")
        .session_time();
    let presented_source_range = SessionRange::new(first_time, last_time)?;
    let gaps = coalesce_epoch_gaps(&input.range().gaps, presented_source_range)?;
    let mut segments = real_time_segments(&input, &gaps)?;
    if input.policy() == VideoPresentationPolicy::ModelOptimized {
        segments = optimize_for_model(segments, input.meaningful_frame_ids())?;
    }

    VideoPresentationPlan::new(
        input.policy(),
        input.range().requested_range,
        input.range().resolved_range,
        presented_source_range,
        input.epoch().clone(),
        input.frames().iter().map(|frame| frame.id()).collect(),
        input
            .frames()
            .iter()
            .map(|frame| frame.session_time())
            .collect(),
        input.meaningful_frame_ids().to_vec(),
        segments,
        input.output(),
    )
}

fn coalesce_epoch_gaps(
    gaps: &[CaptureGap],
    presented_source_range: SessionRange,
) -> Result<Vec<CoalescedGap>> {
    let mut clipped: Vec<_> = gaps
        .iter()
        .filter_map(|gap| {
            let start = gap.range().start().max(presented_source_range.start());
            let end = gap.range().end().min(presented_source_range.end());
            (start < end).then_some((start, end, gap.id()))
        })
        .collect();
    clipped.sort_unstable_by_key(|(start, end, id)| (*start, *end, *id));

    let mut output: Vec<CoalescedGap> = Vec::new();
    for (start, end, id) in clipped {
        if let Some(last) = output.last_mut()
            && start <= last.source_range.end()
        {
            let merged_end = end.max(last.source_range.end());
            last.source_range = SessionRange::new(last.source_range.start(), merged_end)?;
            if !last.gap_ids.contains(&id) {
                last.gap_ids.push(id);
                last.gap_ids.sort_unstable();
            }
            continue;
        }
        output.push(CoalescedGap {
            gap_ids: vec![id],
            source_range: SessionRange::new(start, end)?,
        });
    }
    Ok(output)
}

fn real_time_segments(
    input: &VideoPlanInput,
    gaps: &[CoalescedGap],
) -> Result<Vec<VideoPresentationSegment>> {
    let frames = input.frames();
    let mut drafts = Vec::new();
    for pair in frames.windows(2) {
        let frame = &pair[0];
        let next = &pair[1];
        let start = frame.session_time();
        let end = next.session_time();
        if start == end {
            drafts.push(DraftSegment {
                source: VideoSegmentSource::source_frame(frame.id(), start)?,
                duration_nanos: MINIMUM_VISIBLE_FRAME_NANOS,
                timing_basis: VideoTimingBasis::MinimumVisibleFrame,
            });
            continue;
        }

        let mut cursor = start;
        for gap in gaps {
            let gap_start = gap.source_range.start().max(start);
            let gap_end = gap.source_range.end().min(end);
            if gap_start >= gap_end || gap_end <= cursor {
                continue;
            }
            if cursor < gap_start {
                push_draft(
                    &mut drafts,
                    DraftSegment {
                        source: VideoSegmentSource::source_frame(frame.id(), start)?,
                        duration_nanos: subtract_time(gap_start, cursor)?,
                        timing_basis: VideoTimingBasis::RecordedDelta,
                    },
                )?;
            }
            let visible_gap_start = cursor.max(gap_start);
            if visible_gap_start < gap_end {
                push_draft(
                    &mut drafts,
                    DraftSegment {
                        source: VideoSegmentSource::gap_slate(
                            gap.gap_ids.clone(),
                            SessionRange::new(visible_gap_start, gap_end)?,
                        )?,
                        duration_nanos: subtract_time(gap_end, visible_gap_start)?,
                        timing_basis: VideoTimingBasis::RecordedGap,
                    },
                )?;
                cursor = gap_end;
            }
        }
        if cursor < end {
            push_draft(
                &mut drafts,
                DraftSegment {
                    source: VideoSegmentSource::source_frame(frame.id(), start)?,
                    duration_nanos: subtract_time(end, cursor)?,
                    timing_basis: VideoTimingBasis::RecordedDelta,
                },
            )?;
        }
    }

    let last = frames
        .last()
        .expect("validated video plan input has frames");
    push_draft(
        &mut drafts,
        DraftSegment {
            source: VideoSegmentSource::source_frame(last.id(), last.session_time())?,
            duration_nanos: TERMINAL_HOLD_NANOS,
            timing_basis: VideoTimingBasis::TerminalHold,
        },
    )?;
    finish_segments(drafts)
}

fn optimize_for_model(
    segments: Vec<VideoPresentationSegment>,
    meaningful_frame_ids: &[FrameId],
) -> Result<Vec<VideoPresentationSegment>> {
    let mut drafts: Vec<_> = segments
        .into_iter()
        .map(|segment| DraftSegment {
            source: segment.source().clone(),
            duration_nanos: segment.presentation().duration_nanos(),
            timing_basis: segment.timing_basis(),
        })
        .collect();

    for draft in &mut drafts {
        if matches!(draft.source, VideoSegmentSource::GapSlate { .. })
            && draft.duration_nanos < MODEL_GAP_HOLD_NANOS
        {
            draft.duration_nanos = MODEL_GAP_HOLD_NANOS;
            draft.timing_basis = VideoTimingBasis::ModelGapHold;
        }
    }

    for frame_id in meaningful_frame_ids {
        let positions: Vec<_> = drafts
            .iter()
            .enumerate()
            .filter_map(|(index, draft)| match draft.source {
                VideoSegmentSource::SourceFrame {
                    frame_id: candidate,
                    ..
                } if candidate == *frame_id => Some(index),
                _ => None,
            })
            .collect();
        let Some(first_position) = positions.first().copied() else {
            return Err(invalid_video(
                "a meaningful frame is fully obscured by a declared capture gap",
            ));
        };
        let total = positions.iter().try_fold(0_u64, |total, position| {
            total
                .checked_add(drafts[*position].duration_nanos)
                .ok_or_else(|| limit_error("model-optimized frame hold overflowed"))
        })?;
        if total < MODEL_MEANINGFUL_HOLD_NANOS {
            drafts[first_position].duration_nanos = drafts[first_position]
                .duration_nanos
                .checked_add(MODEL_MEANINGFUL_HOLD_NANOS - total)
                .ok_or_else(|| limit_error("model-optimized frame hold overflowed"))?;
            drafts[first_position].timing_basis = VideoTimingBasis::ModelMeaningfulHold;
        }
    }
    finish_segments(drafts)
}

fn push_draft(output: &mut Vec<DraftSegment>, draft: DraftSegment) -> Result<()> {
    if let Some(last) = output.last_mut()
        && let (
            VideoSegmentSource::GapSlate {
                gap_ids: last_ids,
                source_range: last_range,
            },
            VideoSegmentSource::GapSlate {
                gap_ids: next_ids,
                source_range: next_range,
            },
        ) = (&mut last.source, &draft.source)
        && last.timing_basis == draft.timing_basis
        && *last_ids == *next_ids
        && last_range.end() == next_range.start()
    {
        *last_range = SessionRange::new(last_range.start(), next_range.end())?;
        last.duration_nanos = last
            .duration_nanos
            .checked_add(draft.duration_nanos)
            .ok_or_else(|| limit_error("video gap presentation duration overflowed"))?;
        return Ok(());
    }
    output.push(draft);
    Ok(())
}

fn finish_segments(drafts: Vec<DraftSegment>) -> Result<Vec<VideoPresentationSegment>> {
    if drafts.len() > MAX_VIDEO_PRESENTATION_SEGMENTS {
        return Err(limit_error(
            "video plan exceeds the 512 presentation segment server limit",
        ));
    }
    let mut cursor = 0_u64;
    drafts
        .into_iter()
        .enumerate()
        .map(|(index, draft)| {
            let end = cursor
                .checked_add(draft.duration_nanos)
                .ok_or_else(|| limit_error("video presentation duration overflowed"))?;
            let range = PresentationRange::new(
                PresentationTime::from_nanos(cursor)?,
                PresentationTime::from_nanos(end)?,
            )?;
            cursor = end;
            VideoPresentationSegment::new(index as u32, draft.source, range, draft.timing_basis)
        })
        .collect()
}

fn subtract_time(end: SessionTime, start: SessionTime) -> Result<u64> {
    end.as_nanos()
        .checked_sub(start.as_nanos())
        .ok_or_else(|| invalid_video("video source interval is not ordered"))
}

fn invalid_video(message: impl Into<String>) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new(message.into()).expect("video validation messages are non-empty"),
    )
}

fn limit_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::ResourceLimitExceeded,
        NonEmptyText::new(message).expect("video limit messages are non-empty"),
    )
}

#[cfg(test)]
pub(super) fn segment_limit_probe(count: usize) -> Result<()> {
    let id = FrameId::from_uuid(uuid::Uuid::from_u128(1));
    let drafts = (0..count)
        .map(|_| DraftSegment {
            source: VideoSegmentSource::source_frame(id, SessionTime::ZERO).unwrap(),
            duration_nanos: 1,
            timing_basis: VideoTimingBasis::MinimumVisibleFrame,
        })
        .collect();
    finish_segments(drafts).map(|_| ())
}

#[cfg(test)]
pub(super) fn timing_constants() -> (u64, u64, u64, u64) {
    (
        MINIMUM_VISIBLE_FRAME_NANOS,
        TERMINAL_HOLD_NANOS,
        MODEL_MEANINGFUL_HOLD_NANOS,
        MODEL_GAP_HOLD_NANOS,
    )
}
