use sha2::{Digest, Sha256};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdapterFailureStage {
    InputStaging,
    Spawn,
    ProcessWait,
    ProcessCleanup,
    OutputValidation,
    ExecutableIdentity,
    VersionProbe,
    EncodeProbe,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AdapterFailureKind {
    Cancelled,
    Deadline,
    Spawn,
    ProcessExit,
    ProcessIo,
    ProcessCleanup,
    StdoutOverflow,
    DiagnosticOverflow,
    OutputOverflow,
    InvalidOutput,
    UnrepresentableTiming,
    InvalidCandidate,
    ChangedCandidate,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("FFmpeg adapter failed at a sanitized stage")]
pub(crate) struct AdapterFailure {
    pub(crate) stage: AdapterFailureStage,
    pub(crate) kind: AdapterFailureKind,
    pub(crate) observed_bytes: Option<u64>,
    pub(crate) diagnostic_sha256: Option<[u8; 32]>,
}

impl AdapterFailure {
    pub(crate) const fn new(stage: AdapterFailureStage, kind: AdapterFailureKind) -> Self {
        Self {
            stage,
            kind,
            observed_bytes: None,
            diagnostic_sha256: None,
        }
    }

    pub(crate) fn with_bytes(mut self, bytes: &[u8]) -> Self {
        self.observed_bytes = Some(bytes.len() as u64);
        self.diagnostic_sha256 = Some(Sha256::digest(bytes).into());
        self
    }

    pub(crate) const fn with_observed_bytes(mut self, bytes: u64) -> Self {
        self.observed_bytes = Some(bytes);
        self
    }

    pub(crate) const fn at_stage(mut self, stage: AdapterFailureStage) -> Self {
        if !matches!(self.stage, AdapterFailureStage::ProcessCleanup) {
            self.stage = stage;
        }
        self
    }

    pub(crate) fn trace(&self) {
        let digest = self.diagnostic_sha256.map(HexDigest);
        tracing::debug!(
            event = "ffmpeg.adapter.failure",
            failure_stage = ?self.stage,
            failure_kind = ?self.kind,
            observed_bytes = self.observed_bytes,
            diagnostic_sha256 = digest.as_ref().map(ToString::to_string),
            "FFmpeg adapter operation failed"
        );
    }
}

struct HexDigest([u8; 32]);

impl std::fmt::Display for HexDigest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}
