use std::{path::PathBuf, sync::Arc};

use krometrail_core::{
    BrowserConnector, CAPABILITY_REGISTRY, CapabilityDefault, CapabilityId, ProgressiveEvidence,
    Result, TemporalContextQuery, TemporalDebugBundles, validate_capability_selection,
};

#[derive(Clone)]
pub struct McpDependencies {
    pub browser: Arc<dyn BrowserConnector>,
    pub temporal_debug_bundles: Arc<dyn TemporalDebugBundles>,
    pub progressive_evidence: Arc<dyn ProgressiveEvidence>,
    pub temporal_context: Arc<dyn TemporalContextQuery>,
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
    enabled_capabilities: Arc<[CapabilityId]>,
}

impl McpConfig {
    pub fn new(enabled_capabilities: Vec<CapabilityId>) -> Result<Self> {
        validate_capability_selection(&enabled_capabilities)?;
        Ok(Self {
            enabled_capabilities: enabled_capabilities.into(),
        })
    }

    pub fn is_enabled(&self, capability: CapabilityId) -> bool {
        self.enabled_capabilities.contains(&capability)
    }

    pub fn enabled_capabilities(&self) -> &[CapabilityId] {
        &self.enabled_capabilities
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        let enabled = CAPABILITY_REGISTRY
            .iter()
            .filter(|definition| definition.default == CapabilityDefault::Enabled)
            .map(|definition| definition.id)
            .collect();
        Self::new(enabled).expect("default capability registry selection is valid")
    }
}
