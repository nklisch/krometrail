use std::sync::Arc;

use crate::{
    browser::{BrowserVersion, PageTarget, ProfileIdentity},
    error::Result,
};

use super::PortFuture;

/// Browser connection inputs stay capability-shaped until a real transport gate
/// supplies measured protocol details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BrowserConnectRequest {
    Launch(LaunchBrowser),
    Attach(AttachBrowser),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchBrowser {
    pub profile: ProfileIdentity,
    pub initial_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachBrowser {
    pub endpoint: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BrowserCompatibility {
    pub version: BrowserVersion,
    pub required_domains: Vec<DomainSupport>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DomainSupport {
    pub domain: String,
    pub available: bool,
    pub detail: Option<String>,
}

pub trait BrowserConnector: Send + Sync {
    fn connect(
        &self,
        request: BrowserConnectRequest,
    ) -> PortFuture<'_, Result<Arc<dyn BrowserSessionPort>>>;
}

pub trait BrowserSessionPort: Send + Sync {
    fn compatibility(&self) -> &BrowserCompatibility;
    fn page_targets(&self) -> PortFuture<'_, Result<Vec<PageTarget>>>;
    fn close(&self) -> PortFuture<'_, Result<()>>;
}
