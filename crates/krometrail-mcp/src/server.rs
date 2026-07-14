use std::sync::Arc;

use krometrail_core::{
    BrowserConnector, ErrorCode, KrometrailError, NonEmptyText, Result, RetryAdvice,
};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::tool::{ToolCallContext, ToolRouter},
    model::{
        CallToolRequestParam, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParam, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    service::{QuitReason, RequestContext, RoleServer, ServerInitializeError, ServiceExt as _},
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
    pub async fn serve_stdio(self) -> Result<()> {
        let running = match self.server.serve(rmcp::transport::stdio()).await {
            Ok(running) => running,
            Err(ServerInitializeError::ConnectionClosed(_)) => {
                return self.sessions.shutdown().await;
            }
            Err(_) => return Err(service_error("MCP stdio service could not start")),
        };
        let cancellation = running.cancellation_token();
        let signal_task = tokio::spawn(async move {
            wait_for_shutdown_signal().await;
            cancellation.cancel();
        });
        let service_result = match running.waiting().await {
            Ok(QuitReason::Closed | QuitReason::Cancelled) => Ok(()),
            Ok(QuitReason::JoinError(_)) | Err(_) => {
                Err(service_error("MCP stdio service ended unexpectedly"))
            }
        };
        signal_task.abort();
        let _ = signal_task.await;
        let shutdown_result = self.sessions.shutdown().await;
        service_result.and(shutdown_result)
    }

    #[cfg(test)]
    pub(crate) fn server(&self) -> &KrometrailMcpServer {
        &self.server
    }
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut interrupt = signal(SignalKind::interrupt()).ok();
        let mut terminate = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = async { interrupt.as_mut().expect("guarded signal").recv().await }, if interrupt.is_some() => {},
            _ = async { terminate.as_mut().expect("guarded signal").recv().await }, if terminate.is_some() => {},
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

fn service_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Internal,
        NonEmptyText::new(message).expect("static service error is valid"),
    )
    .with_retry(RetryAdvice::Safe)
    .with_recovery(
        NonEmptyText::new("restart the MCP client connection and try again")
            .expect("static service recovery is valid"),
    )
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
        BROWSER_OPERATION_REGISTRY, BrowserCompatibility, BrowserConnectRequest,
        BrowserInstallation, BrowserOperationContext, BrowserOperationRequest,
        BrowserOperationResult, BrowserOwnership, BrowserProduct, BrowserProductVersion,
        BrowserSessionEvent, BrowserSessionEvents, BrowserSessionPort, BrowserSessionState,
        BrowserStatus, BrowserStopOutcome, BrowserVersion, CapabilityId, CapabilitySupport,
        PageStatus, PortFuture, ProfileRef, RendererCapability, RetentionStatus, SessionId,
        SessionOrigin,
    };
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

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

    struct ProtocolEvents;
    impl BrowserSessionEvents for ProtocolEvents {
        fn next(&mut self) -> PortFuture<'_, krometrail_core::Result<Option<BrowserSessionEvent>>> {
            Box::pin(std::future::ready(Ok(None)))
        }
    }

    struct ProtocolSession {
        status: BrowserStatus,
        execute_calls: Arc<AtomicUsize>,
        stop_calls: Arc<AtomicUsize>,
    }

    impl BrowserSessionPort for ProtocolSession {
        fn session_origin(&self) -> SessionOrigin {
            SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0))
        }
        fn status(&self) -> PortFuture<'_, krometrail_core::Result<BrowserStatus>> {
            Box::pin(std::future::ready(Ok(self.status.clone())))
        }
        fn subscribe(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<Box<dyn BrowserSessionEvents>>> {
            Box::pin(std::future::ready(Ok(
                Box::new(ProtocolEvents) as Box<dyn BrowserSessionEvents>
            )))
        }
        fn execute(
            &self,
            request: BrowserOperationRequest,
            _context: BrowserOperationContext,
        ) -> PortFuture<'_, krometrail_core::Result<BrowserOperationResult>> {
            self.execute_calls.fetch_add(1, Ordering::SeqCst);
            let result = match request {
                BrowserOperationRequest::ListPages(_) => {
                    Ok(BrowserOperationResult::ListPages(Box::default()))
                }
                _ => panic!("protocol test dispatched an unexpected operation"),
            };
            Box::pin(std::future::ready(result))
        }
        fn stop(&self) -> PortFuture<'_, krometrail_core::Result<BrowserStopOutcome>> {
            self.stop_calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(std::future::ready(Ok(BrowserStopOutcome::Detached)))
        }
    }

    struct ProtocolConnector {
        session: Arc<ProtocolSession>,
    }
    impl BrowserConnector for ProtocolConnector {
        fn installations(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserInstallation>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }
        fn connect(
            &self,
            _request: BrowserConnectRequest,
        ) -> PortFuture<'_, krometrail_core::Result<Arc<dyn BrowserSessionPort>>> {
            let session = Arc::clone(&self.session) as Arc<dyn BrowserSessionPort>;
            Box::pin(std::future::ready(Ok(session)))
        }
    }

    fn protocol_status() -> BrowserStatus {
        let version = BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("1").unwrap(),
            "revision",
            "1.3",
            "agent",
            "1",
        )
        .unwrap();
        let compatibility = BrowserCompatibility::new(
            version,
            RendererCapability::ALL
                .iter()
                .map(|capability| CapabilitySupport::new(*capability, true, true, None).unwrap())
                .collect(),
        )
        .unwrap();
        BrowserStatus::new(
            "00000000-0000-0000-0000-000000000001"
                .parse::<SessionId>()
                .unwrap(),
            BrowserSessionState::Ready,
            BrowserOwnership::Attached,
            ProfileRef::External,
            compatibility,
            None,
            Vec::<PageStatus>::new(),
            Vec::new(),
            RetentionStatus::empty(krometrail_core::DiskBudgetBytes::default()),
        )
        .unwrap()
    }

    async fn send_json(writer: &mut tokio::io::WriteHalf<tokio::io::DuplexStream>, value: Value) {
        writer
            .write_all(value.to_string().as_bytes())
            .await
            .unwrap();
        writer.write_all(b"\n").await.unwrap();
        writer.flush().await.unwrap();
    }

    async fn read_json(
        reader: &mut BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>,
    ) -> Value {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(!line.is_empty(), "server closed before a JSON-RPC response");
        serde_json::from_str(&line).unwrap()
    }

    #[tokio::test]
    async fn in_memory_json_rpc_initializes_lists_calls_validates_and_closes() {
        let execute_calls = Arc::new(AtomicUsize::new(0));
        let stop_calls = Arc::new(AtomicUsize::new(0));
        let session = Arc::new(ProtocolSession {
            status: protocol_status(),
            execute_calls: Arc::clone(&execute_calls),
            stop_calls: Arc::clone(&stop_calls),
        });
        let service = build_service(
            Arc::new(ProtocolConnector { session }),
            McpConfig::new(vec![CapabilityId::Control]).unwrap(),
        )
        .unwrap();
        let owner = Arc::clone(&service.sessions);
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let server_task = tokio::spawn(async move {
            let running = service.server.serve(server_io).await.unwrap();
            running.waiting().await.unwrap()
        });
        let (read, mut write) = tokio::io::split(client_io);
        let mut read = BufReader::new(read);

        send_json(
            &mut write,
            json!({
                "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                    "protocolVersion":"2025-06-18","capabilities":{},
                    "clientInfo":{"name":"krometrail-test","version":"1"}
                }
            }),
        )
        .await;
        let initialized = read_json(&mut read).await;
        assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}),
        )
        .await;
        let listed = read_json(&mut read).await;
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), BROWSER_OPERATION_REGISTRY.len() + 4);
        assert!(
            tools.windows(2).all(|pair| {
                pair[0]["name"].as_str().unwrap() < pair[1]["name"].as_str().unwrap()
            })
        );

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"start_browser","arguments":{}}}),
        )
        .await;
        assert_eq!(read_json(&mut read).await["result"]["isError"], false);
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"list_pages","arguments":{}}}),
        )
        .await;
        let valid = read_json(&mut read).await;
        assert_eq!(valid["result"]["isError"], false);
        assert_eq!(valid["result"]["structuredContent"]["status"], "succeeded");
        assert_eq!(execute_calls.load(Ordering::SeqCst), 1);

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"navigate_page","arguments":{"url":""}}}),
        )
        .await;
        let invalid = read_json(&mut read).await;
        assert_eq!(invalid["result"]["isError"], true);
        assert_eq!(
            invalid["result"]["structuredContent"]["error"]["code"],
            "invalid_input"
        );
        assert_eq!(execute_calls.load(Ordering::SeqCst), 1);

        drop(write);
        drop(read);
        assert!(matches!(server_task.await.unwrap(), QuitReason::Closed));
        owner.shutdown().await.unwrap();
        assert_eq!(stop_calls.load(Ordering::SeqCst), 1);
    }
}
