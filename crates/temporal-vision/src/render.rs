pub(crate) mod canvas;
pub(crate) mod font;

use std::{
    collections::BTreeMap,
    fmt::Display,
    num::{NonZeroU32, NonZeroUsize},
};

use crate::{
    AlgorithmDescriptor, ArtifactKind, ArtifactManifest, DeclaredGap, EncodedImage, ErrorCode,
    EvidenceClass, FrameSequence, GeneratedArtifact, Marker, MeasurementParameters,
    NormalizationKind, NormalizationStep, NormalizedSequence, ParameterValue, Parameters,
    PixelDimensions, Result, SelectionReason, StoryboardSelection, StoryboardTileLimit, Timestamp,
    VisionError, generator_descriptor, normalize::make_parameters,
    pair_analysis::PairAnalysisContext, select_storyboard_frames,
};
use canvas::{BLACK, Canvas, MUTED, PANEL, WARNING, WHITE, canvas_limit_error};
use font::{CELL_WIDTH, draw_text, ellipsize};

const PREFERRED_TILE_WIDTH: u32 = 240;
const MINIMUM_TILE_WIDTH: u32 = 160;
const HEADER_HEIGHT: u32 = 52;
const STORY_ANNOTATION_HEIGHT: u32 = 70;
const ORIENTATION_ANNOTATION_HEIGHT: u32 = 48;
const TIMELINE_HEIGHT: u32 = 24;
const DEFAULT_MAX_DIMENSION: u32 = 4_096;
const DEFAULT_MAX_BYTES: usize = 64 * 1024 * 1024;

/// Required human-readable labels placed on every artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactLabels {
    title: String,
    source: String,
}

impl ArtifactLabels {
    pub fn new(title: impl Into<String>, source: impl Into<String>) -> Result<Self> {
        let title = title.into();
        let source = source.into();
        if title.trim().is_empty() || source.trim().is_empty() {
            return Err(VisionError::new(
                ErrorCode::InvalidParameter,
                "artifact title and source context must not be empty",
            ));
        }
        Ok(Self { title, source })
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// Explicit raster and encoded-output ceilings for one generation request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderLimits {
    max_width: NonZeroU32,
    max_height: NonZeroU32,
    max_canvas_bytes: NonZeroUsize,
    max_encoded_bytes: NonZeroUsize,
}

impl RenderLimits {
    pub const fn new(
        max_width: NonZeroU32,
        max_height: NonZeroU32,
        max_canvas_bytes: NonZeroUsize,
        max_encoded_bytes: NonZeroUsize,
    ) -> Self {
        Self {
            max_width,
            max_height,
            max_canvas_bytes,
            max_encoded_bytes,
        }
    }

    pub const fn max_width(self) -> u32 {
        self.max_width.get()
    }

    pub const fn max_height(self) -> u32 {
        self.max_height.get()
    }

    pub const fn max_canvas_bytes(self) -> usize {
        self.max_canvas_bytes.get()
    }

    pub const fn max_encoded_bytes(self) -> usize {
        self.max_encoded_bytes.get()
    }
}

impl Default for RenderLimits {
    fn default() -> Self {
        Self::new(
            NonZeroU32::new(DEFAULT_MAX_DIMENSION).expect("default is nonzero"),
            NonZeroU32::new(DEFAULT_MAX_DIMENSION).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).expect("default is nonzero"),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).expect("default is nonzero"),
        )
    }
}

/// Complete deterministic storyboard request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoryboardParameters {
    anchor: Timestamp,
    tile_limit: StoryboardTileLimit,
    measurement: MeasurementParameters,
    labels: ArtifactLabels,
    limits: RenderLimits,
}

impl StoryboardParameters {
    pub fn new(
        anchor: Timestamp,
        tile_limit: StoryboardTileLimit,
        measurement: MeasurementParameters,
        labels: ArtifactLabels,
        limits: RenderLimits,
    ) -> Self {
        Self {
            anchor,
            tile_limit,
            measurement,
            labels,
            limits,
        }
    }

    pub const fn anchor(&self) -> Timestamp {
        self.anchor
    }
    pub const fn tile_limit(&self) -> StoryboardTileLimit {
        self.tile_limit
    }
    pub const fn measurement(&self) -> MeasurementParameters {
        self.measurement
    }
    pub const fn labels(&self) -> &ArtifactLabels {
        &self.labels
    }
    pub const fn limits(&self) -> RenderLimits {
        self.limits
    }
    pub const fn preferred_tile_width(&self) -> u32 {
        PREFERRED_TILE_WIDTH
    }
    pub const fn minimum_tile_width(&self) -> u32 {
        MINIMUM_TILE_WIDTH
    }
}

/// Storyboard, optional orientation composite, and their shared selection plan.
#[derive(Clone, Debug, PartialEq)]
pub struct StoryboardArtifacts<ArtifactId, FrameId, MarkerId, GapId> {
    storyboard: GeneratedArtifact<ArtifactId, FrameId, MarkerId, GapId>,
    orientation: Option<GeneratedArtifact<ArtifactId, FrameId, MarkerId, GapId>>,
    selection: StoryboardSelection<FrameId>,
}

impl<A, F, M, G> StoryboardArtifacts<A, F, M, G> {
    pub const fn storyboard(&self) -> &GeneratedArtifact<A, F, M, G> {
        &self.storyboard
    }
    pub fn orientation(&self) -> Option<&GeneratedArtifact<A, F, M, G>> {
        self.orientation.as_ref()
    }
    pub const fn selection(&self) -> &StoryboardSelection<F> {
        &self.selection
    }
}

/// Generate the primary storyboard and, when requested, its orientation composite.
pub fn generate_storyboard<A, F, M, G, P>(
    storyboard_artifact_id: A,
    orientation_artifact_id: Option<A>,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: StoryboardParameters,
) -> Result<StoryboardArtifacts<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    let selection = select_storyboard_frames(
        source,
        normalized,
        parameters.anchor,
        parameters.tile_limit,
        parameters.measurement,
    )?;
    let marker_assignments = assign_markers(source.markers(), &selection);
    let storyboard_raster = render_storyboard(
        source,
        normalized,
        &selection,
        &marker_assignments,
        &parameters,
    )?;
    let storyboard = finish_artifact(
        storyboard_artifact_id,
        ArtifactKind::Storyboard,
        source,
        normalized,
        selection
            .selected_frames()
            .iter()
            .map(|frame| frame.frame_id().clone())
            .collect(),
        &selection,
        &marker_assignments,
        &parameters,
        storyboard_raster,
        "chronological_strip",
    )?;

    let orientation = if let Some(artifact_id) = orientation_artifact_id {
        let role_indices = [
            selection.before_index(),
            selection.during_index(),
            selection.after_index(),
        ];
        let mut selected_indices = role_indices.to_vec();
        selected_indices.sort_unstable();
        selected_indices.dedup();
        let selected_ids = selected_indices
            .iter()
            .map(|index| source.frames()[*index].id().clone())
            .collect();
        let raster = render_orientation(source, normalized, &selection, &parameters)?;
        Some(finish_artifact(
            artifact_id,
            ArtifactKind::BeforeDuringAfter,
            source,
            normalized,
            selected_ids,
            &selection,
            &marker_assignments,
            &parameters,
            raster,
            "before_during_after",
        )?)
    } else {
        None
    };

    Ok(StoryboardArtifacts {
        storyboard,
        orientation,
        selection,
    })
}

/// Generate storyboard and optional orientation using precomputed adjacent pairs.
pub fn generate_storyboard_with_context<A, F, M, G, P>(
    storyboard_artifact_id: A,
    orientation_artifact_id: Option<A>,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: StoryboardParameters,
    context: &PairAnalysisContext<'_>,
) -> Result<StoryboardArtifacts<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    context.ensure_normalized(normalized, parameters.measurement)?;
    let selection = crate::select::select_storyboard_frames_with_comparisons(
        source,
        normalized,
        parameters.anchor,
        parameters.tile_limit,
        parameters.measurement,
        context.comparisons(),
    )?;
    let marker_assignments = assign_markers(source.markers(), &selection);
    let storyboard_raster = render_storyboard(
        source,
        normalized,
        &selection,
        &marker_assignments,
        &parameters,
    )?;
    let storyboard = finish_artifact(
        storyboard_artifact_id,
        ArtifactKind::Storyboard,
        source,
        normalized,
        selection
            .selected_frames()
            .iter()
            .map(|frame| frame.frame_id().clone())
            .collect(),
        &selection,
        &marker_assignments,
        &parameters,
        storyboard_raster,
        "chronological_strip",
    )?;
    let orientation = orientation_artifact_id.map_or(Ok(None), |artifact_id| {
        let role_indices = [
            selection.before_index(),
            selection.during_index(),
            selection.after_index(),
        ];
        let mut selected_indices = role_indices.to_vec();
        selected_indices.sort_unstable();
        selected_indices.dedup();
        let selected_ids = selected_indices
            .iter()
            .map(|index| source.frames()[*index].id().clone())
            .collect();
        let raster = render_orientation(source, normalized, &selection, &parameters)?;
        finish_artifact(
            artifact_id,
            ArtifactKind::BeforeDuringAfter,
            source,
            normalized,
            selected_ids,
            &selection,
            &marker_assignments,
            &parameters,
            raster,
            "before_during_after",
        )
        .map(Some)
    })?;
    Ok(StoryboardArtifacts {
        storyboard,
        orientation,
        selection,
    })
}

#[cfg(test)]
pub(crate) fn generate_storyboard_direct<A, F, M, G, P>(
    storyboard_artifact_id: A,
    orientation_artifact_id: Option<A>,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    parameters: StoryboardParameters,
) -> Result<StoryboardArtifacts<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    let selection = crate::select::select_storyboard_frames_direct(
        source,
        normalized,
        parameters.anchor,
        parameters.tile_limit,
        parameters.measurement,
    )?;
    let marker_assignments = assign_markers(source.markers(), &selection);
    let storyboard_raster = render_storyboard(
        source,
        normalized,
        &selection,
        &marker_assignments,
        &parameters,
    )?;
    let storyboard = finish_artifact(
        storyboard_artifact_id,
        ArtifactKind::Storyboard,
        source,
        normalized,
        selection
            .selected_frames()
            .iter()
            .map(|frame| frame.frame_id().clone())
            .collect(),
        &selection,
        &marker_assignments,
        &parameters,
        storyboard_raster,
        "chronological_strip",
    )?;
    let orientation = if let Some(artifact_id) = orientation_artifact_id {
        let role_indices = [
            selection.before_index(),
            selection.during_index(),
            selection.after_index(),
        ];
        let mut selected_indices = role_indices.to_vec();
        selected_indices.sort_unstable();
        selected_indices.dedup();
        let selected_ids = selected_indices
            .iter()
            .map(|index| source.frames()[*index].id().clone())
            .collect();
        let raster = render_orientation(source, normalized, &selection, &parameters)?;
        Some(finish_artifact(
            artifact_id,
            ArtifactKind::BeforeDuringAfter,
            source,
            normalized,
            selected_ids,
            &selection,
            &marker_assignments,
            &parameters,
            raster,
            "before_during_after",
        )?)
    } else {
        None
    };
    Ok(StoryboardArtifacts {
        storyboard,
        orientation,
        selection,
    })
}

struct Raster {
    canvas: Canvas,
    tile_width: u32,
    image_height: u32,
}

fn render_storyboard<F: Display + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    selection: &StoryboardSelection<F>,
    marker_assignments: &[Vec<usize>],
    parameters: &StoryboardParameters,
) -> Result<Raster> {
    let layout = checked_layout(
        selection.selected_frames().len(),
        normalized.dimensions(),
        STORY_ANNOTATION_HEIGHT,
        parameters.limits,
    )?;
    let mut canvas = Canvas::new(
        layout.dimensions,
        BLACK,
        parameters.limits.max_canvas_bytes(),
    )?;
    draw_header(
        &mut canvas,
        source,
        parameters,
        !source.gaps().is_empty(),
        selection.omitted_anchors().len(),
    )?;
    let image_y = HEADER_HEIGHT;
    let annotation_y = image_y + layout.image_height;
    canvas.fill_rect(
        0,
        annotation_y,
        layout.dimensions.width(),
        STORY_ANNOTATION_HEIGHT,
        PANEL,
    )?;
    canvas.fill_rect(
        0,
        annotation_y + STORY_ANNOTATION_HEIGHT,
        layout.dimensions.width(),
        TIMELINE_HEIGHT,
        BLACK,
    )?;

    for (tile, selected) in selection.selected_frames().iter().enumerate() {
        let tile_x = u32::try_from(tile)
            .map_err(|_| canvas_limit_error())?
            .checked_mul(layout.tile_width)
            .ok_or_else(canvas_limit_error)?;
        let frame = &normalized.frames()[selected.frame_index()];
        canvas.draw_linear_frame(
            frame.dimensions(),
            frame.linear_rgb16(),
            tile_x,
            image_y,
            layout.tile_width,
            layout.image_height,
        )?;
        draw_story_annotation(
            &mut canvas,
            (tile_x, annotation_y, layout.tile_width),
            selected,
            source,
            &marker_assignments[tile],
            parameters.anchor,
        )?;
    }

    for pair in selection.selected_frames().windows(2) {
        if gap_intersects(source.gaps(), pair[0].timestamp(), pair[1].timestamp()) {
            let right_tile = selection
                .selected_frames()
                .iter()
                .position(|frame| frame.frame_index() == pair[1].frame_index())
                .expect("selected frame is present");
            let boundary_x = u32::try_from(right_tile)
                .map_err(|_| canvas_limit_error())?
                .checked_mul(layout.tile_width)
                .ok_or_else(canvas_limit_error)?;
            let hatch_x = boundary_x.saturating_sub(4);
            canvas.draw_hatch(
                hatch_x,
                annotation_y,
                8,
                STORY_ANNOTATION_HEIGHT + TIMELINE_HEIGHT,
                WARNING,
            )?;
            let label_x = boundary_x.saturating_sub(18);
            draw_text(&mut canvas, label_x, annotation_y + 50, "GAP", WARNING)?;
        }
    }
    draw_timeline(
        &mut canvas,
        annotation_y + STORY_ANNOTATION_HEIGHT,
        source,
        layout.dimensions.width(),
    )?;
    Ok(Raster {
        canvas,
        tile_width: layout.tile_width,
        image_height: layout.image_height,
    })
}

fn render_orientation<F: Display + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    selection: &StoryboardSelection<F>,
    parameters: &StoryboardParameters,
) -> Result<Raster> {
    let layout = checked_layout(
        3,
        normalized.dimensions(),
        ORIENTATION_ANNOTATION_HEIGHT,
        parameters.limits,
    )?;
    let mut canvas = Canvas::new(
        layout.dimensions,
        BLACK,
        parameters.limits.max_canvas_bytes(),
    )?;
    draw_header(
        &mut canvas,
        source,
        parameters,
        !source.gaps().is_empty(),
        selection.omitted_anchors().len(),
    )?;
    let image_y = HEADER_HEIGHT;
    let annotation_y = image_y + layout.image_height;
    canvas.fill_rect(
        0,
        annotation_y,
        layout.dimensions.width(),
        ORIENTATION_ANNOTATION_HEIGHT,
        PANEL,
    )?;
    canvas.fill_rect(
        0,
        annotation_y + ORIENTATION_ANNOTATION_HEIGHT,
        layout.dimensions.width(),
        TIMELINE_HEIGHT,
        BLACK,
    )?;

    let roles = [
        ("BEFORE", selection.before_index()),
        (during_label(selection), selection.during_index()),
        ("AFTER", selection.after_index()),
    ];
    for (panel, (label, frame_index)) in roles.into_iter().enumerate() {
        let panel_x = u32::try_from(panel)
            .map_err(|_| canvas_limit_error())?
            .checked_mul(layout.tile_width)
            .ok_or_else(canvas_limit_error)?;
        let frame = &normalized.frames()[frame_index];
        canvas.draw_linear_frame(
            frame.dimensions(),
            frame.linear_rgb16(),
            panel_x,
            image_y,
            layout.tile_width,
            layout.image_height,
        )?;
        let max_cells = usize::try_from(layout.tile_width / CELL_WIDTH)
            .unwrap_or(0)
            .saturating_sub(1);
        draw_text(
            &mut canvas,
            panel_x + 3,
            annotation_y + 2,
            &ellipsize(label, max_cells),
            WHITE,
        )?;
        let source_label = format!("FRAME {}", source.frames()[frame_index].id());
        draw_text(
            &mut canvas,
            panel_x + 3,
            annotation_y + 14,
            &ellipsize(&source_label, max_cells),
            MUTED,
        )?;
        let time = time_and_offset(source.frames()[frame_index].timestamp(), parameters.anchor);
        draw_text(
            &mut canvas,
            panel_x + 3,
            annotation_y + 26,
            &ellipsize(&time, max_cells),
            MUTED,
        )?;
    }
    draw_timeline(
        &mut canvas,
        annotation_y + ORIENTATION_ANNOTATION_HEIGHT,
        source,
        layout.dimensions.width(),
    )?;
    Ok(Raster {
        canvas,
        tile_width: layout.tile_width,
        image_height: layout.image_height,
    })
}

fn during_label<F>(selection: &StoryboardSelection<F>) -> &'static str {
    let selected = selection
        .selected_frames()
        .iter()
        .find(|frame| frame.frame_index() == selection.during_index());
    if selected.is_some_and(|frame| {
        frame
            .reasons()
            .contains(&SelectionReason::PeakBaselineChange)
    }) {
        "DURING - PEAK BASELINE CHANGE"
    } else if selection.during_index() != selection.before_index() {
        "DURING - FIRST POST ANCHOR"
    } else {
        "DURING - BASELINE FALLBACK"
    }
}

struct Layout {
    dimensions: PixelDimensions,
    tile_width: u32,
    image_height: u32,
}

fn checked_layout(
    tile_count: usize,
    source_dimensions: PixelDimensions,
    annotation_height: u32,
    limits: RenderLimits,
) -> Result<Layout> {
    let count = u32::try_from(tile_count).map_err(|_| canvas_limit_error())?;
    if count == 0 {
        return Err(canvas_limit_error());
    }
    let tile_width = PREFERRED_TILE_WIDTH.min(limits.max_width() / count);
    if tile_width < MINIMUM_TILE_WIDTH {
        return Err(canvas_limit_error());
    }
    let image_height_u64 = u64::from(tile_width)
        .checked_mul(u64::from(source_dimensions.height()))
        .ok_or_else(canvas_limit_error)?
        .checked_add(u64::from(source_dimensions.width()) / 2)
        .ok_or_else(canvas_limit_error)?
        / u64::from(source_dimensions.width());
    let image_height = u32::try_from(image_height_u64.max(1)).map_err(|_| canvas_limit_error())?;
    let width = tile_width
        .checked_mul(count)
        .ok_or_else(canvas_limit_error)?;
    let height = HEADER_HEIGHT
        .checked_add(image_height)
        .and_then(|value| value.checked_add(annotation_height))
        .and_then(|value| value.checked_add(TIMELINE_HEIGHT))
        .ok_or_else(canvas_limit_error)?;
    if width > limits.max_width() || height > limits.max_height() {
        return Err(canvas_limit_error());
    }
    let dimensions = PixelDimensions::new(width, height).map_err(|_| canvas_limit_error())?;
    let bytes = dimensions
        .pixel_count()?
        .checked_mul(3)
        .ok_or_else(canvas_limit_error)?;
    if bytes > limits.max_canvas_bytes() {
        return Err(canvas_limit_error());
    }
    Ok(Layout {
        dimensions,
        tile_width,
        image_height,
    })
}

fn draw_header<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    source: &FrameSequence<F, M, G, P>,
    parameters: &StoryboardParameters,
    has_gaps: bool,
    omitted_anchors: usize,
) -> Result<()> {
    let max_cells = usize::try_from(canvas.dimensions().width() / CELL_WIDTH)
        .unwrap_or(0)
        .saturating_sub(1);
    draw_text(
        canvas,
        4,
        2,
        &ellipsize(parameters.labels.title(), max_cells),
        WHITE,
    )?;
    draw_text(
        canvas,
        4,
        14,
        &ellipsize(parameters.labels.source(), max_cells),
        MUTED,
    )?;
    let range = format!(
        "RANGE {} - {} | ANCHOR {}",
        format_time(source.range().start()),
        format_time(source.range().end()),
        format_time(parameters.anchor)
    );
    draw_text(canvas, 4, 26, &ellipsize(&range, max_cells), MUTED)?;
    let warning = match (has_gaps, omitted_anchors) {
        (true, 0) => "GAP - UNSEEN BEHAVIOR MAY HAVE OCCURRED".to_owned(),
        (false, count) if count > 0 => format!("ANCHORS OMITTED: {count}; SEE MANIFEST"),
        (true, count) => format!(
            "GAP - UNSEEN BEHAVIOR MAY HAVE OCCURRED | ANCHORS OMITTED: {count}; SEE MANIFEST"
        ),
        (false, 0) => "SOURCE-DERIVED | SELECTED FRAMES; SEE MANIFEST".to_owned(),
        _ => unreachable!(),
    };
    draw_text(
        canvas,
        4,
        38,
        &ellipsize(&warning, max_cells),
        if has_gaps { WARNING } else { MUTED },
    )
}

fn draw_story_annotation<F: Display + Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    tile: (u32, u32, u32),
    selected: &crate::SelectedFrame<F>,
    source: &FrameSequence<F, M, G, P>,
    marker_indices: &[usize],
    anchor: Timestamp,
) -> Result<()> {
    let (tile_x, annotation_y, tile_width) = tile;
    let max_cells = usize::try_from(tile_width / CELL_WIDTH)
        .unwrap_or(0)
        .saturating_sub(1);
    let marker_text = if marker_indices.is_empty() {
        "MARKERS NONE".to_owned()
    } else {
        format!(
            "MARKERS {}",
            marker_indices
                .iter()
                .map(|index| {
                    let marker = &source.markers()[*index];
                    format!("{}: {}", marker.kind(), marker.label())
                })
                .collect::<Vec<_>>()
                .join(" | ")
        )
    };
    let lines = [
        time_and_offset(selected.timestamp(), anchor),
        format!("FRAME {}", selected.frame_id()),
        format!(
            "REASON {}",
            selected
                .reasons()
                .iter()
                .map(|reason| reason.as_str())
                .collect::<Vec<_>>()
                .join(" + ")
        ),
        marker_text,
        "SOURCE FRAME - AUTHORITATIVE".to_owned(),
    ];
    for (line, text) in lines.iter().enumerate() {
        draw_text(
            canvas,
            tile_x + 3,
            annotation_y + 2 + line as u32 * 12,
            &ellipsize(text, max_cells),
            if line == 0 { WHITE } else { MUTED },
        )?;
    }
    Ok(())
}

fn draw_timeline<F: Eq, M: Eq, G: Eq, P: AsRef<[u8]>>(
    canvas: &mut Canvas,
    y: u32,
    source: &FrameSequence<F, M, G, P>,
    width: u32,
) -> Result<()> {
    draw_text(canvas, 4, y + 6, "TIME ->", WHITE)?;
    let end = format!(
        "{} -> {}",
        format_time(source.range().start()),
        format_time(source.range().end())
    );
    let cells = usize::try_from(width / CELL_WIDTH).unwrap_or(0);
    let text_width = u32::try_from(end.chars().count())
        .unwrap_or(u32::MAX)
        .saturating_mul(CELL_WIDTH);
    let x = width.saturating_sub(text_width + 4);
    draw_text(canvas, x, y + 6, &ellipsize(&end, cells), MUTED)
}

fn format_time(timestamp: Timestamp) -> String {
    let milliseconds = timestamp.as_nanos() / 1_000_000;
    let micros = timestamp.as_nanos() % 1_000_000 / 1_000;
    format!("{milliseconds}.{micros:03} MS")
}

fn time_and_offset(timestamp: Timestamp, anchor: Timestamp) -> String {
    let offset = i128::from(timestamp.as_nanos()) - i128::from(anchor.as_nanos());
    let sign = if offset < 0 { '-' } else { '+' };
    let magnitude = offset.unsigned_abs();
    let milliseconds = magnitude / 1_000_000;
    let micros = magnitude % 1_000_000 / 1_000;
    format!(
        "T {} | {sign}{milliseconds}.{micros:03} MS",
        format_time(timestamp)
    )
}

fn assign_markers<F, M>(
    markers: &[Marker<M>],
    selection: &StoryboardSelection<F>,
) -> Vec<Vec<usize>> {
    let mut assignments = vec![Vec::new(); selection.selected_frames().len()];
    for (marker_index, marker) in markers.iter().enumerate() {
        let tile = selection
            .selected_frames()
            .iter()
            .position(|frame| frame.timestamp() >= marker.timestamp())
            .unwrap_or(selection.selected_frames().len() - 1);
        assignments[tile].push(marker_index);
    }
    assignments
}

fn gap_intersects<G>(gaps: &[DeclaredGap<G>], start: Timestamp, end: Timestamp) -> bool {
    gaps.iter()
        .any(|gap| gap.range().start() <= end && gap.range().end() >= start)
}

#[allow(clippy::too_many_arguments)]
fn finish_artifact<A, F, M, G, P>(
    artifact_id: A,
    kind: ArtifactKind,
    source: &FrameSequence<F, M, G, P>,
    normalized: &NormalizedSequence<F>,
    selected_ids: Vec<F>,
    selection: &StoryboardSelection<F>,
    marker_assignments: &[Vec<usize>],
    request: &StoryboardParameters,
    raster: Raster,
    layout_name: &'static str,
) -> Result<GeneratedArtifact<A, F, M, G>>
where
    F: Clone + Eq + Display,
    M: Clone + Eq,
    G: Clone + Eq,
    P: AsRef<[u8]>,
{
    let dimensions = raster.canvas.dimensions();
    let (bytes, hash) = crate::encode::encode_png(
        dimensions,
        raster.canvas.pixels(),
        request.limits.max_encoded_bytes(),
    )?;
    let mut normalization = normalized.normalization_steps().to_vec();
    normalization.push(request.measurement.provenance_step()?);
    normalization.push(display_conversion_step()?);
    let parameters = artifact_parameters(
        kind,
        selection,
        marker_assignments,
        request,
        raster.tile_width,
        raster.image_height,
        layout_name,
    )?;
    let manifest = ArtifactManifest::from_storyboard_sequence(
        artifact_id,
        kind,
        EvidenceClass::SourceDerived,
        {
            let descriptor = generator_descriptor(kind);
            AlgorithmDescriptor::new(descriptor.name, descriptor.version)?
        },
        source,
        selected_ids,
        selection.clone(),
        normalization,
        parameters,
        dimensions,
        hash,
    )?;
    Ok(GeneratedArtifact::new(
        EncodedImage::new(dimensions, bytes),
        manifest,
    ))
}

fn display_conversion_step() -> Result<NormalizationStep> {
    NormalizationStep::new(
        NormalizationKind::ColorSpaceConversion,
        "linear16-to-srgb8-v1",
        make_parameters([
            ("input", ParameterValue::Text("rgb16_linear_opaque".into())),
            ("output", ParameterValue::Text("rgb8_srgb_opaque".into())),
            (
                "mapping",
                ParameterValue::Text("nearest_checked_lut_entry".into()),
            ),
            (
                "tie_break",
                ParameterValue::Text("lower_encoded_value".into()),
            ),
        ])?,
    )
}

fn artifact_parameters<F>(
    kind: ArtifactKind,
    selection: &StoryboardSelection<F>,
    marker_assignments: &[Vec<usize>],
    request: &StoryboardParameters,
    tile_width: u32,
    image_height: u32,
    layout_name: &'static str,
) -> Result<Parameters>
where
    F: Display,
{
    let selected = selection
        .selected_frames()
        .iter()
        .map(|frame| {
            object([
                ("frame_index", unsigned_usize(frame.frame_index())?),
                (
                    "frame_label",
                    ParameterValue::Text(frame.frame_id().to_string().into()),
                ),
                (
                    "timestamp_nanos",
                    ParameterValue::Unsigned(frame.timestamp().as_nanos()),
                ),
                (
                    "reasons",
                    ParameterValue::List(
                        frame
                            .reasons()
                            .iter()
                            .map(|reason| ParameterValue::Text(reason.as_str().into()))
                            .collect(),
                    ),
                ),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let omitted = selection
        .omitted_anchors()
        .iter()
        .map(|anchor| {
            object([
                ("frame_index", unsigned_usize(anchor.frame_index())?),
                (
                    "reason",
                    ParameterValue::Text(anchor.reason().as_str().into()),
                ),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let marker_buckets = marker_assignments
        .iter()
        .enumerate()
        .map(|(tile, markers)| {
            object([
                ("tile_index", unsigned_usize(tile)?),
                (
                    "marker_declaration_indices",
                    ParameterValue::List(
                        markers
                            .iter()
                            .map(|index| unsigned_usize(*index))
                            .collect::<Result<Vec<_>>>()?,
                    ),
                ),
            ])
        })
        .collect::<Result<Vec<_>>>()?;
    let roles = object([
        (
            "before_source_index",
            unsigned_usize(selection.before_index())?,
        ),
        (
            "during_source_index",
            unsigned_usize(selection.during_index())?,
        ),
        (
            "after_source_index",
            unsigned_usize(selection.after_index())?,
        ),
        (
            "during_rule",
            ParameterValue::Text(during_label(selection).into()),
        ),
    ])?;
    let mut values = BTreeMap::new();
    for (key, value) in [
        (
            "algorithm_version",
            ParameterValue::Text(generator_descriptor(kind).version.into()),
        ),
        ("artifact_kind", ParameterValue::Text(kind.as_str().into())),
        (
            "anchor_nanos",
            ParameterValue::Unsigned(request.anchor.as_nanos()),
        ),
        (
            "tile_limit",
            ParameterValue::Unsigned(u64::from(request.tile_limit.get())),
        ),
        ("title", ParameterValue::Text(request.labels.title().into())),
        (
            "source_context",
            ParameterValue::Text(request.labels.source().into()),
        ),
        ("layout", ParameterValue::Text(layout_name.into())),
        (
            "tile_width",
            ParameterValue::Unsigned(u64::from(tile_width)),
        ),
        (
            "image_height",
            ParameterValue::Unsigned(u64::from(image_height)),
        ),
        (
            "preferred_tile_width",
            ParameterValue::Unsigned(u64::from(PREFERRED_TILE_WIDTH)),
        ),
        (
            "minimum_tile_width",
            ParameterValue::Unsigned(u64::from(MINIMUM_TILE_WIDTH)),
        ),
        (
            "max_output_width",
            ParameterValue::Unsigned(u64::from(request.limits.max_width())),
        ),
        (
            "max_output_height",
            ParameterValue::Unsigned(u64::from(request.limits.max_height())),
        ),
        (
            "max_canvas_bytes",
            unsigned_usize(request.limits.max_canvas_bytes())?,
        ),
        (
            "max_encoded_bytes",
            unsigned_usize(request.limits.max_encoded_bytes())?,
        ),
        (
            "scale_kernel",
            ParameterValue::Text("nearest_neighbor_integer_center".into()),
        ),
        (
            "source_fit",
            ParameterValue::Text("contain_preserve_aspect".into()),
        ),
        (
            "font",
            ParameterValue::Text("embedded-5x7-in-6x10-ascii-v1".into()),
        ),
        (
            "text_policy",
            ParameterValue::Text("escaped_ascii_middle_notice_exact_text_in_manifest".into()),
        ),
        (
            "png",
            ParameterValue::Text("png-0.17.16-rgb8-best-no_filter-no_chunks".into()),
        ),
        (
            "continuity_segments",
            unsigned_usize(selection.continuity_segment_count())?,
        ),
        ("selected", ParameterValue::List(selected)),
        ("omitted_anchors", ParameterValue::List(omitted)),
        ("marker_assignments", ParameterValue::List(marker_buckets)),
        ("orientation_roles", roles),
    ] {
        values.insert(key.into(), value);
    }
    Parameters::new(values)
}

fn unsigned_usize(value: usize) -> Result<ParameterValue> {
    Ok(ParameterValue::Unsigned(
        u64::try_from(value).map_err(|_| canvas_limit_error())?,
    ))
}

fn object<const N: usize>(entries: [(&'static str, ParameterValue); N]) -> Result<ParameterValue> {
    let map: BTreeMap<Box<str>, ParameterValue> = entries
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect();
    Parameters::new(map.clone())?;
    Ok(ParameterValue::Object(map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_layout_rejects_small_tiles_height_and_canvas() {
        let dimensions = PixelDimensions::new(16, 9).unwrap();
        let small_width = RenderLimits::new(
            NonZeroU32::new(479).unwrap(),
            NonZeroU32::new(4096).unwrap(),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).unwrap(),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).unwrap(),
        );
        assert!(checked_layout(3, dimensions, STORY_ANNOTATION_HEIGHT, small_width).is_err());
        let small_height = RenderLimits::new(
            NonZeroU32::new(720).unwrap(),
            NonZeroU32::new(100).unwrap(),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).unwrap(),
            NonZeroUsize::new(DEFAULT_MAX_BYTES).unwrap(),
        );
        assert!(checked_layout(3, dimensions, STORY_ANNOTATION_HEIGHT, small_height).is_err());
    }

    #[test]
    fn gap_intersection_includes_equal_boundaries() {
        let gap = DeclaredGap::new(
            1_u8,
            crate::TimeRange::new(Timestamp::from_nanos(2), Timestamp::from_nanos(2)).unwrap(),
            "loss",
            None,
        )
        .unwrap();
        assert!(gap_intersects(
            &[gap],
            Timestamp::from_nanos(1),
            Timestamp::from_nanos(2)
        ));
    }
}
