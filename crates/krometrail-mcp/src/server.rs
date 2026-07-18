use std::sync::Arc;

use krometrail_core::{
    CancellationSignal, ErrorCode, KrometrailError, NonEmptyText, Result, RetryAdvice,
};
use rmcp::{
    ErrorData, ServerHandler,
    handler::server::tool::{ToolCallContext, ToolRouter},
    model::{
        CallToolRequestParam, CallToolResult, Implementation, ListResourceTemplatesResult,
        ListResourcesResult, ListToolsResult, PaginatedRequestParam, ProtocolVersion,
        ReadResourceRequestParam, ServerCapabilities, ServerInfo,
    },
    service::{QuitReason, RequestContext, RoleServer, ServerInitializeError, ServiceExt as _},
};
use tracing::Instrument as _;
use uuid::Uuid;

use crate::{
    config::{DiagnosticContext, McpConfig, McpDependencies},
    registry::{MCP_REQUEST_DEADLINE, McpCancellation, build_router},
    resources::{read_resource, resource_templates},
    session::BrowserSessionOwner,
};

#[derive(Clone)]
pub struct KrometrailMcpServer {
    sessions: Arc<BrowserSessionOwner>,
    router: Arc<ToolRouter<KrometrailMcpServer>>,
    dependencies: Arc<McpDependencies>,
    temporal_resources: bool,
    diagnostics: DiagnosticContext,
}

impl KrometrailMcpServer {
    fn new(
        sessions: Arc<BrowserSessionOwner>,
        dependencies: Arc<McpDependencies>,
        config: &McpConfig,
    ) -> Result<Self> {
        let router = Arc::new(build_router(
            config,
            Arc::clone(&dependencies),
            Arc::clone(&sessions),
        )?);
        let diagnostics = dependencies.diagnostics.clone();
        let server = Self {
            sessions,
            router,
            dependencies,
            temporal_resources: config.is_enabled(krometrail_core::CapabilityId::TemporalVision),
            diagnostics,
        };
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

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourcesResult, ErrorData> {
        // Retained evidence is dynamic and potentially large. Agents discover
        // concrete handles from tool responses and follow the strict URI back.
        Ok(ListResourcesResult::with_all_items(Vec::new()))
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParam>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, ErrorData> {
        if self.temporal_resources {
            Ok(ListResourceTemplatesResult::with_all_items(
                resource_templates(),
            ))
        } else {
            Ok(ListResourceTemplatesResult::with_all_items(Vec::new()))
        }
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<rmcp::model::ReadResourceResult, ErrorData> {
        if !self.temporal_resources {
            return Err(ErrorData::method_not_found::<
                rmcp::model::ReadResourceRequestMethod,
            >());
        }
        let deadline = std::time::Instant::now() + MCP_REQUEST_DEADLINE;
        let cancellation: Arc<dyn CancellationSignal> =
            Arc::new(McpCancellation::new(context.ct.clone()));
        let correlation_id = Uuid::new_v4().to_string();
        let span = tracing::info_span!(
            "mcp.request",
            correlation_id = %correlation_id,
            route = "resources/read"
        );
        async {
            let mut result = read_resource(
                &request.uri,
                self.dependencies.progressive_evidence.as_ref(),
                deadline,
                cancellation,
            )
            .await;
            if let Err(error) = &mut result {
                attach_error_diagnostics(error, &correlation_id, &self.diagnostics);
            }
            tracing::info!(
                event = "mcp.request.completed",
                outcome = if result.is_ok() {
                    "succeeded"
                } else {
                    "failed"
                },
                "mcp.request.completed"
            );
            result
        }
        .instrument(span)
        .await
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParam,
        context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let correlation_id = Uuid::new_v4().to_string();
        let route = request.name.to_string();
        let span = tracing::info_span!(
            "mcp.request",
            correlation_id = %correlation_id,
            route = %route
        );
        async {
            let mut result = self
                .router
                .call(ToolCallContext::new(self, request, context))
                .await;
            let outcome = match &mut result {
                Ok(result) => attach_diagnostics(result, &correlation_id, &self.diagnostics),
                Err(error) => {
                    attach_error_diagnostics(error, &correlation_id, &self.diagnostics);
                    "failed"
                }
            };
            tracing::info!(
                event = "mcp.request.completed",
                outcome,
                "mcp.request.completed"
            );
            result
        }
        .instrument(span)
        .await
    }

    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_06_18,
            capabilities: if self.temporal_resources {
                ServerCapabilities::builder()
                    .enable_tools()
                    .enable_resources()
                    .build()
            } else {
                ServerCapabilities::builder().enable_tools().build()
            },
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

fn attach_error_diagnostics(
    error: &mut ErrorData,
    correlation_id: &str,
    context: &DiagnosticContext,
) {
    let diagnostics = serde_json::json!({
        "correlation_id": correlation_id,
        "log_path": context.log_path().map(|path| path.to_string_lossy()),
    });
    match error.data.take() {
        Some(serde_json::Value::Object(mut data)) => {
            data.insert("diagnostics".into(), diagnostics);
            error.data = Some(serde_json::Value::Object(data));
        }
        Some(data) => error.data = Some(data),
        None => error.data = Some(serde_json::json!({ "diagnostics": diagnostics })),
    }
}

fn attach_diagnostics(
    result: &mut CallToolResult,
    correlation_id: &str,
    context: &DiagnosticContext,
) -> &'static str {
    let Some(serde_json::Value::Object(response)) = result.structured_content.as_mut() else {
        return if result.is_error == Some(true) {
            "failed"
        } else {
            "succeeded"
        };
    };
    let outcome = match response
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(if result.is_error == Some(true) {
            "failed"
        } else {
            "succeeded"
        }) {
        "failed" => "failed",
        "degraded" => "degraded",
        _ => "succeeded",
    };
    if outcome != "succeeded" {
        response.insert(
            "diagnostics".into(),
            serde_json::json!({
                "correlation_id": correlation_id,
                "log_path": context.log_path().map(|path| path.to_string_lossy()),
            }),
        );
    }
    outcome
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

pub fn build_service(dependencies: McpDependencies, config: McpConfig) -> Result<McpService> {
    let dependencies = Arc::new(dependencies);
    let sessions = Arc::new(BrowserSessionOwner::new(Arc::clone(&dependencies.browser)));
    let server = KrometrailMcpServer::new(Arc::clone(&sessions), dependencies, &config)?;
    Ok(McpService { server, sessions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::lifecycle_tool_names;
    use krometrail_core::{
        AnalysisScale, AnchorScope, ArtifactCacheDisposition, ArtifactEvidenceHandle,
        ArtifactFailurePolicy, ArtifactGenerationRequest, ArtifactGenerationResult,
        ArtifactGeneratorRequest, ArtifactHandle, ArtifactLabelsRequest, ArtifactOutcome,
        ArtifactRead, BROWSER_OPERATION_REGISTRY, BrowserCompatibility, BrowserConnectRequest,
        BrowserConnector, BrowserEventDetailRequest, BrowserEventFilter, BrowserInstallation,
        BrowserOperationContext, BrowserOperationRequest, BrowserOperationResult, BrowserOwnership,
        BrowserProduct, BrowserProductVersion, BrowserSessionEvent, BrowserSessionEvents,
        BrowserSessionPort, BrowserSessionState, BrowserStatus, BrowserStopOutcome, BrowserVersion,
        BundleArtifactEvidence, BundleContextEvidence, BundleDegradation, CapabilityId,
        CapabilitySupport, CaptureStatistics, CaptureStreamState, CaptureTimingSummary,
        EffectiveBundlePolicy, EveryNthFrame, EvidenceScope, FrameId, GenerateArtifactsRequest,
        NonEmptyText, NormalizationRequest, OutputLimitsRequest, PageStatus, PortFuture,
        ProfileRef, ProgressiveEvidence, ProgressiveEvidenceContext, ProgressiveEvidenceRequest,
        ProgressiveEvidenceResult, ProgressiveRegion, RangeResolutionOptions,
        RegionFilmstripEvidenceRequest, RendererCapability, ResolvedRange,
        ResolvedRangeEvidenceRequest, RetentionStatus, SessionId, SessionOrigin, SessionRange,
        SessionTime, Sha256Digest, SourceFrameSelection, SourceFramesRequest,
        SourceReadLimitsRequest, StoryboardRequest, TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION,
        TargetCaptureStatus, TargetId, TemporalContext, TemporalContextQuery,
        TemporalContextRequest, TemporalDebugBundle, TemporalDebugBundleContext,
        TemporalDebugBundleRequest, TemporalDebugBundles, TemporalDebugHeader,
        TemporalQueryRequest, TemporalRangeAnchor, TemporalRangeAnchorKind,
    };
    use serde_json::{Value, json};
    use std::{
        num::NonZeroU32,
        sync::{
            Mutex,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader};

    struct UnusedConnector;

    #[test]
    fn diagnostics_are_added_only_to_failed_or_degraded_tool_envelopes() {
        let context = DiagnosticContext::new(Some("/private/krometrail.log".into()));
        let error = KrometrailError::new(
            ErrorCode::InvalidInput,
            NonEmptyText::new("invalid fixture request").unwrap(),
        );
        let mut failed = crate::response::visible_error("browser_click", error);
        assert_eq!(
            attach_diagnostics(&mut failed, "correlation-1", &context),
            "failed"
        );
        let structured = failed.structured_content.unwrap();
        assert_eq!(
            structured["diagnostics"],
            serde_json::json!({
                "correlation_id": "correlation-1",
                "log_path": "/private/krometrail.log"
            })
        );

        let mapped = crate::response::map_lifecycle_result("browser_status", json!({})).unwrap();
        let mut succeeded = crate::response::into_call_tool_result(mapped).unwrap();
        assert_eq!(
            attach_diagnostics(&mut succeeded, "correlation-2", &context),
            "succeeded"
        );
        assert!(
            succeeded
                .structured_content
                .unwrap()
                .get("diagnostics")
                .is_none()
        );
    }

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

    struct UnusedTemporal;

    impl TemporalDebugBundles for UnusedTemporal {
        fn bundle(
            &self,
            _request: TemporalDebugBundleRequest,
            _context: TemporalDebugBundleContext,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalDebugBundle>> {
            panic!("temporal bundle should not be called by a control-only test")
        }
    }

    impl ProgressiveEvidence for UnusedTemporal {
        fn execute(
            &self,
            _request: ProgressiveEvidenceRequest,
            _context: ProgressiveEvidenceContext,
        ) -> PortFuture<'_, krometrail_core::Result<ProgressiveEvidenceResult>> {
            panic!("progressive evidence should not be called by a control-only test")
        }
    }

    impl TemporalContextQuery for UnusedTemporal {
        fn context(
            &self,
            _request: TemporalContextRequest,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalContext>> {
            panic!("temporal context should not be called by a control-only test")
        }
    }

    fn dependencies(browser: Arc<dyn BrowserConnector>) -> McpDependencies {
        let temporal = Arc::new(UnusedTemporal);
        McpDependencies {
            browser,
            temporal_debug_bundles: Arc::clone(&temporal) as Arc<dyn TemporalDebugBundles>,
            progressive_evidence: Arc::clone(&temporal) as Arc<dyn ProgressiveEvidence>,
            temporal_context: temporal as Arc<dyn TemporalContextQuery>,
            temporal_video: None,
            diagnostics: DiagnosticContext::default(),
        }
    }

    struct TemporalSpy {
        bundle_calls: AtomicUsize,
        progressive_calls: AtomicUsize,
        context_calls: AtomicUsize,
        bundle_request: Mutex<Option<Value>>,
        progressive_request: Mutex<Option<Value>>,
        context_request: Mutex<Option<Value>>,
    }

    impl TemporalSpy {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                bundle_calls: AtomicUsize::new(0),
                progressive_calls: AtomicUsize::new(0),
                context_calls: AtomicUsize::new(0),
                bundle_request: Mutex::new(None),
                progressive_request: Mutex::new(None),
                context_request: Mutex::new(None),
            })
        }

        fn error() -> KrometrailError {
            KrometrailError::new(
                krometrail_core::ErrorCode::Unsupported,
                krometrail_core::NonEmptyText::new("spy result").unwrap(),
            )
        }
    }

    impl TemporalDebugBundles for TemporalSpy {
        fn bundle(
            &self,
            request: TemporalDebugBundleRequest,
            _context: TemporalDebugBundleContext,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalDebugBundle>> {
            self.bundle_calls.fetch_add(1, Ordering::SeqCst);
            *self.bundle_request.lock().unwrap() = Some(serde_json::to_value(request).unwrap());
            Box::pin(std::future::ready(Err(Self::error())))
        }
    }

    impl ProgressiveEvidence for TemporalSpy {
        fn execute(
            &self,
            request: ProgressiveEvidenceRequest,
            _context: ProgressiveEvidenceContext,
        ) -> PortFuture<'_, krometrail_core::Result<ProgressiveEvidenceResult>> {
            self.progressive_calls.fetch_add(1, Ordering::SeqCst);
            *self.progressive_request.lock().unwrap() =
                Some(serde_json::to_value(request).unwrap());
            Box::pin(std::future::ready(Err(Self::error())))
        }
    }

    impl TemporalContextQuery for TemporalSpy {
        fn context(
            &self,
            request: TemporalContextRequest,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalContext>> {
            self.context_calls.fetch_add(1, Ordering::SeqCst);
            *self.context_request.lock().unwrap() = Some(serde_json::to_value(request).unwrap());
            Box::pin(std::future::ready(Err(Self::error())))
        }
    }

    struct TemporalSuccessSpy {
        bundle: TemporalDebugBundle,
        artifact_read: ArtifactRead,
        mismatched_artifact_read: ArtifactRead,
        progressive_calls: AtomicUsize,
    }

    impl TemporalDebugBundles for TemporalSuccessSpy {
        fn bundle(
            &self,
            _request: TemporalDebugBundleRequest,
            _context: TemporalDebugBundleContext,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalDebugBundle>> {
            Box::pin(std::future::ready(Ok(self.bundle.clone())))
        }
    }

    impl ProgressiveEvidence for TemporalSuccessSpy {
        fn execute(
            &self,
            request: ProgressiveEvidenceRequest,
            _context: ProgressiveEvidenceContext,
        ) -> PortFuture<'_, krometrail_core::Result<ProgressiveEvidenceResult>> {
            let call = self.progressive_calls.fetch_add(1, Ordering::SeqCst) + 1;
            let ProgressiveEvidenceRequest::RetrieveArtifact(_) = request else {
                return Box::pin(std::future::ready(Err(KrometrailError::new(
                    krometrail_core::ErrorCode::Unsupported,
                    NonEmptyText::new("fixture only serves artifact reads").unwrap(),
                ))));
            };
            let read = if call <= 3 {
                self.artifact_read.clone()
            } else {
                self.mismatched_artifact_read.clone()
            };
            Box::pin(std::future::ready(Ok(
                ProgressiveEvidenceResult::RetrieveArtifact(Box::new(read)),
            )))
        }
    }

    impl TemporalContextQuery for TemporalSuccessSpy {
        fn context(
            &self,
            _request: TemporalContextRequest,
        ) -> PortFuture<'_, krometrail_core::Result<TemporalContext>> {
            Box::pin(std::future::ready(Err(KrometrailError::new(
                krometrail_core::ErrorCode::Unsupported,
                NonEmptyText::new("fixture does not serve context reads").unwrap(),
            ))))
        }
    }

    fn dependencies_with_spy(
        browser: Arc<dyn BrowserConnector>,
        spy: Arc<TemporalSpy>,
    ) -> McpDependencies {
        McpDependencies {
            browser,
            temporal_debug_bundles: Arc::clone(&spy) as Arc<dyn TemporalDebugBundles>,
            progressive_evidence: Arc::clone(&spy) as Arc<dyn ProgressiveEvidence>,
            temporal_context: spy as Arc<dyn TemporalContextQuery>,
            temporal_video: None,
            diagnostics: DiagnosticContext::default(),
        }
    }

    fn session_id() -> SessionId {
        "00000000-0000-0000-0000-000000000001".parse().unwrap()
    }

    fn target_id() -> TargetId {
        "00000000-0000-0000-0000-000000000002".parse().unwrap()
    }

    fn frame_id() -> FrameId {
        "00000000-0000-0000-0000-000000000003".parse().unwrap()
    }

    fn resolved_range() -> ResolvedRange {
        let range = SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap();
        ResolvedRange::new(
            session_id(),
            target_id(),
            TemporalRangeAnchorKind::SessionTime,
            range,
            range,
            vec![frame_id()],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            RangeResolutionOptions::DEFAULT,
        )
        .unwrap()
    }

    fn bundle_request() -> TemporalDebugBundleRequest {
        TemporalDebugBundleRequest::default_policy(
            TemporalQueryRequest::strict(TemporalRangeAnchor::SessionTime {
                scope: AnchorScope::new(Some(session_id()), Some(target_id())),
                range: SessionRange::new(SessionTime::ZERO, SessionTime::from_nanos(10)).unwrap(),
            })
            .unwrap(),
        )
        .unwrap()
    }

    fn artifact_id() -> krometrail_core::ArtifactId {
        "00000000-0000-0000-0000-000000000004".parse().unwrap()
    }

    fn successful_bundle_fixture() -> (Arc<TemporalSuccessSpy>, String, Arc<[u8]>) {
        let range = resolved_range();
        let scope = EvidenceScope::new(session_id(), target_id()).unwrap();
        let bytes: Arc<[u8]> = Arc::from(b"\x89PNG\r\n\x1a\npayload".as_slice());
        let digest = Sha256Digest::digest(&bytes);
        let dimensions = temporal_vision::PixelDimensions::new(1, 1).unwrap();
        let sequence: temporal_vision::FrameSequence<
            FrameId,
            krometrail_core::ArtifactMarkerId,
            krometrail_core::GapId,
            Box<[u8]>,
        > = temporal_vision::FrameSequence::new(
            vec![
                temporal_vision::Frame::new(
                    frame_id(),
                    temporal_vision::Timestamp::from_nanos(0),
                    dimensions,
                    temporal_vision::PixelFormat::Rgba8SrgbStraight,
                    vec![0, 0, 0, 255].into_boxed_slice(),
                )
                .unwrap(),
            ],
            Vec::new(),
            Vec::new(),
            None,
            None,
        )
        .unwrap();
        let manifest_for = |id| {
            temporal_vision::ArtifactManifest::from_sequence(
                id,
                temporal_vision::ArtifactKind::DifferenceMap,
                temporal_vision::EvidenceClass::SourceDerived,
                temporal_vision::AlgorithmDescriptor::new("mcp-fixture", "1").unwrap(),
                &sequence,
                Vec::new(),
                Vec::new(),
                temporal_vision::Parameters::empty(),
                dimensions,
                temporal_vision::OutputHash::from_bytes(*digest.as_bytes()),
            )
            .unwrap()
        };
        let artifact_manifest = manifest_for(artifact_id());
        let media_type = NonEmptyText::new("image/png").unwrap();
        let artifact = ArtifactHandle {
            artifact_id: artifact_id(),
            cache: ArtifactCacheDisposition::Generated,
            media_type: media_type.clone(),
            encoded_byte_len: bytes.len() as u64,
            manifest: artifact_manifest.clone(),
        };
        let artifact_read = ArtifactRead::new(
            ArtifactEvidenceHandle::new(
                artifact_id(),
                scope,
                media_type.clone(),
                digest,
                bytes.len() as u64,
                artifact_manifest.clone(),
            )
            .unwrap(),
            Arc::clone(&bytes),
        )
        .unwrap();
        let mismatched_artifact_read = ArtifactRead::new(
            ArtifactEvidenceHandle::new(
                artifact_id(),
                EvidenceScope::new(
                    session_id(),
                    "00000000-0000-0000-0000-000000000099".parse().unwrap(),
                )
                .unwrap(),
                media_type,
                digest,
                bytes.len() as u64,
                artifact_manifest,
            )
            .unwrap(),
            Arc::clone(&bytes),
        )
        .unwrap();
        let generator = ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(5),
            tile_limit: 3,
            noise_floor: 0,
            normalization: NormalizationRequest::new(
                None,
                temporal_vision::Rgb8::new(0, 0, 0),
                AnalysisScale::Identity,
            )
            .unwrap(),
            labels: ArtifactLabelsRequest::new(
                NonEmptyText::new("fixture").unwrap(),
                NonEmptyText::new("mcp").unwrap(),
            ),
            include_orientation: false,
            output: OutputLimitsRequest::new(64, 64, 1024).unwrap(),
        });
        let (requested_query, _, _) = bundle_request().into_parts();
        let effective = EffectiveBundlePolicy::new(
            NonEmptyText::new(TEMPORAL_DEBUG_BUNDLE_POLICY_VERSION).unwrap(),
            range.resolved_anchor.effective_time,
            vec![generator],
            ArtifactFailurePolicy::AllowPartial,
            BrowserEventFilter::default(),
            krometrail_core::BrowserEventSelection::compact_default(),
            Vec::new(),
        )
        .unwrap();
        let bundle = TemporalDebugBundle::new(
            requested_query,
            range.clone(),
            effective,
            TemporalDebugHeader::new(
                NonEmptyText::new("fixture temporal bundle").unwrap(),
                Vec::new(),
            )
            .unwrap(),
            Vec::new(),
            BundleArtifactEvidence::Available(ArtifactGenerationResult {
                range,
                epochs: Vec::new(),
                outcomes: vec![ArtifactOutcome::Available {
                    epoch_index: 0,
                    generator_index: 0,
                    artifact,
                }],
            }),
            BundleContextEvidence::Unavailable {
                error: KrometrailError::new(
                    krometrail_core::ErrorCode::Unsupported,
                    NonEmptyText::new("fixture context is intentionally absent").unwrap(),
                ),
            },
            Vec::new(),
            vec![BundleDegradation::ContextUnavailable],
        )
        .unwrap();
        let uri =
            crate::resources::EvidenceResourceUri::artifact(scope, artifact_id()).canonical_uri();
        (
            Arc::new(TemporalSuccessSpy {
                bundle,
                artifact_read,
                mismatched_artifact_read,
                progressive_calls: AtomicUsize::new(0),
            }),
            uri,
            bytes,
        )
    }

    #[test]
    fn control_registry_is_complete_unique_sorted_and_conservatively_annotated() {
        let service = build_service(
            dependencies(Arc::new(UnusedConnector)),
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
        assert!(service.server().get_info().capabilities.resources.is_none());

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
    fn lifecycle_routes_publish_one_generated_stride_contract() {
        let service = build_service(
            dependencies(Arc::new(UnusedConnector)),
            McpConfig::new(vec![CapabilityId::Control]).unwrap(),
        )
        .unwrap();
        let tools = service.server().tools();
        let input_schema = |name: &str| {
            tools
                .iter()
                .find(|tool| tool.name == name)
                .unwrap_or_else(|| panic!("missing lifecycle tool {name}"))
                .input_schema
                .clone()
        };
        let start = input_schema("start_browser");
        let attach = input_schema("attach_browser");
        let start_properties = start.get("properties").unwrap();
        let attach_properties = attach.get("properties").unwrap();
        let start_stride = start_properties.get("every_nth_frame").unwrap().clone();
        let attach_stride = attach_properties.get("every_nth_frame").unwrap().clone();

        assert_eq!(start_stride, attach_stride);
        assert_eq!(start_stride["type"], "integer");
        assert_eq!(start_stride["minimum"], 1);
        assert_eq!(start_stride["maximum"], 60);
        assert_eq!(start_stride["default"], 1);
        assert_eq!(
            start_properties.get("focus").unwrap()["enum"],
            json!(["foreground", "preserve"])
        );
        assert_eq!(
            start_properties.get("focus").unwrap()["default"],
            "foreground"
        );
        assert!(attach_properties.get("focus").is_none());
        for schema in [start.as_ref(), attach.as_ref()] {
            assert!(
                !schema
                    .get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|required| {
                        required.iter().any(|field| field == "every_nth_frame")
                    })
            );
        }
    }

    #[tokio::test]
    async fn start_browser_forwards_explicit_preserve_focus_policy() {
        let connector = LifecycleConnector::new();
        let responses = invoke_control_tools(
            Arc::clone(&connector),
            vec![("start_browser", json!({"focus":"preserve"}))],
        )
        .await;
        assert_eq!(responses[0]["result"]["isError"], false);
        let requests = connector.requests();
        let [BrowserConnectRequest::Launch(request)] = requests.as_slice() else {
            panic!("start_browser did not forward a launch request")
        };
        assert_eq!(request.focus, krometrail_core::BrowserFocusPolicy::Preserve);
    }

    #[tokio::test]
    async fn lifecycle_routes_forward_defaults_values_and_capture_events_through_the_owner() {
        for route in ["start_browser", "attach_browser"] {
            for requested in [None, Some(1_u8), Some(7), Some(60)] {
                let connector = LifecycleConnector::new();
                let mut arguments = json!({});
                if route == "attach_browser" {
                    arguments["endpoint"] = json!("ws://127.0.0.1:9222");
                }
                if let Some(value) = requested {
                    arguments["every_nth_frame"] = json!(value);
                }
                let responses = invoke_control_tools(
                    Arc::clone(&connector),
                    vec![(route, arguments), ("browser_status", json!({}))],
                )
                .await;
                let expected = requested.unwrap_or(1);
                for response in responses {
                    assert_eq!(response["result"]["isError"], false);
                    assert_eq!(
                        response["result"]["structuredContent"]["result"]["every_nth_frame"],
                        expected
                    );
                }

                assert_eq!(connector.connect_calls.load(Ordering::SeqCst), 1);
                match (route, connector.requests().as_slice()) {
                    ("start_browser", [BrowserConnectRequest::Launch(request)]) => {
                        assert_eq!(request.every_nth_frame.get(), expected);
                    }
                    ("attach_browser", [BrowserConnectRequest::Attach(request)]) => {
                        assert_eq!(request.every_nth_frame.get(), expected);
                    }
                    _ => panic!("lifecycle route did not forward its core request"),
                }

                let mut events = connector.session().subscribe().await.unwrap();
                let event = events.next().await.unwrap().unwrap();
                assert_eq!(
                    serde_json::to_value(&event).unwrap()["status"]["every_nth_frame"],
                    expected
                );
                match event {
                    BrowserSessionEvent::CaptureStateChanged { status } => {
                        assert_eq!(status.every_nth_frame().get(), expected);
                    }
                    event => panic!("unexpected lifecycle capture event: {event:?}"),
                }
            }
        }
    }

    #[tokio::test]
    async fn lifecycle_routes_reject_invalid_stride_before_connector_calls() {
        for invalid in [json!(0), json!(61), json!(null), json!("7"), json!(7.5)] {
            for route in ["start_browser", "attach_browser"] {
                let connector = LifecycleConnector::new();
                let mut arguments = json!({});
                if route == "attach_browser" {
                    arguments["endpoint"] = json!("ws://127.0.0.1:9222");
                }
                arguments["every_nth_frame"] = invalid.clone();
                let response =
                    invoke_control_tools(Arc::clone(&connector), vec![(route, arguments)])
                        .await
                        .pop()
                        .unwrap();
                assert_eq!(response["result"]["isError"], true);
                assert_eq!(
                    response["result"]["structuredContent"]["error"]["code"],
                    "invalid_input"
                );
                assert_eq!(connector.connect_calls.load(Ordering::SeqCst), 0);
                assert!(connector.requests().is_empty());
            }
        }
    }

    #[test]
    fn capability_filters_keep_temporal_tools_when_control_is_disabled() {
        let service = build_service(
            dependencies(Arc::new(UnusedConnector)),
            McpConfig::new(vec![
                CapabilityId::TemporalVision,
                CapabilityId::BrowserEvents,
            ])
            .unwrap(),
        )
        .unwrap();
        let mut expected = vec![
            krometrail_core::TEMPORAL_DEBUG_BUNDLE_OPERATION.stable_name,
            krometrail_core::TEMPORAL_CONTEXT_OPERATION_REGISTRY[0].stable_name,
        ];
        expected.extend(
            krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY
                .iter()
                .filter(|definition| {
                    definition.exposure == krometrail_core::OperationExposure::Tool
                })
                .map(|definition| definition.stable_name),
        );
        expected.sort_unstable();
        let tools = service.server().tools();
        let names: Vec<_> = tools.iter().map(|tool| tool.name.as_ref()).collect();
        assert_eq!(names, expected);
        assert!(names.iter().all(|name| {
            *name != "start_browser" && *name != "list_pages" && *name != "retrieve_artifact"
        }));

        let temporal_only = build_service(
            dependencies(Arc::new(UnusedConnector)),
            McpConfig::new(vec![CapabilityId::TemporalVision]).unwrap(),
        )
        .unwrap();
        assert!(
            temporal_only
                .server()
                .tools()
                .iter()
                .all(|tool| tool.name != "query_browser_events")
        );
        let events_only = build_service(
            dependencies(Arc::new(UnusedConnector)),
            McpConfig::new(vec![CapabilityId::BrowserEvents]).unwrap(),
        )
        .unwrap();
        assert_eq!(
            events_only
                .server()
                .tools()
                .iter()
                .map(|tool| tool.name.as_ref())
                .collect::<Vec<_>>(),
            vec!["query_browser_events"]
        );
        assert!(
            events_only
                .server()
                .get_info()
                .capabilities
                .resources
                .is_none()
        );
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

    struct LifecycleEvents {
        next: Option<BrowserSessionEvent>,
    }

    impl BrowserSessionEvents for LifecycleEvents {
        fn next(&mut self) -> PortFuture<'_, krometrail_core::Result<Option<BrowserSessionEvent>>> {
            Box::pin(std::future::ready(Ok(self.next.take())))
        }
    }

    struct LifecycleSession {
        status: BrowserStatus,
        capture_event: BrowserSessionEvent,
    }

    impl BrowserSessionPort for LifecycleSession {
        fn session_origin(&self) -> SessionOrigin {
            SessionOrigin::new(krometrail_core::ObservedTime::from_nanos(0))
        }

        fn status(&self) -> PortFuture<'_, krometrail_core::Result<BrowserStatus>> {
            Box::pin(std::future::ready(Ok(self.status.clone())))
        }

        fn subscribe(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<Box<dyn BrowserSessionEvents>>> {
            Box::pin(std::future::ready(Ok(Box::new(LifecycleEvents {
                next: Some(self.capture_event.clone()),
            })
                as Box<dyn BrowserSessionEvents>)))
        }

        fn execute(
            &self,
            _request: BrowserOperationRequest,
            _context: BrowserOperationContext,
        ) -> PortFuture<'_, krometrail_core::Result<BrowserOperationResult>> {
            panic!("lifecycle contract test must not execute a browser operation")
        }

        fn stop(&self) -> PortFuture<'_, krometrail_core::Result<BrowserStopOutcome>> {
            Box::pin(std::future::ready(Ok(BrowserStopOutcome::Detached)))
        }
    }

    struct LifecycleConnector {
        connect_calls: AtomicUsize,
        requests: Mutex<Vec<BrowserConnectRequest>>,
        session: Mutex<Option<Arc<LifecycleSession>>>,
    }

    impl LifecycleConnector {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                connect_calls: AtomicUsize::new(0),
                requests: Mutex::new(Vec::new()),
                session: Mutex::new(None),
            })
        }

        fn requests(&self) -> Vec<BrowserConnectRequest> {
            self.requests.lock().unwrap().clone()
        }

        fn session(&self) -> Arc<LifecycleSession> {
            self.session
                .lock()
                .unwrap()
                .clone()
                .expect("valid lifecycle call creates a session")
        }
    }

    impl BrowserConnector for LifecycleConnector {
        fn installations(
            &self,
        ) -> PortFuture<'_, krometrail_core::Result<Vec<BrowserInstallation>>> {
            Box::pin(std::future::ready(Ok(Vec::new())))
        }

        fn connect(
            &self,
            request: BrowserConnectRequest,
        ) -> PortFuture<'_, krometrail_core::Result<Arc<dyn BrowserSessionPort>>> {
            let stride = match &request {
                BrowserConnectRequest::Launch(request) => request.every_nth_frame,
                BrowserConnectRequest::Attach(request) => request.every_nth_frame,
            };
            self.connect_calls.fetch_add(1, Ordering::SeqCst);
            self.requests.lock().unwrap().push(request);
            let session = Arc::new(LifecycleSession {
                status: protocol_status_with_stride(stride),
                capture_event: capture_event(stride),
            });
            *self.session.lock().unwrap() = Some(Arc::clone(&session));
            Box::pin(std::future::ready(Ok(
                session as Arc<dyn BrowserSessionPort>
            )))
        }
    }

    fn capture_event(stride: EveryNthFrame) -> BrowserSessionEvent {
        BrowserSessionEvent::CaptureStateChanged {
            status: TargetCaptureStatus::new(
                target_id(),
                1,
                CaptureStreamState::Capturing,
                CaptureStatistics::default(),
                1,
                0,
                None,
                CaptureTimingSummary::empty(),
                CaptureTimingSummary::empty(),
                stride,
            )
            .unwrap(),
        }
    }

    fn protocol_status() -> BrowserStatus {
        protocol_status_with_stride(EveryNthFrame::default())
    }

    fn protocol_status_with_stride(stride: EveryNthFrame) -> BrowserStatus {
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
            stride,
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

    async fn invoke_control_tools(
        connector: Arc<LifecycleConnector>,
        calls: Vec<(&'static str, Value)>,
    ) -> Vec<Value> {
        let browser: Arc<dyn BrowserConnector> = connector;
        let service = build_service(
            dependencies(browser),
            McpConfig::new(vec![CapabilityId::Control]).unwrap(),
        )
        .unwrap();
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let server_task = tokio::spawn(async move {
            let running = service.server.serve(server_io).await.unwrap();
            running.waiting().await.unwrap();
        });
        let (read, mut write) = tokio::io::split(client_io);
        let mut read = BufReader::new(read);
        send_json(
            &mut write,
            json!({
                "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                    "protocolVersion":"2025-06-18","capabilities":{},
                    "clientInfo":{"name":"lifecycle-route-test","version":"1"}
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

        let mut responses = Vec::with_capacity(calls.len());
        for (index, (name, arguments)) in calls.into_iter().enumerate() {
            send_json(
                &mut write,
                json!({
                    "jsonrpc":"2.0",
                    "id": index + 2,
                    "method":"tools/call",
                    "params":{"name":name,"arguments":arguments}
                }),
            )
            .await;
            responses.push(read_json(&mut read).await);
        }
        drop(write);
        drop(read);
        let _ = server_task.await;
        responses
    }

    async fn invoke_temporal_tool(
        dependencies: McpDependencies,
        name: &'static str,
        arguments: Value,
    ) -> Value {
        let service = build_service(
            dependencies,
            McpConfig::new(vec![
                CapabilityId::TemporalVision,
                CapabilityId::BrowserEvents,
            ])
            .unwrap(),
        )
        .unwrap();
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let server_task = tokio::spawn(async move {
            let running = service.server.serve(server_io).await.unwrap();
            running.waiting().await.unwrap();
        });
        let (read, mut write) = tokio::io::split(client_io);
        let mut read = BufReader::new(read);
        send_json(
            &mut write,
            json!({
                "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                    "protocolVersion":"2025-06-18","capabilities":{},
                    "clientInfo":{"name":"temporal-route-test","version":"1"}
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
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        )
        .await;
        let response = read_json(&mut read).await;
        drop(write);
        drop(read);
        let _ = server_task.await;
        response
    }

    async fn invoke_bundle_and_read_resource(
        dependencies: McpDependencies,
        arguments: Value,
        artifact_uri: &str,
        manifest_uri: &str,
    ) -> (Value, Value, Value, Value) {
        let service = build_service(
            dependencies,
            McpConfig::new(vec![
                CapabilityId::TemporalVision,
                CapabilityId::BrowserEvents,
            ])
            .unwrap(),
        )
        .unwrap();
        let (client_io, server_io) = tokio::io::duplex(256 * 1024);
        let server_task = tokio::spawn(async move {
            let running = service.server.serve(server_io).await.unwrap();
            running.waiting().await.unwrap();
        });
        let (read, mut write) = tokio::io::split(client_io);
        let mut read = BufReader::new(read);
        send_json(
            &mut write,
            json!({
                "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                    "protocolVersion":"2025-06-18","capabilities":{},
                    "clientInfo":{"name":"temporal-artifact-resource-test","version":"1"}
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
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"temporal_debug_bundle","arguments":arguments}}),
        )
        .await;
        let bundle = read_json(&mut read).await;
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":artifact_uri}}),
        )
        .await;
        let artifact_resource = read_json(&mut read).await;
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":manifest_uri}}),
        )
        .await;
        let manifest_resource = read_json(&mut read).await;
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":artifact_uri}}),
        )
        .await;
        let mismatch = read_json(&mut read).await;
        drop(write);
        drop(read);
        let _ = server_task.await;
        (bundle, artifact_resource, manifest_resource, mismatch)
    }

    #[tokio::test]
    async fn temporal_routes_dispatch_exact_domain_requests_and_reject_invalid_input_before_calls()
    {
        let spy = TemporalSpy::new();
        let bundle = bundle_request();
        let bundle_wire = serde_json::to_value(&bundle).unwrap();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "temporal_debug_bundle",
            bundle_wire.clone(),
        )
        .await;
        assert_eq!(spy.bundle_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            spy.bundle_request.lock().unwrap().as_ref(),
            Some(&bundle_wire)
        );

        let range = resolved_range();
        let progressive = krometrail_core::ProgressiveEvidenceRequest::QueryPinState(
            krometrail_core::ResolvedRangeEvidenceRequest::new(range.clone()).unwrap(),
        );
        let progressive_value = serde_json::to_value(&progressive).unwrap();
        let progressive_wire = progressive_value["request"].clone();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "query_pin_state",
            progressive_wire.clone(),
        )
        .await;
        assert_eq!(spy.progressive_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            spy.progressive_request.lock().unwrap().as_ref(),
            Some(&serde_json::to_value(&progressive).unwrap())
        );

        let source_limits = SourceReadLimitsRequest::new(1, 1024, 2048).unwrap();
        let list_request = SourceFramesRequest::new(
            range.clone(),
            SourceFrameSelection::ResolvedOrder,
            source_limits,
        )
        .unwrap();
        let list_operation = ProgressiveEvidenceRequest::ListSourceFrames(list_request);
        let list_wire = serde_json::to_value(&list_operation).unwrap();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "list_source_frames",
            list_wire["request"].clone(),
        )
        .await;
        assert_eq!(spy.progressive_calls.load(Ordering::SeqCst), 2);
        assert_eq!(
            spy.progressive_request.lock().unwrap().as_ref(),
            Some(&list_wire)
        );

        let fetch_request = SourceFramesRequest::new(
            range.clone(),
            SourceFrameSelection::Ids(vec![frame_id()]),
            SourceReadLimitsRequest::new(1, 1024, 1024).unwrap(),
        )
        .unwrap();
        let fetch_operation = ProgressiveEvidenceRequest::FetchSourceFrames(fetch_request);
        let fetch_wire = serde_json::to_value(&fetch_operation).unwrap();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "fetch_source_frames",
            fetch_wire["request"].clone(),
        )
        .await;
        assert_eq!(spy.progressive_calls.load(Ordering::SeqCst), 3);
        assert_eq!(
            spy.progressive_request.lock().unwrap().as_ref(),
            Some(&fetch_wire)
        );

        let normalization = NormalizationRequest::new(
            None,
            temporal_vision::Rgb8::new(0, 0, 0),
            AnalysisScale::Identity,
        )
        .unwrap();
        let generator = ArtifactGeneratorRequest::Storyboard(StoryboardRequest {
            anchor: SessionTime::from_nanos(5),
            tile_limit: 3,
            noise_floor: 0,
            normalization,
            labels: ArtifactLabelsRequest::new(
                NonEmptyText::new("qualification").unwrap(),
                NonEmptyText::new("schema-v5").unwrap(),
            ),
            include_orientation: true,
            output: OutputLimitsRequest::new(64, 64, 1024).unwrap(),
        });
        let artifact_operation = ProgressiveEvidenceRequest::GenerateArtifacts(
            GenerateArtifactsRequest::new(
                ArtifactGenerationRequest::new(
                    range.clone(),
                    vec![],
                    vec![generator],
                    ArtifactFailurePolicy::AllowPartial,
                )
                .unwrap(),
            )
            .unwrap(),
        );
        let artifact_wire = serde_json::to_value(&artifact_operation).unwrap();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "generate_artifacts",
            artifact_wire["request"].clone(),
        )
        .await;
        assert_eq!(spy.progressive_calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            spy.progressive_request.lock().unwrap().as_ref(),
            Some(&artifact_wire)
        );

        let region = ProgressiveRegion::SourcePixels {
            rect: temporal_vision::SignedPixelRect::new(
                0,
                0,
                NonZeroU32::new(1).unwrap(),
                NonZeroU32::new(1).unwrap(),
            )
            .unwrap(),
            source_frame_id: frame_id(),
        };
        let region_operation = ProgressiveEvidenceRequest::GenerateRegionFilmstrip(
            RegionFilmstripEvidenceRequest::new(
                range.clone(),
                region,
                vec![],
                SessionTime::from_nanos(5),
                3,
                temporal_vision::Rgb8::new(0, 0, 0),
                temporal_vision::Rgb8::new(255, 255, 255),
                AnalysisScale::Identity,
                ArtifactLabelsRequest::new(
                    NonEmptyText::new("qualification").unwrap(),
                    NonEmptyText::new("schema-v5").unwrap(),
                ),
                OutputLimitsRequest::new(64, 64, 1024).unwrap(),
            )
            .unwrap(),
        );
        let region_wire = serde_json::to_value(&region_operation).unwrap();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "generate_region_filmstrip",
            region_wire["request"].clone(),
        )
        .await;
        assert_eq!(spy.progressive_calls.load(Ordering::SeqCst), 5);
        assert_eq!(
            spy.progressive_request.lock().unwrap().as_ref(),
            Some(&region_wire)
        );

        for name in ["pin_resolved_range", "unpin_resolved_range"] {
            let pin_operation = match name {
                "pin_resolved_range" => ProgressiveEvidenceRequest::PinResolvedRange(
                    ResolvedRangeEvidenceRequest::new(range.clone()).unwrap(),
                ),
                _ => ProgressiveEvidenceRequest::UnpinResolvedRange(
                    ResolvedRangeEvidenceRequest::new(range.clone()).unwrap(),
                ),
            };
            let pin_wire = serde_json::to_value(&pin_operation).unwrap();
            let _ = invoke_temporal_tool(
                dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
                name,
                pin_wire["request"].clone(),
            )
            .await;
            assert_eq!(
                spy.progressive_request.lock().unwrap().as_ref(),
                Some(&pin_wire)
            );
        }
        assert_eq!(spy.progressive_calls.load(Ordering::SeqCst), 7);

        let detail = BrowserEventDetailRequest::new(
            range,
            None,
            BrowserEventFilter::default(),
            1,
            None,
            vec![],
        )
        .unwrap();
        let detail_wire = serde_json::to_value(&detail).unwrap();
        let _ = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "query_browser_events",
            detail_wire.clone(),
        )
        .await;
        assert_eq!(spy.context_calls.load(Ordering::SeqCst), 1);
        assert_eq!(
            spy.context_request.lock().unwrap().as_ref(),
            Some(&serde_json::to_value(detail.context_request()).unwrap())
        );

        let compact = serde_json::to_value(
            TemporalContextRequest::compact(resolved_range(), vec![]).unwrap(),
        )
        .unwrap();
        let compact_response = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "query_browser_events",
            compact,
        )
        .await;
        assert!(
            compact_response["result"]["isError"]
                .as_bool()
                .unwrap_or(false)
        );
        assert_eq!(spy.context_calls.load(Ordering::SeqCst), 1);

        let invalid = invoke_temporal_tool(
            dependencies_with_spy(Arc::new(UnusedConnector), Arc::clone(&spy)),
            "query_browser_events",
            json!({"unexpected": true}),
        )
        .await;
        assert!(invalid["result"]["isError"].as_bool().unwrap_or(false));
        assert_eq!(spy.context_calls.load(Ordering::SeqCst), 1);
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
            dependencies(Arc::new(ProtocolConnector { session })),
            McpConfig::new(vec![CapabilityId::Control]).unwrap(),
        )
        .unwrap();
        let owner = Arc::clone(&service.sessions);
        assert!(Arc::ptr_eq(&owner, &service.server.sessions));
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
            json!({"jsonrpc":"2.0","id":2,"method":"resources/templates/list","params":{}}),
        )
        .await;
        assert_eq!(
            read_json(&mut read).await["result"]["resourceTemplates"],
            json!([])
        );
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":3,"method":"tools/list","params":{}}),
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
            json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"start_browser","arguments":{}}}),
        )
        .await;
        assert_eq!(read_json(&mut read).await["result"]["isError"], false);
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"list_pages","arguments":{}}}),
        )
        .await;
        let valid = read_json(&mut read).await;
        assert_eq!(valid["result"]["isError"], false);
        assert_eq!(valid["result"]["structuredContent"]["status"], "succeeded");
        assert_eq!(execute_calls.load(Ordering::SeqCst), 1);

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"navigate_page","arguments":{"url":""}}}),
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

    #[tokio::test]
    async fn successful_temporal_bundle_exposes_canonical_artifact_resource_end_to_end() {
        use base64::{Engine as _, engine::general_purpose::STANDARD};

        let (spy, uri, bytes) = successful_bundle_fixture();
        let scope = EvidenceScope::new(session_id(), target_id()).unwrap();
        let manifest_uri =
            crate::resources::EvidenceResourceUri::artifact_manifest(scope, artifact_id())
                .canonical_uri();
        let expected_manifest =
            serde_json::to_string(&spy.artifact_read.handle.provenance).unwrap();
        let BundleArtifactEvidence::Available(generation) = &spy.bundle.artifacts else {
            unreachable!()
        };
        let generic = crate::response::map_progressive_result(
            "generate_artifacts",
            ProgressiveEvidenceResult::GenerateArtifacts(Box::new(generation.clone())),
        )
        .unwrap();
        assert_eq!(
            generic.response.result["outcomes"][0]["artifact"]["manifest"],
            serde_json::to_value(&spy.artifact_read.handle.provenance).unwrap(),
            "generic artifact generation must retain the complete manifest"
        );
        assert_eq!(generic.response.resources.len(), 1);
        let arguments = serde_json::to_value(bundle_request()).unwrap();
        let dependencies = McpDependencies {
            browser: Arc::new(UnusedConnector),
            temporal_debug_bundles: Arc::clone(&spy) as Arc<dyn TemporalDebugBundles>,
            progressive_evidence: Arc::clone(&spy) as Arc<dyn ProgressiveEvidence>,
            temporal_context: spy as Arc<dyn TemporalContextQuery>,
            temporal_video: None,
            diagnostics: DiagnosticContext::default(),
        };
        let (bundle, resource, manifest_resource, mismatch) =
            invoke_bundle_and_read_resource(dependencies, arguments, &uri, &manifest_uri).await;

        assert_eq!(bundle["result"]["isError"], false);
        let structured = &bundle["result"]["structuredContent"];
        assert_eq!(structured["status"], "succeeded");
        let image_metadata = &structured["images"][0]["metadata"];
        assert_eq!(image_metadata["kind"], "artifact");
        assert_eq!(image_metadata["media_type"], "image/png");
        assert_eq!(image_metadata["encoded_byte_len"], bytes.len());
        assert_eq!(image_metadata["width"], 1);
        assert_eq!(image_metadata["height"], 1);
        let structured_resource = &structured["resources"][0];
        assert_eq!(structured_resource["uri"], uri);
        assert_eq!(structured_resource["mime_type"], "image/png");
        assert_eq!(structured_resource["encoded_byte_len"], bytes.len());
        let manifest_link = &structured["resources"][1];
        assert_eq!(manifest_link["uri"], manifest_uri);
        assert_eq!(manifest_link["mime_type"], "application/json");
        assert_eq!(manifest_link["encoded_byte_len"], expected_manifest.len());
        let compact = &structured["result"]["artifacts"]["outcomes"][0]["artifact"];
        assert!(compact.get("manifest").is_none());
        assert!(compact.get("source_frame_ids").is_none());
        assert_eq!(compact["manifest_uri"], manifest_uri);
        assert_eq!(compact["source_frame_count"], 1);
        assert_eq!(compact["selected_frame_count"], 0);
        assert_eq!(compact["omitted_frame_count"], 1);
        let output_hash = compact["output_hash"].as_str().unwrap();
        assert_eq!(output_hash, Sha256Digest::digest(&bytes).to_string());

        let content = bundle["result"]["content"].as_array().unwrap();
        let inline = content
            .iter()
            .find(|item| item["type"] == "image")
            .expect("successful artifact must be inline in the bundle");
        assert_eq!(inline["mimeType"], "image/png");
        assert_eq!(
            STANDARD.decode(inline["data"].as_str().unwrap()).unwrap(),
            bytes.as_ref()
        );
        let link = content
            .iter()
            .find(|item| item["type"] == "resource_link")
            .expect("bundle must publish a ResourceLink");
        assert_eq!(link["uri"], uri);
        assert_eq!(link["mimeType"], "image/png");
        assert_eq!(link["size"], bytes.len());

        assert_eq!(resource["result"]["contents"][0]["uri"], uri);
        assert_eq!(resource["result"]["contents"][0]["mimeType"], "image/png");
        assert_eq!(
            STANDARD
                .decode(resource["result"]["contents"][0]["blob"].as_str().unwrap())
                .unwrap(),
            bytes.as_ref()
        );
        assert_eq!(
            manifest_resource["result"]["contents"][0]["uri"],
            manifest_uri
        );
        assert_eq!(
            manifest_resource["result"]["contents"][0]["mimeType"],
            "application/json"
        );
        assert_eq!(
            manifest_resource["result"]["contents"][0]["text"],
            expected_manifest
        );
        assert_eq!(
            mismatch["error"]["message"],
            "resource handle identity mismatch"
        );
    }

    #[tokio::test]
    async fn temporal_resource_protocol_lists_templates_reads_strictly_and_closes_on_eof() {
        let service = build_service(
            dependencies(Arc::new(UnusedConnector)),
            McpConfig::new(vec![CapabilityId::TemporalVision]).unwrap(),
        )
        .unwrap();
        assert!(service.server().get_info().capabilities.resources.is_some());
        assert!(service.server().get_info().capabilities.tools.is_some());
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
                    "clientInfo":{"name":"resource-test","version":"1"}
                }
            }),
        )
        .await;
        let initialized = read_json(&mut read).await;
        assert_eq!(initialized["result"]["protocolVersion"], "2025-06-18");
        assert!(initialized["result"]["capabilities"]["tools"].is_object());
        assert!(initialized["result"]["capabilities"]["resources"].is_object());
        assert!(initialized["result"]["capabilities"]["tasks"].is_null());
        assert!(initialized["result"]["capabilities"]["resources"]["subscribe"].is_null());
        assert!(initialized["result"]["capabilities"]["resources"]["listChanged"].is_null());
        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
        )
        .await;

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":2,"method":"resources/list","params":{}}),
        )
        .await;
        let resources = read_json(&mut read).await;
        assert_eq!(resources["result"]["resources"], json!([]));

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":3,"method":"resources/templates/list","params":{}}),
        )
        .await;
        let templates = read_json(&mut read).await;
        let templates = templates["result"]["resourceTemplates"].as_array().unwrap();
        assert_eq!(templates.len(), 3);
        assert_eq!(
            templates[0]["uriTemplate"],
            "krometrail://evidence/{session}/{target}/artifacts/{id}"
        );
        assert_eq!(templates[0]["mimeType"], "image/png");
        assert_eq!(
            templates[1]["uriTemplate"],
            "krometrail://evidence/{session}/{target}/artifact-manifests/{id}"
        );
        assert_eq!(templates[1]["mimeType"], "application/json");
        assert_eq!(
            templates[2]["uriTemplate"],
            "krometrail://evidence/{session}/{target}/frames/{id}"
        );
        assert!(templates[2].get("mimeType").is_none());

        send_json(
            &mut write,
            json!({
                "jsonrpc":"2.0","id":4,"method":"resources/read",
                "params":{"uri":"file:///tmp/not-an-evidence-resource"}
            }),
        )
        .await;
        let malformed = read_json(&mut read).await;
        assert_eq!(malformed["error"]["code"], -32602);

        send_json(
            &mut write,
            json!({"jsonrpc":"2.0","id":5,"method":"resources/subscribe","params":{"uri":"x"}}),
        )
        .await;
        let unsupported = read_json(&mut read).await;
        assert_eq!(unsupported["error"]["code"], -32601);

        drop(write);
        drop(read);
        assert!(matches!(server_task.await.unwrap(), QuitReason::Closed));
    }
}
