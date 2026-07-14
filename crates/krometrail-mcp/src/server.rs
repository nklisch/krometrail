use std::sync::Arc;

use krometrail_core::{BrowserConnector, Result};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::tool::{ToolCallContext, ToolRouter},
    model::{
        CallToolRequestParam, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RoleServer},
};

use crate::{config::McpConfig, registry::build_router, session::BrowserSessionOwner};

#[derive(Clone)]
pub struct KrometrailMcpServer {
    sessions: Arc<BrowserSessionOwner>,
    router: Arc<ToolRouter<KrometrailMcpServer>>,
}

impl KrometrailMcpServer {
    fn new(sessions: Arc<BrowserSessionOwner>, config: &McpConfig) -> Result<Self> {
        let mut server = Self {
            sessions,
            router: Arc::new(ToolRouter::new()),
        };
        server.router = Arc::new(build_router(config)?);
        Ok(server)
    }

    pub(crate) fn sessions(&self) -> &BrowserSessionOwner {
        &self.sessions
    }

    pub(crate) fn tools(&self) -> Vec<rmcp::model::Tool> {
        let mut tools = self.router.list_all();
        tools.sort_by(|left, right| left.name.cmp(&right.name));
        tools
    }
}

impl ServerHandler for KrometrailMcpServer {
    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult::with_all_items(self.tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        self.router
            .call(ToolCallContext::new(self, request, context))
            .await
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "krometrail".into(),
                title: Some("Krometrail browser control".into()),
                version: env!("CARGO_PKG_VERSION").into(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Start or attach one local browser session before calling browser operations."
                    .into(),
            ),
        }
    }
}

pub struct McpService {
    server: KrometrailMcpServer,
    sessions: Arc<BrowserSessionOwner>,
}

impl McpService {
    pub(crate) fn server(&self) -> &KrometrailMcpServer {
        &self.server
    }

    pub(crate) fn sessions(&self) -> &BrowserSessionOwner {
        &self.sessions
    }
}

pub fn build_service(
    connector: Arc<dyn BrowserConnector>,
    config: McpConfig,
) -> Result<McpService> {
    let sessions = Arc::new(BrowserSessionOwner::new(connector));
    let server = KrometrailMcpServer::new(Arc::clone(&sessions), &config)?;
    Ok(McpService { server, sessions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::lifecycle_tool_names;
    use krometrail_core::{
        BROWSER_OPERATION_REGISTRY, BrowserConnectRequest, BrowserInstallation, BrowserSessionPort,
        CapabilityId, PortFuture,
    };

    struct UnusedConnector;

    impl BrowserConnector for UnusedConnector {
        fn installations(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserInstallation>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }

        fn connect(
            &self,
            _request: BrowserConnectRequest,
        ) -> PortFuture<'_, krometrail_core::Result<Arc<dyn BrowserSessionPort>>> {
            panic!("tool registration must not connect a browser")
        }
    }

    #[test]
    fn control_registry_is_complete_unique_sorted_and_conservatively_annotated() {
        let service = build_service(
            Arc::new(UnusedConnector),
            McpConfig::new(vec![CapabilityId::Control]).unwrap(),
        )
        .unwrap();
        let tools = service.server().tools();
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        let mut expected: Vec<_> = lifecycle_tool_names()
            .chain(
                BROWSER_OPERATION_REGISTRY
                    .iter()
                    .map(|definition| definition.stable_name),
            )
            .collect();
        expected.sort_unstable();
        assert_eq!(names, expected);
        assert!(names.windows(2).all(|pair| pair[0] < pair[1]));

        let output_schema = tools[0].output_schema.clone().unwrap();
        for tool in tools {
            assert_eq!(tool.output_schema.as_ref(), Some(&output_schema));
            let annotations = tool.annotations.unwrap();
            assert_eq!(annotations.open_world_hint, Some(true));
            if let Some(definition) = BROWSER_OPERATION_REGISTRY
                .iter()
                .find(|definition| definition.stable_name == tool.name)
            {
                let read_only =
                    definition.mutability == krometrail_core::OperationMutability::ReadOnly;
                assert_eq!(annotations.read_only_hint, Some(read_only));
                assert_eq!(annotations.destructive_hint, Some(!read_only));
                assert_eq!(annotations.idempotent_hint, Some(read_only));
            }
        }
    }

    #[test]
    fn disabled_control_registers_no_speculative_tools() {
        let service = build_service(
            Arc::new(UnusedConnector),
            McpConfig::new(vec![
                CapabilityId::TemporalVision,
                CapabilityId::BrowserEvents,
            ])
            .unwrap(),
        )
        .unwrap();
        assert!(service.server().tools().is_empty());
    }
}
