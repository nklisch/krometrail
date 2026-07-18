use std::{path::PathBuf, sync::Arc};

use krometrail_core::{
    BrowserConnector, CapabilityId, CapabilitySnapshot, ProgressiveEvidence, ResolvedRangeHandles,
    Result, TemporalContextQuery, TemporalDebugBundles, TemporalVideoGeneration,
};

#[derive(Clone)]
pub struct McpDependencies {
    pub browser: Arc<dyn BrowserConnector>,
    pub temporal_debug_bundles: Arc<dyn TemporalDebugBundles>,
    pub progressive_evidence: Arc<dyn ProgressiveEvidence>,
    pub temporal_context: Arc<dyn TemporalContextQuery>,
    pub range_handles: Arc<dyn ResolvedRangeHandles>,
    pub temporal_video: Option<Arc<dyn TemporalVideoGeneration>>,
    pub diagnostics: DiagnosticContext,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct DiagnosticContext {
    log_path: Option<PathBuf>,
}

impl DiagnosticContext {
    pub fn new(log_path: Option<PathBuf>) -> Self {
        Self { log_path }
    }

    pub fn log_path(&self) -> Option<&std::path::Path> {
        self.log_path.as_deref()
    }
}

#[derive(Clone, Debug)]
pub struct McpConfig {
    snapshot: CapabilitySnapshot,
}

impl McpConfig {
    pub fn new(enabled_capabilities: Vec<CapabilityId>) -> Result<Self> {
        Ok(Self::from_snapshot(CapabilitySnapshot::resolve_explicit(
            enabled_capabilities,
            &[],
        )?))
    }

    pub fn from_snapshot(snapshot: CapabilitySnapshot) -> Self {
        Self { snapshot }
    }

    pub fn is_enabled(&self, capability: CapabilityId) -> bool {
        self.snapshot.is_enabled(capability)
    }

    pub fn enabled_capabilities(&self) -> &[CapabilityId] {
        self.snapshot.enabled_capabilities()
    }

    pub fn snapshot(&self) -> &CapabilitySnapshot {
        &self.snapshot
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self::from_snapshot(
            CapabilitySnapshot::resolve_defaults(&[])
                .expect("default capability registry selection is valid"),
        )
    }
}
