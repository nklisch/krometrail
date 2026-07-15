use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    ContractError, Result, canonical,
    conditions::{EvidenceCondition, canonical_conditions},
    matrix::MatrixDefinition,
    prompts::PromptSet,
    vocabulary::ScoringVocabulary,
};

pub const BENCHMARK_SCHEMA_VERSION: u16 = 1;
pub const BENCHMARK_ID: &str = "temporal-advantage-corpus-v1";
pub const FIXTURE_NAME: &str = "temporal-benchmark";
pub const FIXTURE_ROOT: &str = "tests/fixtures/browser/temporal-benchmark";
pub const VIEWPORT_WIDTH: u32 = 800;
pub const VIEWPORT_HEIGHT: u32 = 450;
pub const DEVICE_SCALE_FACTOR_MILLI: u16 = 1_000;
pub const DURATIONS_MS: [u16; 5] = [16, 33, 50, 100, 200];

const FIXTURE_FILES: [&str; 4] = ["README.md", "benchmark.css", "benchmark.js", "index.html"];

// These hashes are part of the current v1 definition. Contract tests recompute them from the
// committed target files, so changing a fixture requires an intentional definition update.
const FIXTURE_FILE_SHA256: [&str; 4] = [
    "sha256:440ebde2a44869fd05ad266bef778d294ca5a94108b124f1fd4afb676b29f314",
    "sha256:e098d0c7eb95f9f3dd2d268ed820197820242dbb7c15f900cacffb9b4a9c7d2c",
    "sha256:aea769bb28dff5317d143485c366774d0805ec1e789d01cf7118328a572553a7",
    "sha256:23da2695cb0b0e164b2a181e5436f31cd59ee42696933f7c7015bccd4648cabb",
];

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum CaseFamily {
    MovementReversal,
    Flicker,
    TransientLayout,
    DomOpaqueMotion,
    StableControl,
}

impl CaseFamily {
    pub const ALL: [Self; 5] = [
        Self::MovementReversal,
        Self::Flicker,
        Self::TransientLayout,
        Self::DomOpaqueMotion,
        Self::StableControl,
    ];
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CaseIntent {
    Defect,
    Intentional,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DurationMode {
    DefectInterval,
    TransitionInterval,
    ObservationWindow,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhaseBoundary {
    Zero,
    OffsetMs { value: u32 },
    OffsetPlusDurationMs { offset_ms: u32 },
    End,
}

impl PhaseBoundary {
    pub fn resolve_for_duration(self, duration_ms: u16) -> Option<u32> {
        match self {
            Self::Zero => Some(0),
            Self::OffsetMs { value } => Some(value),
            Self::OffsetPlusDurationMs { offset_ms } => Some(offset_ms + u32::from(duration_ms)),
            Self::End => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhaseDefinition {
    pub id: String,
    pub state_id: String,
    pub start: PhaseBoundary,
    pub end: PhaseBoundary,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimeInterval {
    pub start: PhaseBoundary,
    pub end: PhaseBoundary,
}

/// An affected extent in the fixed qualification viewport.
///
/// Coordinates are integer captured-viewport pixels with the origin at the screenshot's top-left
/// corner. The rectangle is half-open (`x..x + width`, `y..y + height`) in the 800x450,
/// device-scale-one contract; it is never a stage-relative offset or a canvas-local drawing
/// coordinate. The extent is the union of the affected fixture subject's visible extents across
/// its declared phases, clipped by the fixture's viewport/stage visibility. A moving subject
/// therefore covers its complete path, and a DOM-opaque canvas uses its complete viewport canvas
/// box so a scorer has one exact localization ROI.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(
    description = "Half-open affected extent in fixed 800x450 viewport pixels, origin at the top-left; not stage-relative or canvas-local coordinates."
)]
pub struct Rect {
    /// Horizontal viewport-pixel origin of the half-open rectangle.
    pub x: u32,
    /// Vertical viewport-pixel origin of the half-open rectangle.
    pub y: u32,
    /// Extent width in viewport pixels.
    pub width: u32,
    /// Extent height in viewport pixels.
    pub height: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TimingDefinition {
    pub lead_in_ms: u32,
    pub settle_ms: u32,
    pub duration_mode: DurationMode,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaseDefinition {
    pub case_id: String,
    pub family: CaseFamily,
    pub intent: CaseIntent,
    pub variant: String,
    pub anchor_id: String,
    pub timing: TimingDefinition,
    pub phases: Vec<PhaseDefinition>,
    pub defect_interval: Option<TimeInterval>,
    /// The exact captured-viewport-pixel ROI consumed by localization scoring.
    pub affected_region: Rect,
    pub final_state_id: String,
    /// Evaluator-owned truth withheld from condition packages and model-facing prompts.
    pub ground_truth: GroundTruthDefinition,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GroundTruthDefinition {
    pub temporary_state: crate::AnswerTruth,
    pub state_order: Vec<crate::StateLabel>,
    pub affected_region: Rect,
    pub motion_behavior: crate::MotionBehavior,
    pub judgment: crate::Judgment,
}

impl GroundTruthDefinition {
    pub fn validate(&self) -> Result<()> {
        if self.state_order.is_empty() || self.state_order.len() > 8 {
            return Err(ContractError::new(
                "ground truth state_order must contain between one and eight labels",
            ));
        }
        let mut states = BTreeSet::new();
        if self
            .state_order
            .iter()
            .any(|state| *state == crate::StateLabel::Unknown || !states.insert(state))
        {
            return Err(ContractError::new(
                "ground truth state_order labels must be unique and known",
            ));
        }
        if self.temporary_state == crate::AnswerTruth::Uncertain
            || self.motion_behavior == crate::MotionBehavior::Uncertain
            || self.judgment == crate::Judgment::Uncertain
        {
            return Err(ContractError::new(
                "ground truth cannot contain uncertain values",
            ));
        }
        validate_rect(&self.affected_region, "ground truth affected_region")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureFile {
    pub path: String,
    pub sha256: String,
}

impl FixtureFile {
    pub fn from_bytes(path: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        let path = path.into();
        validate_relative_file_path(&path)?;
        Ok(Self {
            path,
            sha256: crate::sha256_prefixed(bytes),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FixtureIdentity {
    pub name: String,
    pub root_relative_path: String,
    pub files: Vec<FixtureFile>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InputIdentities {
    pub matrix_sha256: String,
    pub conditions_sha256: String,
    pub prompts_sha256: String,
    pub scoring_sha256: String,
}

impl InputIdentities {
    fn from_parts(
        matrix: &MatrixDefinition,
        conditions: &[EvidenceCondition],
        prompts: &PromptSet,
        scoring: &ScoringVocabulary,
    ) -> Result<Self> {
        Ok(Self {
            matrix_sha256: crate::sha256_prefixed(&canonical::canonical_json(matrix)?),
            conditions_sha256: crate::sha256_prefixed(&canonical::canonical_json(conditions)?),
            prompts_sha256: crate::sha256_prefixed(&canonical::canonical_json(prompts)?),
            scoring_sha256: crate::sha256_prefixed(&canonical::canonical_json(scoring)?),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkDefinition {
    pub schema_version: u16,
    pub benchmark_id: String,
    pub fixture: FixtureIdentity,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub device_scale_factor_milli: u16,
    pub duration_ms: Vec<u16>,
    pub cases: Vec<CaseDefinition>,
    pub matrix: MatrixDefinition,
    pub conditions: Vec<EvidenceCondition>,
    pub prompts: PromptSet,
    pub scoring: ScoringVocabulary,
    pub input_identities: InputIdentities,
}

impl BenchmarkDefinition {
    pub fn canonical() -> Self {
        let matrix = MatrixDefinition::canonical();
        let conditions = canonical_conditions();
        let prompts = PromptSet::canonical();
        let scoring = ScoringVocabulary::canonical();
        let input_identities =
            InputIdentities::from_parts(&matrix, &conditions, &prompts, &scoring)
                .expect("canonical benchmark inputs must hash");
        Self {
            schema_version: BENCHMARK_SCHEMA_VERSION,
            benchmark_id: BENCHMARK_ID.into(),
            fixture: expected_fixture(),
            viewport_width: VIEWPORT_WIDTH,
            viewport_height: VIEWPORT_HEIGHT,
            device_scale_factor_milli: DEVICE_SCALE_FACTOR_MILLI,
            duration_ms: DURATIONS_MS.to_vec(),
            cases: expected_cases(),
            matrix,
            conditions,
            prompts,
            scoring,
            input_identities,
        }
    }

    pub fn from_canonical_json(bytes: &[u8]) -> Result<Self> {
        let definition: Self = serde_json::from_slice(bytes)?;
        definition.validate()?;
        canonical::require_canonical(bytes, &definition)?;
        Ok(definition)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != BENCHMARK_SCHEMA_VERSION {
            return Err(ContractError::new(format!(
                "schema_version must be {BENCHMARK_SCHEMA_VERSION}"
            )));
        }
        if self.benchmark_id != BENCHMARK_ID {
            return Err(ContractError::new(format!(
                "benchmark_id must be {BENCHMARK_ID}"
            )));
        }
        if self.viewport_width != VIEWPORT_WIDTH || self.viewport_height != VIEWPORT_HEIGHT {
            return Err(ContractError::new("viewport must be exactly 800x450"));
        }
        if self.device_scale_factor_milli != DEVICE_SCALE_FACTOR_MILLI {
            return Err(ContractError::new(
                "device_scale_factor_milli must be exactly 1000",
            ));
        }
        if self.duration_ms != DURATIONS_MS {
            return Err(ContractError::new(
                "duration_ms must be exactly [16, 33, 50, 100, 200]",
            ));
        }
        validate_fixture(&self.fixture)?;

        let expected_matrix = MatrixDefinition::canonical();
        if self.matrix != expected_matrix {
            return Err(ContractError::new(
                "matrix does not match the current deterministic trial contract",
            ));
        }
        self.matrix.validate()?;
        let expected_conditions = canonical_conditions();
        if self.conditions != expected_conditions {
            return Err(ContractError::new(
                "conditions do not match the current A-E evidence contract",
            ));
        }
        for condition in &self.conditions {
            condition.validate()?;
        }
        let expected_prompts = PromptSet::canonical();
        if self.prompts != expected_prompts {
            return Err(ContractError::new(
                "prompts do not match the current model-facing contract",
            ));
        }
        self.prompts.validate()?;
        let expected_scoring = ScoringVocabulary::canonical();
        if self.scoring != expected_scoring {
            return Err(ContractError::new(
                "scoring vocabulary does not match the current contract",
            ));
        }
        self.scoring.validate()?;
        let expected_identities = InputIdentities::from_parts(
            &self.matrix,
            &self.conditions,
            &self.prompts,
            &self.scoring,
        )?;
        if self.input_identities != expected_identities {
            return Err(ContractError::new(
                "input identities do not match their canonical contract inputs",
            ));
        }

        let expected = expected_cases();
        if self.cases != expected {
            return Err(ContractError::new(
                "cases do not match the current canonical case and phase registry",
            ));
        }
        for case in &self.cases {
            validate_case(case, &self.duration_ms)?;
        }
        Ok(())
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        canonical::canonical_json(self)
    }

    pub fn definition_digest(&self) -> Result<String> {
        Ok(crate::sha256_prefixed(&self.canonical_bytes()?))
    }

    pub fn case(&self, case_id: &str) -> Option<&CaseDefinition> {
        self.cases.iter().find(|case| case.case_id == case_id)
    }

    pub fn supports_duration(&self, duration_ms: u16) -> bool {
        self.duration_ms.contains(&duration_ms)
    }
}

fn validate_rect(rect: &Rect, label: &str) -> Result<()> {
    if rect.width == 0
        || rect.height == 0
        || rect.x > VIEWPORT_WIDTH
        || rect.y > VIEWPORT_HEIGHT
        || rect.width > VIEWPORT_WIDTH - rect.x
        || rect.height > VIEWPORT_HEIGHT - rect.y
    {
        return Err(ContractError::new(format!(
            "{label} must be a non-empty rectangle within the 800x450 viewport"
        )));
    }
    Ok(())
}

fn validate_fixture(fixture: &FixtureIdentity) -> Result<()> {
    if fixture.name != FIXTURE_NAME || fixture.root_relative_path != FIXTURE_ROOT {
        return Err(ContractError::new(
            "fixture identity does not match the canonical target",
        ));
    }
    if fixture.files.len() != FIXTURE_FILES.len() {
        return Err(ContractError::new(
            "fixture file identity list has the wrong length",
        ));
    }
    let mut paths = BTreeSet::new();
    for (index, file) in fixture.files.iter().enumerate() {
        validate_relative_file_path(&file.path)?;
        validate_sha256(&file.sha256)?;
        if !paths.insert(&file.path) {
            return Err(ContractError::new("fixture file paths must be unique"));
        }
        if file.path != FIXTURE_FILES[index] || file.sha256 != FIXTURE_FILE_SHA256[index] {
            return Err(ContractError::new(
                "fixture file identities do not match the current canonical target",
            ));
        }
    }
    Ok(())
}

fn validate_case(case: &CaseDefinition, durations: &[u16]) -> Result<()> {
    if case.anchor_id != "run" {
        return Err(ContractError::new(format!(
            "{} must use the run interaction anchor",
            case.case_id
        )));
    }
    if case.phases.is_empty() {
        return Err(ContractError::new(format!(
            "{} must declare at least one phase",
            case.case_id
        )));
    }
    validate_rect(
        &case.affected_region,
        &format!("{} affected_region", case.case_id),
    )?;
    case.ground_truth.validate()?;
    if case.ground_truth.affected_region != case.affected_region {
        return Err(ContractError::new(format!(
            "{} ground truth ROI must match the corrected case ROI",
            case.case_id
        )));
    }
    let mut phase_ids = BTreeSet::new();
    let mut state_ids = BTreeSet::new();
    for phase in &case.phases {
        if phase.id.is_empty() || phase.state_id.is_empty() {
            return Err(ContractError::new(format!(
                "{} has an empty phase or state ID",
                case.case_id
            )));
        }
        if !phase_ids.insert(&phase.id) {
            return Err(ContractError::new(format!(
                "{} phase IDs must be unique",
                case.case_id
            )));
        }
        state_ids.insert(&phase.state_id);
    }
    if !state_ids.contains(&case.final_state_id) {
        return Err(ContractError::new(format!(
            "{} final state is not declared by a phase",
            case.case_id
        )));
    }

    for duration_ms in durations {
        let first = case.phases.first().expect("non-empty phases");
        if first.start.resolve_for_duration(*duration_ms) != Some(0) {
            return Err(ContractError::new(format!(
                "{} phases must start at zero",
                case.case_id
            )));
        }
        for (index, phase) in case.phases.iter().enumerate() {
            let start = phase.start.resolve_for_duration(*duration_ms);
            let end = phase.end.resolve_for_duration(*duration_ms);
            if end.is_none() && index + 1 != case.phases.len() {
                return Err(ContractError::new(format!(
                    "{} may use an open-ended phase only at the end",
                    case.case_id
                )));
            }
            if let (Some(start), Some(end)) = (start, end)
                && end <= start
            {
                return Err(ContractError::new(format!(
                    "{} phase {} is not a positive interval",
                    case.case_id, phase.id
                )));
            }
            if let Some(next) = case.phases.get(index + 1)
                && end != next.start.resolve_for_duration(*duration_ms)
            {
                return Err(ContractError::new(format!(
                    "{} phase intervals are not contiguous",
                    case.case_id
                )));
            }
        }

        if let Some(interval) = &case.defect_interval {
            let Some(start) = interval.start.resolve_for_duration(*duration_ms) else {
                return Err(ContractError::new(format!(
                    "{} defect interval cannot start at the end",
                    case.case_id
                )));
            };
            let Some(end) = interval.end.resolve_for_duration(*duration_ms) else {
                return Err(ContractError::new(format!(
                    "{} defect interval must have a finite end",
                    case.case_id
                )));
            };
            if end <= start {
                return Err(ContractError::new(format!(
                    "{} defect interval is not positive",
                    case.case_id
                )));
            }
        }
    }

    match (case.intent, case.defect_interval.is_some()) {
        (CaseIntent::Defect, true) => {}
        (CaseIntent::Defect, false) => {
            return Err(ContractError::new(format!(
                "defect case {} must declare a defect interval",
                case.case_id
            )));
        }
        (CaseIntent::Intentional, false) => {}
        (CaseIntent::Intentional, true) => {
            return Err(ContractError::new(format!(
                "intentional case {} cannot declare a defect interval",
                case.case_id
            )));
        }
    }
    Ok(())
}

fn validate_relative_file_path(path: &str) -> Result<()> {
    if path.is_empty()
        || path.starts_with('/')
        || path.contains('\\')
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ContractError::new(format!(
            "fixture path is not a relative POSIX path: {path}"
        )));
    }
    Ok(())
}

fn validate_sha256(value: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        return Err(ContractError::new(
            "fixture hashes must use sha256:<64 hex>",
        ));
    };
    if hex.len() != 64 || hex.bytes().any(|byte| !byte.is_ascii_hexdigit()) {
        return Err(ContractError::new(
            "fixture hashes must use 64 hexadecimal characters",
        ));
    }
    if hex.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(ContractError::new(
            "fixture hashes must use lowercase hexadecimal",
        ));
    }
    Ok(())
}

fn expected_fixture() -> FixtureIdentity {
    FixtureIdentity {
        name: FIXTURE_NAME.into(),
        root_relative_path: FIXTURE_ROOT.into(),
        files: FIXTURE_FILES
            .into_iter()
            .zip(FIXTURE_FILE_SHA256)
            .map(|(path, sha256)| FixtureFile {
                path: path.into(),
                sha256: sha256.into(),
            })
            .collect(),
    }
}

fn phase(id: &str, state_id: &str, start: PhaseBoundary, end: PhaseBoundary) -> PhaseDefinition {
    PhaseDefinition {
        id: id.into(),
        state_id: state_id.into(),
        start,
        end,
    }
}

fn defect_interval(start: u32) -> TimeInterval {
    TimeInterval {
        start: PhaseBoundary::OffsetMs { value: start },
        end: PhaseBoundary::OffsetPlusDurationMs { offset_ms: start },
    }
}

// The canonical registry is deliberately explicit: each field makes one part of a case's
// externally validated contract visible at the call site.
#[allow(clippy::too_many_arguments)]
fn case(
    case_id: &str,
    family: CaseFamily,
    intent: CaseIntent,
    variant: &str,
    duration_mode: DurationMode,
    lead_in_ms: u32,
    settle_ms: u32,
    phases: Vec<PhaseDefinition>,
    defect_interval: Option<TimeInterval>,
    affected_region: Rect,
    final_state_id: &str,
    ground_truth: GroundTruthDefinition,
) -> CaseDefinition {
    CaseDefinition {
        case_id: case_id.into(),
        family,
        intent,
        variant: variant.into(),
        anchor_id: "run".into(),
        timing: TimingDefinition {
            lead_in_ms,
            settle_ms,
            duration_mode,
        },
        phases,
        defect_interval,
        affected_region,
        final_state_id: final_state_id.into(),
        ground_truth,
    }
}

fn ground_truth(
    temporary_state: crate::AnswerTruth,
    state_order: &[crate::StateLabel],
    affected_region: Rect,
    motion_behavior: crate::MotionBehavior,
    judgment: crate::Judgment,
) -> GroundTruthDefinition {
    GroundTruthDefinition {
        temporary_state,
        state_order: state_order.to_vec(),
        affected_region,
        motion_behavior,
        judgment,
    }
}

fn expected_cases() -> Vec<CaseDefinition> {
    use crate::{AnswerTruth, Judgment, MotionBehavior, StateLabel};
    use CaseFamily::{DomOpaqueMotion, Flicker, MovementReversal, StableControl, TransientLayout};
    use CaseIntent::{Defect, Intentional};
    use DurationMode::{DefectInterval, ObservationWindow, TransitionInterval};
    use PhaseBoundary::{End, OffsetMs, OffsetPlusDurationMs, Zero};

    vec![
        case(
            "movement-reversal/basic",
            MovementReversal,
            Defect,
            "basic",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "movement.baseline",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "forward",
                    "movement.forward",
                    OffsetMs { value: 100 },
                    OffsetMs { value: 200 },
                ),
                phase(
                    "reversal",
                    "movement.reversal",
                    OffsetMs { value: 200 },
                    OffsetPlusDurationMs { offset_ms: 200 },
                ),
                phase(
                    "correction",
                    "movement.correction",
                    OffsetPlusDurationMs { offset_ms: 200 },
                    OffsetPlusDurationMs { offset_ms: 300 },
                ),
                phase(
                    "stable",
                    "movement.stable",
                    OffsetPlusDurationMs { offset_ms: 300 },
                    End,
                ),
            ],
            Some(defect_interval(200)),
            Rect {
                x: 49,
                y: 73,
                width: 480,
                height: 120,
            },
            "movement.stable",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 49,
                    y: 73,
                    width: 480,
                    height: 120,
                },
                MotionBehavior::Reversal,
                Judgment::Defective,
            ),
        ),
        case(
            "flicker/visibility",
            Flicker,
            Defect,
            "visibility",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "flicker.visibility.baseline",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-visibility",
                    "flicker.visibility.absent",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "flicker.visibility.ready",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
            "flicker.visibility.ready",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 361,
                    y: 73,
                    width: 240,
                    height: 120,
                },
                MotionBehavior::Flicker,
                Judgment::Defective,
            ),
        ),
        case(
            "flicker/color",
            Flicker,
            Defect,
            "color",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "flicker.color.neutral",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-color",
                    "flicker.color.incorrect",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "flicker.color.neutral",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
            "flicker.color.neutral",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 361,
                    y: 73,
                    width: 240,
                    height: 120,
                },
                MotionBehavior::Flicker,
                Judgment::Defective,
            ),
        ),
        case(
            "flicker/text",
            Flicker,
            Defect,
            "text",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "flicker.text.ready",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-text",
                    "flicker.text.stale",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "flicker.text.ready",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
            "flicker.text.ready",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 361,
                    y: 73,
                    width: 240,
                    height: 120,
                },
                MotionBehavior::Flicker,
                Judgment::Defective,
            ),
        ),
        case(
            "layout/width",
            TransientLayout,
            Defect,
            "width",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "layout.width.stable",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-width",
                    "layout.width.narrow",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "layout.width.stable",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 49,
                y: 241,
                width: 640,
                height: 160,
            },
            "layout.width.stable",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 49,
                    y: 241,
                    width: 640,
                    height: 160,
                },
                MotionBehavior::LayoutShift,
                Judgment::Defective,
            ),
        ),
        case(
            "layout/content-shift",
            TransientLayout,
            Defect,
            "content_shift",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "layout.content_shift.stable",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-shift",
                    "layout.content_shift.notice",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "layout.content_shift.stable",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 49,
                y: 223,
                width: 640,
                height: 202,
            },
            "layout.content_shift.stable",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 49,
                    y: 223,
                    width: 640,
                    height: 202,
                },
                MotionBehavior::LayoutShift,
                Judgment::Defective,
            ),
        ),
        case(
            "layout/scroll-position",
            TransientLayout,
            Defect,
            "scroll_position",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "layout.scroll_position.top",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-scroll",
                    "layout.scroll_position.jumped",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "layout.scroll_position.top",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 49,
                y: 241,
                width: 320,
                height: 120,
            },
            "layout.scroll_position.top",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 49,
                    y: 241,
                    width: 320,
                    height: 120,
                },
                MotionBehavior::LayoutShift,
                Judgment::Defective,
            ),
        ),
        case(
            "dom-opaque/path-reversal",
            DomOpaqueMotion,
            Defect,
            "path_reversal",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "dom_opaque.path.baseline",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "forward",
                    "dom_opaque.path.forward",
                    OffsetMs { value: 100 },
                    OffsetMs { value: 200 },
                ),
                phase(
                    "reversal",
                    "dom_opaque.path.reversal",
                    OffsetMs { value: 200 },
                    OffsetPlusDurationMs { offset_ms: 200 },
                ),
                phase(
                    "correction",
                    "dom_opaque.path.correction",
                    OffsetPlusDurationMs { offset_ms: 200 },
                    OffsetPlusDurationMs { offset_ms: 300 },
                ),
                phase(
                    "stable",
                    "dom_opaque.path.final",
                    OffsetPlusDurationMs { offset_ms: 300 },
                    End,
                ),
            ],
            Some(defect_interval(200)),
            Rect {
                x: 401,
                y: 241,
                width: 320,
                height: 160,
            },
            "dom_opaque.path.final",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 401,
                    y: 241,
                    width: 320,
                    height: 160,
                },
                MotionBehavior::Reversal,
                Judgment::Defective,
            ),
        ),
        case(
            "dom-opaque/teleport",
            DomOpaqueMotion,
            Defect,
            "teleport",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "dom_opaque.teleport.baseline",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-teleport",
                    "dom_opaque.teleport.wrong",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "dom_opaque.teleport.final",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 401,
                y: 241,
                width: 320,
                height: 160,
            },
            "dom_opaque.teleport.final",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 401,
                    y: 241,
                    width: 320,
                    height: 160,
                },
                MotionBehavior::Teleport,
                Judgment::Defective,
            ),
        ),
        case(
            "dom-opaque/sprite",
            DomOpaqueMotion,
            Defect,
            "sprite",
            DefectInterval,
            100,
            100,
            vec![
                phase(
                    "baseline",
                    "dom_opaque.sprite.baseline",
                    Zero,
                    OffsetMs { value: 100 },
                ),
                phase(
                    "incorrect-sprite",
                    "dom_opaque.sprite.wrong",
                    OffsetMs { value: 100 },
                    OffsetPlusDurationMs { offset_ms: 100 },
                ),
                phase(
                    "stable",
                    "dom_opaque.sprite.final",
                    OffsetPlusDurationMs { offset_ms: 100 },
                    End,
                ),
            ],
            Some(defect_interval(100)),
            Rect {
                x: 401,
                y: 241,
                width: 320,
                height: 160,
            },
            "dom_opaque.sprite.final",
            ground_truth(
                AnswerTruth::Yes,
                &[StateLabel::Baseline, StateLabel::Changed, StateLabel::Final],
                Rect {
                    x: 401,
                    y: 241,
                    width: 320,
                    height: 160,
                },
                MotionBehavior::Flicker,
                Judgment::Defective,
            ),
        ),
        case(
            "stable/smooth-panel",
            StableControl,
            Intentional,
            "smooth_panel",
            TransitionInterval,
            0,
            0,
            vec![
                phase(
                    "transition",
                    "stable.smooth_panel.moving",
                    Zero,
                    OffsetPlusDurationMs { offset_ms: 0 },
                ),
                phase(
                    "stable",
                    "stable.smooth_panel.final",
                    OffsetPlusDurationMs { offset_ms: 0 },
                    End,
                ),
            ],
            None,
            Rect {
                x: 49,
                y: 73,
                width: 480,
                height: 120,
            },
            "stable.smooth_panel.final",
            ground_truth(
                AnswerTruth::No,
                &[StateLabel::IntentionalMotion, StateLabel::Final],
                Rect {
                    x: 49,
                    y: 73,
                    width: 480,
                    height: 120,
                },
                MotionBehavior::Monotonic,
                Judgment::Intentional,
            ),
        ),
        case(
            "stable/loading-indicator",
            StableControl,
            Intentional,
            "loading_indicator",
            TransitionInterval,
            0,
            0,
            vec![
                phase(
                    "loading",
                    "stable.loading_indicator.loading",
                    Zero,
                    OffsetPlusDurationMs { offset_ms: 0 },
                ),
                phase(
                    "stable",
                    "stable.loading_indicator.final",
                    OffsetPlusDurationMs { offset_ms: 0 },
                    End,
                ),
            ],
            None,
            Rect {
                x: 361,
                y: 73,
                width: 240,
                height: 120,
            },
            "stable.loading_indicator.final",
            ground_truth(
                AnswerTruth::No,
                &[StateLabel::IntentionalMotion, StateLabel::Final],
                Rect {
                    x: 361,
                    y: 73,
                    width: 240,
                    height: 120,
                },
                MotionBehavior::None,
                Judgment::Intentional,
            ),
        ),
        case(
            "stable/caret",
            StableControl,
            Intentional,
            "caret",
            ObservationWindow,
            0,
            0,
            vec![phase("ready", "stable.caret.ready", Zero, End)],
            None,
            Rect {
                x: 49,
                y: 381,
                width: 300,
                height: 32,
            },
            "stable.caret.ready",
            ground_truth(
                AnswerTruth::No,
                &[StateLabel::IntentionalMotion],
                Rect {
                    x: 49,
                    y: 381,
                    width: 300,
                    height: 32,
                },
                MotionBehavior::None,
                Judgment::Intentional,
            ),
        ),
    ]
}
