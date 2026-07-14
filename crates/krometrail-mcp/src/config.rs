use std::sync::Arc;

use krometrail_core::{
    CAPABILITY_REGISTRY, CapabilityDefault, CapabilityId, Result, validate_capability_selection,
};

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
