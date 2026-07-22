use std::{
    collections::BTreeSet,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::FutureExt as _;
use krometrail_core::{
    ArtifactGenerationContext, AttachBrowser, BROWSER_OPERATION_REGISTRY,
    BrowserEventDetailRequest, BrowserOperationContext, BrowserOperationKind,
    BrowserOperationRequest, CancellationSignal, CapabilityId, CurrentReferenceGeometry, ErrorCode,
    KrometrailError, LaunchBrowser, NonEmptyText, OperationExposure, OperationMutability,
    PortFuture, ProgressiveEvidenceContext, ProgressiveEvidenceOperationKind,
    ProgressiveEvidenceRequest, ResolvedRange, ResolvedRangeHandleId, Result, RetryAdvice,
    TEMPORAL_CONTEXT_OPERATION_REGISTRY, TEMPORAL_DEBUG_BUNDLE_OPERATION,
    TEMPORAL_RANGE_RESOLUTION_OPERATION, TEMPORAL_VIDEO_OPERATION, TemporalContextOperationKind,
    TemporalDebugBundleContext, TemporalDebugBundleRequest, TemporalQueryRequest,
    TemporalVideoGenerationRequest,
};
use rmcp::{
    handler::server::tool::{ToolCallContext, ToolRoute, ToolRouter},
    model::{EmptyObject, Tool, ToolAnnotations},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{McpConfig, McpDependencies},
    response::{
        ResponseRequest, into_call_tool_result, map_browser_status, map_lifecycle_result,
        map_operation_result_with_novelty, map_progressive_result, map_temporal_bundle_result,
        map_temporal_context_result, map_temporal_range_resolution_result,
        map_temporal_video_result, split_response_request, visible_error,
        visible_error_with_capture,
    },
    schema::{
        ResolvedRangeHandleArgument, generated_input_schema, operation_input_schema,
        projected_input_schema, range_handle_input_schema, tool_response_schema, type_input_schema,
    },
    server::KrometrailMcpServer,
    session::BrowserSessionOwner,
};

const LIFECYCLE_TOOLS: &[LifecycleTool] = &[
    LifecycleTool {
        name: "start_browser",
        description: "Launch one controlled browser with a managed profile.",
        kind: LifecycleKind::Start,
    },
    LifecycleTool {
        name: "attach_browser",
        description: "Attach to one explicitly configured local Chromium endpoint.",
        kind: LifecycleKind::Attach,
    },
    LifecycleTool {
        name: "browser_status",
        description: "Report the active controlled-browser session status.",
        kind: LifecycleKind::Status,
    },
    LifecycleTool {
        name: "stop_browser",
        description: "Close a managed browser or detach from an attached browser.",
        kind: LifecycleKind::Stop,
    },
    LifecycleTool {
        name: "list_managed_profiles",
        description: "List reusable Krometrail-managed profile identities without filesystem paths or browser data.",
        kind: LifecycleKind::Profiles,
    },
];

#[derive(Clone, Copy)]
struct LifecycleTool {
    name: &'static str,
    description: &'static str,
    kind: LifecycleKind,
}

#[derive(Clone, Copy)]
enum LifecycleKind {
    Start,
    Attach,
    Status,
    Stop,
    Profiles,
}

pub(crate) const MCP_REQUEST_DEADLINE: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub(crate) struct McpCancellation(CancellationToken);

impl McpCancellation {
    pub(crate) fn new(token: CancellationToken) -> Self {
        Self(token)
    }
}

impl CancellationSignal for McpCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(self.0.cancelled())
    }
}

struct RequestBudget {
    cancellation: Arc<McpCancellation>,
    deadline: Instant,
}

impl RequestBudget {
    fn new(token: CancellationToken) -> Self {
        Self {
            cancellation: Arc::new(McpCancellation::new(token)),
            deadline: Instant::now() + MCP_REQUEST_DEADLINE,
        }
    }

    fn check(&self) -> Result<()> {
        if self.cancellation.is_cancelled() {
            return Err(request_cancelled_error());
        }
        if Instant::now() >= self.deadline {
            return Err(request_deadline_error());
        }
        Ok(())
    }

    async fn run<T>(&self, future: impl Future<Output = Result<T>>) -> Result<T> {
        tokio::select! {
            result = future => result,
            () = self.cancellation.cancelled() => Err(request_cancelled_error()),
            () = tokio::time::sleep_until(tokio::time::Instant::from_std(self.deadline)) => {
                Err(request_deadline_error())
            }
        }
    }
}

pub(crate) fn build_router(
    config: &McpConfig,
    dependencies: Arc<McpDependencies>,
    sessions: Arc<BrowserSessionOwner>,
) -> Result<ToolRouter<KrometrailMcpServer>> {
    validate_route_registry(config)?;
    let mut router = ToolRouter::new();

    if config.is_enabled(CapabilityId::Control) {
        for lifecycle in LIFECYCLE_TOOLS {
            router.add_route(lifecycle_route(*lifecycle)?);
        }
        for definition in BROWSER_OPERATION_REGISTRY {
            if !config.is_enabled(definition.capability) {
                continue;
            }
            let kind = definition.kind;
            let name = definition.stable_name;
            let input_schema = projected_input_schema(operation_input_schema(kind, config)?)?;
            let annotations = operation_annotations(definition.mutability);
            let mut tool =
                Tool::new(name, definition.description, input_schema).annotate(annotations);
            tool.output_schema = Some(tool_response_schema(false)?);
            router.add_route(ToolRoute::new_dyn(tool, move |context| {
                async move { call_operation(context, kind, name).await }.boxed()
            }));
        }
    }

    if config.is_enabled(TEMPORAL_DEBUG_BUNDLE_OPERATION.capability) {
        let definition = &TEMPORAL_DEBUG_BUNDLE_OPERATION;
        let name = definition.stable_name;
        let mut tool = Tool::new(
            name,
            definition.description,
            projected_input_schema(type_input_schema::<TemporalDebugBundleRequest>()?)?,
        )
        .annotate(temporal_annotations(definition.mutability, false));
        tool.output_schema = Some(tool_response_schema(false)?);
        let dependencies = Arc::clone(&dependencies);
        router.add_route(ToolRoute::new_dyn(tool, move |context| {
            let dependencies = Arc::clone(&dependencies);
            async move { call_bundle(context, dependencies, name).await }.boxed()
        }));
    }

    if config.is_enabled(TEMPORAL_RANGE_RESOLUTION_OPERATION.capability) {
        let definition = &TEMPORAL_RANGE_RESOLUTION_OPERATION;
        let name = definition.stable_name;
        let mut tool = Tool::new(
            name,
            definition.description,
            projected_input_schema(type_input_schema::<TemporalQueryRequest>()?)?,
        )
        .annotate(temporal_annotations(definition.mutability, false));
        tool.output_schema = Some(tool_response_schema(false)?);
        let dependencies = Arc::clone(&dependencies);
        router.add_route(ToolRoute::new_dyn(tool, move |context| {
            let dependencies = Arc::clone(&dependencies);
            async move { call_resolve_temporal_range(context, dependencies, name).await }.boxed()
        }));
    }

    if config.is_enabled(TEMPORAL_VIDEO_OPERATION.capability) {
        let definition = &TEMPORAL_VIDEO_OPERATION;
        let name = definition.stable_name;
        let mut tool = Tool::new(
            name,
            definition.description,
            projected_input_schema(range_handle_input_schema(type_input_schema::<
                TemporalVideoGenerationRequest,
            >()?)?)?,
        )
        .annotate(temporal_annotations(definition.mutability, false));
        tool.output_schema = Some(tool_response_schema(true)?);
        let dependencies = Arc::clone(&dependencies);
        router.add_route(ToolRoute::new_dyn(tool, move |context| {
            let dependencies = Arc::clone(&dependencies);
            async move { call_temporal_video(context, dependencies, name).await }.boxed()
        }));
    }

    let current_geometry: Arc<dyn CurrentReferenceGeometry> = sessions;
    for definition in krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY
        .iter()
        .filter(|definition| {
            definition.exposure == OperationExposure::Tool
                && config.is_enabled(definition.capability)
        })
    {
        let kind = definition.kind;
        let name = definition.stable_name;
        let input_schema = progressive_input_schema(kind)?;
        let annotations = temporal_annotations(
            definition.mutability,
            kind == ProgressiveEvidenceOperationKind::UnpinResolvedRange,
        );
        let mut tool = Tool::new(name, definition.description, input_schema).annotate(annotations);
        tool.output_schema = Some(tool_response_schema(false)?);
        let dependencies = Arc::clone(&dependencies);
        let current_geometry = Arc::clone(&current_geometry);
        router.add_route(ToolRoute::new_dyn(tool, move |context| {
            let dependencies = Arc::clone(&dependencies);
            let current_geometry = Arc::clone(&current_geometry);
            async move {
                    call_progressive(context, dependencies, current_geometry, kind, name).await
                }
                .boxed()
        }));
    }

    if config.is_enabled(CapabilityId::BrowserEvents) {
        for definition in TEMPORAL_CONTEXT_OPERATION_REGISTRY {
            let kind = definition.kind;
            let name = definition.stable_name;
            let input_schema = projected_input_schema(range_handle_input_schema(
                generated_input_schema(kind.input_schema())?,
            )?)?;
            let mut tool = Tool::new(name, definition.description, input_schema)
                .annotate(temporal_annotations(definition.mutability, false));
            tool.output_schema = Some(tool_response_schema(false)?);
            let dependencies = Arc::clone(&dependencies);
            router.add_route(ToolRoute::new_dyn(tool, move |context| {
                let dependencies = Arc::clone(&dependencies);
                async move { call_context(context, dependencies, kind, name).await }.boxed()
            }));
        }
    }

    Ok(router)
}

fn validate_route_registry(config: &McpConfig) -> Result<()> {
    let mut names = BTreeSet::new();
    for lifecycle in LIFECYCLE_TOOLS {
        register_route_name(&mut names, lifecycle.name, lifecycle.description)?;
        let _ = lifecycle_route(*lifecycle)?;
    }
    if BROWSER_OPERATION_REGISTRY.len() != krometrail_core::BrowserOperationKind::ALL.len() {
        return Err(registry_error(
            "browser operation registry is missing a declared operation",
        ));
    }
    for definition in BROWSER_OPERATION_REGISTRY {
        if definition.kind.stable_name() != definition.stable_name {
            return Err(registry_error(
                "browser operation registry has a mismatched stable name",
            ));
        }
        register_route_name(&mut names, definition.stable_name, definition.description)?;
        let _ = projected_input_schema(operation_input_schema(definition.kind, config)?)?;
    }
    let progressive_registry = krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY;
    if progressive_registry.len() != ProgressiveEvidenceOperationKind::ALL.len()
        || progressive_registry
            .iter()
            .zip(ProgressiveEvidenceOperationKind::ALL)
            .any(|(definition, kind)| definition.kind != *kind)
    {
        return Err(registry_error(
            "progressive evidence registry is missing a declared operation",
        ));
    }
    for definition in progressive_registry {
        if definition.kind.as_str() != definition.stable_name {
            return Err(registry_error(
                "progressive evidence registry has a mismatched stable name",
            ));
        }
        register_route_name(&mut names, definition.stable_name, definition.description)?;
        let _ = progressive_input_schema(definition.kind)?;
    }
    register_route_name(
        &mut names,
        TEMPORAL_DEBUG_BUNDLE_OPERATION.stable_name,
        TEMPORAL_DEBUG_BUNDLE_OPERATION.description,
    )?;
    let _ = projected_input_schema(type_input_schema::<TemporalDebugBundleRequest>()?)?;
    register_route_name(
        &mut names,
        TEMPORAL_RANGE_RESOLUTION_OPERATION.stable_name,
        TEMPORAL_RANGE_RESOLUTION_OPERATION.description,
    )?;
    let _ = projected_input_schema(type_input_schema::<TemporalQueryRequest>()?)?;
    register_route_name(
        &mut names,
        TEMPORAL_VIDEO_OPERATION.stable_name,
        TEMPORAL_VIDEO_OPERATION.description,
    )?;
    let _ = projected_input_schema(range_handle_input_schema(type_input_schema::<
        TemporalVideoGenerationRequest,
    >()?)?)?;
    if TEMPORAL_CONTEXT_OPERATION_REGISTRY.len() != TemporalContextOperationKind::ALL.len()
        || TEMPORAL_CONTEXT_OPERATION_REGISTRY
            .iter()
            .zip(TemporalContextOperationKind::ALL)
            .any(|(definition, kind)| definition.kind != *kind)
    {
        return Err(registry_error(
            "temporal context registry is missing a declared operation",
        ));
    }
    for definition in TEMPORAL_CONTEXT_OPERATION_REGISTRY {
        if definition.kind.stable_name() != definition.stable_name {
            return Err(registry_error(
                "temporal context registry has a mismatched stable name",
            ));
        }
        register_route_name(&mut names, definition.stable_name, definition.description)?;
        let _ = projected_input_schema(range_handle_input_schema(generated_input_schema(
            definition.kind.input_schema(),
        )?)?)?;
    }
    Ok(())
}

fn progressive_input_schema(
    kind: ProgressiveEvidenceOperationKind,
) -> Result<Arc<rmcp::model::JsonObject>> {
    let base = generated_input_schema(kind.input_schema())?;
    let base = if progressive_accepts_range_handle(kind) {
        range_handle_input_schema(base)?
    } else {
        base
    };
    projected_input_schema(base)
}

const fn progressive_accepts_range_handle(kind: ProgressiveEvidenceOperationKind) -> bool {
    matches!(
        kind,
        ProgressiveEvidenceOperationKind::ListSourceFrames
            | ProgressiveEvidenceOperationKind::FetchSourceFrames
            | ProgressiveEvidenceOperationKind::GenerateArtifacts
            | ProgressiveEvidenceOperationKind::GenerateRegionFilmstrip
            | ProgressiveEvidenceOperationKind::PinResolvedRange
            | ProgressiveEvidenceOperationKind::UnpinResolvedRange
            | ProgressiveEvidenceOperationKind::QueryPinState
    )
}

const fn progressive_inline_image_default(kind: ProgressiveEvidenceOperationKind) -> bool {
    matches!(
        kind,
        ProgressiveEvidenceOperationKind::FetchSourceFrames
            | ProgressiveEvidenceOperationKind::GenerateArtifacts
            | ProgressiveEvidenceOperationKind::GenerateRegionFilmstrip
    )
}

fn resolve_progressive_response(
    preference: ResponseRequest,
    kind: ProgressiveEvidenceOperationKind,
) -> ResponseRequest {
    if kind == ProgressiveEvidenceOperationKind::FetchSourceFrames {
        preference
    } else {
        preference.with_inline_default(progressive_inline_image_default(kind))
    }
}

const fn browser_inline_image_default(kind: BrowserOperationKind) -> bool {
    matches!(
        kind,
        BrowserOperationKind::TakeScreenshot
            | BrowserOperationKind::ObserveLive
            | BrowserOperationKind::Scroll
            | BrowserOperationKind::SetViewport
            | BrowserOperationKind::ActivatePage
    )
}

async fn call_temporal_video(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    dependencies: Arc<McpDependencies>,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let sessions = context.service.sessions();
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let (arguments, _) = match split_response_request(context.arguments.unwrap_or_default()) {
        Ok(value) => value,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let (arguments, supplied_handle) = match budget
        .run(resolve_range_argument(
            arguments,
            dependencies.range_handles.as_ref(),
        ))
        .await
    {
        Ok(value) => value,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let request = match parse_arguments::<TemporalVideoGenerationRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let handle = match budget
        .run(range_handle_for_request(
            dependencies.range_handles.as_ref(),
            supplied_handle,
            request.range(),
        ))
        .await
    {
        Ok(handle) => handle,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let cancellation: Arc<dyn CancellationSignal> = budget.cancellation.clone();
    let Some(service) = dependencies.temporal_video.as_ref() else {
        return Err(rmcp::ErrorData::internal_error(
            "temporal video service is missing for a registered route",
            None,
        ));
    };
    // The retained-video service owns the cancellation/deadline race because it must
    // cancel and drain any in-flight durable publication before returning. Wrapping
    // this future in `RequestBudget::run` would drop that cleanup path when MCP wins
    // the same race.
    let result = service
        .generate_video(
            request,
            ArtifactGenerationContext {
                deadline: Some(budget.deadline),
                cancellation: Some(Arc::clone(&cancellation)),
                ..Default::default()
            },
        )
        .await;
    match result {
        Ok(result) => {
            let health = capture_health(sessions).await;
            map_temporal_video_result(name, result)
                .map(|mapped| mapped.with_range_handle(handle))
                .map_err(|_| {
                    rmcp::ErrorData::internal_error("temporal video response mapping failed", None)
                })
                .and_then(|mapped| into_call_tool_result(mapped, &health))
        }
        Err(error) => Ok(call_error_result(name, error)),
    }
}

fn register_route_name(
    names: &mut BTreeSet<&'static str>,
    name: &'static str,
    description: &'static str,
) -> Result<()> {
    if name.trim().is_empty() || description.trim().is_empty() || !names.insert(name) {
        return Err(registry_error(
            "MCP operation registry contains an invalid or duplicate route",
        ));
    }
    Ok(())
}

fn registry_error(message: &'static str) -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Internal,
        NonEmptyText::new(message).expect("static registry error is non-empty"),
    )
}

fn call_error_result(name: &'static str, error: KrometrailError) -> rmcp::model::CallToolResult {
    visible_error(name, error)
}

async fn call_bundle(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    dependencies: Arc<McpDependencies>,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let sessions = context.service.sessions();
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let (arguments, preference) =
        match split_response_request(context.arguments.unwrap_or_default()) {
            Ok((arguments, preference)) => (arguments, preference.with_inline_default(true)),
            Err(error) => return Ok(call_error_result(name, error)),
        };
    let request = match parse_arguments::<TemporalDebugBundleRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let cancellation: Arc<dyn CancellationSignal> = budget.cancellation.clone();
    let result = budget
        .run(dependencies.temporal_debug_bundles.bundle(
            request,
            TemporalDebugBundleContext {
                deadline: Some(budget.deadline),
                cancellation: Some(cancellation),
            },
        ))
        .await;
    match result {
        Ok(bundle) => {
            let handle = match budget
                .run(dependencies.range_handles.register(bundle.range.clone()))
                .await
            {
                Ok(handle) => handle,
                Err(error) => return Ok(call_error_result(name, error)),
            };
            let health = capture_health(sessions).await;
            map_temporal_bundle_result(
                name,
                bundle,
                dependencies.progressive_evidence.as_ref(),
                budget.deadline,
                budget.cancellation.clone(),
                preference,
            )
            .await
            .map(|mapped| mapped.with_range_handle(handle))
            .map_err(|_| {
                rmcp::ErrorData::internal_error("temporal bundle response mapping failed", None)
            })
            .and_then(|mapped| into_call_tool_result(mapped, &health))
        }
        Err(error) => Ok(call_error_result(name, error)),
    }
}

async fn call_resolve_temporal_range(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    dependencies: Arc<McpDependencies>,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let sessions = context.service.sessions();
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let (arguments, preference) =
        match split_response_request(context.arguments.unwrap_or_default()) {
            Ok((arguments, preference)) => (arguments, preference.with_inline_default(false)),
            Err(error) => return Ok(call_error_result(name, error)),
        };
    let request = match parse_arguments::<TemporalQueryRequest>(arguments) {
        Ok(request) => request,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let result = budget
        .run(dependencies.temporal_debug_bundles.resolve(
            request,
            TemporalDebugBundleContext {
                deadline: Some(budget.deadline),
                cancellation: Some(budget.cancellation.clone()),
            },
        ))
        .await;
    match result {
        Ok(result) => {
            let handle = match budget
                .run(dependencies.range_handles.register(result.range.clone()))
                .await
            {
                Ok(handle) => handle,
                Err(error) => return Ok(call_error_result(name, error)),
            };
            let health = capture_health(sessions).await;
            map_temporal_range_resolution_result(name, result, preference)
                .map(|mapped| mapped.with_range_handle(handle))
                .map_err(|_| {
                    rmcp::ErrorData::internal_error("temporal range response mapping failed", None)
                })
                .and_then(|mapped| into_call_tool_result(mapped, &health))
        }
        Err(error) => Ok(call_error_result(name, error)),
    }
}

async fn call_progressive(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    dependencies: Arc<McpDependencies>,
    current_geometry: Arc<dyn CurrentReferenceGeometry>,
    kind: ProgressiveEvidenceOperationKind,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let sessions = context.service.sessions();
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let (arguments, preference) =
        match split_response_request(context.arguments.unwrap_or_default()) {
            Ok((arguments, preference)) => {
                (arguments, resolve_progressive_response(preference, kind))
            }
            Err(error) => return Ok(call_error_result(name, error)),
        };
    let (arguments, supplied_handle) = match budget
        .run(resolve_range_argument(
            arguments,
            dependencies.range_handles.as_ref(),
        ))
        .await
    {
        Ok(value) => value,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let request = match progressive_request(name, Some(arguments)) {
        Ok(request) if request.kind() == kind => request,
        Ok(_) => {
            return Err(rmcp::ErrorData::internal_error(
                "tool registry request association failed",
                None,
            ));
        }
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let Some(range) = progressive_request_range(&request) else {
        return Err(rmcp::ErrorData::internal_error(
            "handle-enabled temporal route is missing its resolved range",
            None,
        ));
    };
    let handle = match budget
        .run(range_handle_for_request(
            dependencies.range_handles.as_ref(),
            supplied_handle,
            range,
        ))
        .await
    {
        Ok(handle) => handle,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let cancellation: Arc<dyn CancellationSignal> = budget.cancellation.clone();
    let result = budget
        .run(dependencies.progressive_evidence.execute(
            request,
            ProgressiveEvidenceContext {
                deadline: Some(budget.deadline),
                cancellation: Some(Arc::clone(&cancellation)),
                current_reference_geometry: Some(current_geometry),
            },
        ))
        .await;
    match result {
        Ok(result) => {
            let health = capture_health(sessions).await;
            map_progressive_result(
                name,
                result,
                dependencies.progressive_evidence.as_ref(),
                budget.deadline,
                cancellation,
                preference,
            )
            .await
            .map(|mapped| mapped.with_range_handle(handle))
            .map_err(|_| {
                rmcp::ErrorData::internal_error("progressive response mapping failed", None)
            })
            .and_then(|mapped| into_call_tool_result(mapped, &health))
        }
        Err(error) => Ok(call_error_result(name, error)),
    }
}

async fn call_context(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    dependencies: Arc<McpDependencies>,
    kind: TemporalContextOperationKind,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let sessions = context.service.sessions();
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let (arguments, preference) =
        match split_response_request(context.arguments.unwrap_or_default()) {
            Ok((arguments, preference)) => (arguments, preference.with_inline_default(false)),
            Err(error) => return Ok(call_error_result(name, error)),
        };
    let (arguments, supplied_handle) = match budget
        .run(resolve_range_argument(
            arguments,
            dependencies.range_handles.as_ref(),
        ))
        .await
    {
        Ok(value) => value,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let request = match kind {
        TemporalContextOperationKind::QueryBrowserEvents => {
            match parse_arguments::<BrowserEventDetailRequest>(arguments) {
                Ok(request) => request,
                Err(error) => return Ok(call_error_result(name, error)),
            }
        }
    };
    let handle = match budget
        .run(range_handle_for_request(
            dependencies.range_handles.as_ref(),
            supplied_handle,
            request.context_request().range(),
        ))
        .await
    {
        Ok(handle) => handle,
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let result = budget
        .run(
            dependencies
                .temporal_context
                .context(request.into_context_request()),
        )
        .await;
    match result {
        Ok(value) => {
            let health = capture_health(sessions).await;
            map_temporal_context_result(name, value, preference)
                .map(|mapped| mapped.with_range_handle(handle))
                .map_err(|_| {
                    rmcp::ErrorData::internal_error("browser event response mapping failed", None)
                })
                .and_then(|mapped| into_call_tool_result(mapped, &health))
        }
        Err(error) => Ok(call_error_result(name, error)),
    }
}

fn progressive_request(
    operation: &'static str,
    arguments: Option<rmcp::model::JsonObject>,
) -> Result<ProgressiveEvidenceRequest> {
    decode_value(json!({
        "operation": operation,
        "request": Value::Object(arguments.unwrap_or_default()),
    }))
}

async fn resolve_range_argument(
    mut arguments: rmcp::model::JsonObject,
    handles: &dyn krometrail_core::ResolvedRangeHandles,
) -> Result<(rmcp::model::JsonObject, Option<ResolvedRangeHandleId>)> {
    let has_range = arguments.contains_key("range");
    let has_handle = arguments.contains_key("range_handle");
    match (has_range, has_handle) {
        (true, false) => Ok((arguments, None)),
        (false, true) => {
            let handle_value = arguments
                .remove("range_handle")
                .expect("range handle presence was checked");
            let handle_argument = parse_arguments::<ResolvedRangeHandleArgument>(
                [("range_handle".into(), handle_value)]
                    .into_iter()
                    .collect(),
            )?;
            let range = handles
                .resolve_available(handle_argument.range_handle)
                .await?;
            arguments.insert("range".into(), serializable(range)?);
            Ok((arguments, Some(handle_argument.range_handle)))
        }
        (true, true) => Err(invalid_arguments("range_handle", None)),
        (false, false) => Err(invalid_arguments("range", None)),
    }
}

async fn range_handle_for_request(
    handles: &dyn krometrail_core::ResolvedRangeHandles,
    supplied_handle: Option<ResolvedRangeHandleId>,
    range: &ResolvedRange,
) -> Result<ResolvedRangeHandleId> {
    match supplied_handle {
        Some(handle) => Ok(handle),
        None => handles.register(range.clone()).await,
    }
}

fn progressive_request_range(request: &ProgressiveEvidenceRequest) -> Option<&ResolvedRange> {
    match request {
        ProgressiveEvidenceRequest::ListSourceFrames(request)
        | ProgressiveEvidenceRequest::FetchSourceFrames(request) => Some(&request.range),
        ProgressiveEvidenceRequest::GenerateArtifacts(request) => Some(request.request().range()),
        ProgressiveEvidenceRequest::GenerateRegionFilmstrip(request) => Some(&request.range),
        ProgressiveEvidenceRequest::PinResolvedRange(request)
        | ProgressiveEvidenceRequest::UnpinResolvedRange(request)
        | ProgressiveEvidenceRequest::QueryPinState(request) => Some(&request.range),
        ProgressiveEvidenceRequest::RetrieveArtifact(_)
        | ProgressiveEvidenceRequest::RetrieveSourceFrame(_) => None,
    }
}

fn temporal_annotations(mutability: OperationMutability, destructive: bool) -> ToolAnnotations {
    ToolAnnotations::new()
        .read_only(mutability == OperationMutability::ReadOnly)
        .destructive(destructive)
        .idempotent(true)
        .open_world(true)
}

fn request_cancelled_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("MCP request was cancelled")
            .expect("static cancellation error is non-empty"),
    )
    .with_retry(RetryAdvice::Safe)
}

fn request_deadline_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("MCP request deadline elapsed")
            .expect("static deadline error is non-empty"),
    )
    .with_retry(RetryAdvice::Safe)
}

fn lifecycle_route(tool: LifecycleTool) -> Result<ToolRoute<KrometrailMcpServer>> {
    let schema = match tool.kind {
        LifecycleKind::Start => type_input_schema::<LaunchBrowser>()?,
        LifecycleKind::Attach => type_input_schema::<AttachBrowser>()?,
        LifecycleKind::Status => {
            projected_input_schema(type_input_schema::<rmcp::model::EmptyObject>()?)?
        }
        LifecycleKind::Stop | LifecycleKind::Profiles => type_input_schema::<EmptyObject>()?,
    };
    let annotations = match tool.kind {
        LifecycleKind::Status | LifecycleKind::Profiles => ToolAnnotations::new()
            .read_only(true)
            .destructive(false)
            .idempotent(true)
            .open_world(true),
        LifecycleKind::Stop => ToolAnnotations::new()
            .read_only(false)
            .destructive(true)
            .idempotent(true)
            .open_world(true),
        LifecycleKind::Start | LifecycleKind::Attach => ToolAnnotations::new()
            .read_only(false)
            .destructive(false)
            .idempotent(false)
            .open_world(true),
    };
    let mut route = Tool::new(tool.name, tool.description, schema).annotate(annotations);
    route.output_schema = Some(tool_response_schema(false)?);
    Ok(ToolRoute::new_dyn(route, move |context| {
        async move { call_lifecycle(context, tool).await }.boxed()
    }))
}

async fn call_operation(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    kind: BrowserOperationKind,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let (arguments, preference) =
        match split_response_request(context.arguments.unwrap_or_default()) {
            Ok((arguments, preference)) => (
                arguments,
                preference.with_inline_default(browser_inline_image_default(kind)),
            ),
            Err(error) => return Ok(visible_error(name, error)),
        };
    let request = match tagged_request(name, Some(arguments)) {
        Ok(request) if request.kind() == kind => request,
        Ok(_) => {
            return Err(rmcp::ErrorData::internal_error(
                "tool registry request association failed",
                None,
            ));
        }
        Err(error) => return Ok(visible_error(name, error)),
    };
    let cancellation = Arc::new(McpCancellation(context.request_context.ct.clone()));
    match context
        .service
        .sessions()
        .execute(
            request,
            BrowserOperationContext::with_cancellation(cancellation),
        )
        .await
    {
        Ok(executed) => {
            let novelty = context
                .service
                .sessions()
                .observe_post_action(&executed.result)
                .await;
            map_operation_result_with_novelty(name, executed.result, preference, novelty)
                .map_err(|_| {
                    rmcp::ErrorData::internal_error("browser tool response mapping failed", None)
                })
                .and_then(|mapped| into_call_tool_result(mapped, &executed.capture_statuses))
        }
        Err(error) => {
            let capture_statuses = context
                .service
                .sessions()
                .capture_statuses()
                .await
                .unwrap_or_default();
            Ok(visible_error_with_capture(name, error, &capture_statuses))
        }
    }
}

async fn call_lifecycle(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    tool: LifecycleTool,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let arguments = context.arguments.unwrap_or_default();
    let sessions = context.service.sessions();
    // Capture health is read before the transition for `stop`, because the session that owned the
    // failing writer is gone by the time the outcome is mapped, and after the transition for every
    // other lifecycle tool, so a freshly started session cannot report a green light on a writer
    // that is already terminal.
    let pre_transition_health = match tool.kind {
        LifecycleKind::Stop => capture_health(sessions).await,
        _ => Vec::new(),
    };
    let result = match tool.kind {
        LifecycleKind::Start => match parse_arguments::<LaunchBrowser>(arguments) {
            Ok(request) => context
                .service
                .sessions()
                .start(request)
                .await
                .and_then(serializable),
            Err(error) => Err(error),
        },
        LifecycleKind::Attach => match parse_arguments::<AttachBrowser>(arguments) {
            Ok(request) => context
                .service
                .sessions()
                .attach(request)
                .await
                .and_then(serializable),
            Err(error) => Err(error),
        },
        LifecycleKind::Status => {
            let response = match split_response_request(arguments) {
                Ok((arguments, response)) if arguments.is_empty() => {
                    response.with_inline_default(false)
                }
                Ok(_) => {
                    return Ok(visible_error(
                        tool.name,
                        invalid_arguments("browser_status", None),
                    ));
                }
                Err(error) => return Ok(visible_error(tool.name, error)),
            };
            return match sessions.status().await {
                Ok(status) => {
                    let health = capture_health(sessions).await;
                    map_browser_status(tool.name, status, response)
                        .map_err(|_| {
                            rmcp::ErrorData::internal_error(
                                "browser status response mapping failed",
                                None,
                            )
                        })
                        .and_then(|mapped| into_call_tool_result(mapped, &health))
                }
                Err(error) => Ok(visible_error(tool.name, error)),
            };
        }
        LifecycleKind::Stop => context
            .service
            .sessions()
            .stop()
            .await
            .and_then(serializable),
        LifecycleKind::Profiles => context
            .service
            .sessions()
            .managed_profiles()
            .await
            .and_then(serializable),
    };
    let health = match tool.kind {
        LifecycleKind::Stop => pre_transition_health,
        _ => capture_health(sessions).await,
    };
    match result {
        Ok(value) => map_lifecycle_result(tool.name, value)
            .map_err(|_| {
                rmcp::ErrorData::internal_error("lifecycle tool response mapping failed", None)
            })
            .and_then(|mapped| into_call_tool_result(mapped, &health)),
        Err(error) => Ok(visible_error_with_capture(tool.name, error, &health)),
    }
}

/// Reads current capture health for the shared response exit. An absent session is not a capture
/// failure, so it contributes no warning.
async fn capture_health(
    sessions: &BrowserSessionOwner,
) -> Vec<krometrail_core::TargetCaptureStatus> {
    sessions.capture_statuses().await.unwrap_or_default()
}

fn tagged_request(
    operation: &'static str,
    arguments: Option<rmcp::model::JsonObject>,
) -> Result<BrowserOperationRequest> {
    decode_value(json!({
        "operation": operation,
        "request": Value::Object(arguments.unwrap_or_default()),
    }))
}

fn parse_arguments<T: DeserializeOwned>(arguments: rmcp::model::JsonObject) -> Result<T> {
    decode_value(Value::Object(arguments))
}

fn decode_value<T: DeserializeOwned>(value: Value) -> Result<T> {
    let encoded = serde_json::to_vec(&value).map_err(|_| invalid_arguments("$", None))?;
    let mut deserializer = serde_json::Deserializer::from_slice(&encoded);
    serde_path_to_error::deserialize(&mut deserializer).map_err(|error| {
        let path = normalize_argument_path(&error.path().to_string());
        let description = bounded_serde_description(&error);
        invalid_arguments(&path, Some(&description))
    })
}

fn serializable<T: serde::Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| {
        KrometrailError::new(
            ErrorCode::Internal,
            NonEmptyText::new("tool response could not be serialized").unwrap(),
        )
    })
}

fn invalid_arguments(path: &str, description: Option<&str>) -> KrometrailError {
    let path = path
        .strip_prefix("request.")
        .or_else(|| path.strip_prefix("request"))
        .filter(|path| !path.is_empty())
        .unwrap_or(path);
    let message = match description {
        Some(description) => {
            format!(
                "tool arguments do not match the advertised input schema at {path}: {description}"
            )
        }
        None => format!("tool arguments do not match the advertised input schema at {path}"),
    };
    KrometrailError::new(ErrorCode::InvalidInput, NonEmptyText::new(message).unwrap())
}

fn bounded_serde_description(error: impl std::fmt::Display) -> String {
    const MAX_SCHEMA_ERROR_BYTES: usize = 512;
    let description = error.to_string();
    let mut safe = String::with_capacity(description.len());
    let mut characters = description.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '"' {
            safe.push(character);
            continue;
        }
        safe.push('"');
        safe.push_str("[redacted]");
        let mut escaped = false;
        for character in characters.by_ref() {
            if escaped {
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == '"' {
                break;
            }
        }
        safe.push('"');
    }
    let mut description = safe;
    if description.len() > MAX_SCHEMA_ERROR_BYTES {
        let mut end = MAX_SCHEMA_ERROR_BYTES;
        while !description.is_char_boundary(end) {
            end -= 1;
        }
        description.truncate(end);
    }
    description
}

fn normalize_argument_path(path: &str) -> String {
    let normalized: String = path
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '[' | ']' | '$')
        })
        .take(256)
        .collect();
    if normalized.is_empty() || normalized == "." {
        "$".into()
    } else {
        normalized
    }
}

fn operation_annotations(mutability: OperationMutability) -> ToolAnnotations {
    let read_only = mutability == OperationMutability::ReadOnly;
    ToolAnnotations::new()
        .read_only(read_only)
        .destructive(!read_only)
        .idempotent(read_only)
        .open_world(true)
}

#[cfg(test)]
pub(crate) fn lifecycle_tool_names() -> impl Iterator<Item = &'static str> {
    LIFECYCLE_TOOLS.iter().map(|tool| tool.name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::McpConfig;
    use crate::response::ResponseRequest;
    use krometrail_core::{BrowserOperationRequest, ProgressiveEvidenceRequest};
    use serde_json::{Map, Value, json};

    #[test]
    fn route_registry_and_schema_validation_fail_closed() {
        validate_route_registry(&McpConfig::default()).unwrap();
        let mut names = BTreeSet::new();
        register_route_name(&mut names, "duplicate", "first").unwrap();
        assert!(register_route_name(&mut names, "duplicate", "second").is_err());
        assert!(generated_input_schema(schemars::schema_for!(String)).is_err());
    }

    #[test]
    fn registered_tool_schemas_accept_every_advertised_enum_and_one_of_branch() {
        #[derive(Clone)]
        struct Case {
            value: Value,
            pointer: String,
            advertised: String,
            enum_visits: usize,
            one_of_branch_visits: usize,
        }

        fn batch_operation_kinds(schema: &Value) -> Vec<BrowserOperationKind> {
            schema
                .get("properties")
                .and_then(Value::as_object)
                .and_then(|properties| properties.get("operation"))
                .and_then(|operation| operation.get("enum"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|name| {
                    BrowserOperationKind::from_stable_name(name)
                        .expect("batch schema operation is registered")
                })
                .collect()
        }

        fn is_flat_batch_step_schema(schema: &Value) -> bool {
            let Some(properties) = schema.get("properties").and_then(Value::as_object) else {
                return false;
            };
            schema.get("type") == Some(&Value::String("object".into()))
                && properties
                    .get("request")
                    .and_then(|request| request.get("description"))
                    == Some(&Value::String(
                        "The same arguments advertised by the named standalone operation.".into(),
                    ))
                && properties.get("operation").is_some()
                && schema
                    .get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|required| {
                        required.iter().any(|value| value == "operation")
                            && required.iter().any(|value| value == "request")
                    })
        }

        fn schema_cases(schema: &Value, pointer: &str) -> Vec<Case> {
            if is_flat_batch_step_schema(schema) {
                let mut cases = Vec::new();
                for kind in batch_operation_kinds(schema) {
                    let request_schema = Value::Object(
                        operation_input_schema(kind, &McpConfig::default())
                            .expect("registered batch operation has a schema")
                            .as_ref()
                            .clone(),
                    );
                    for request_case in schema_cases(&request_schema, &format!("{pointer}/request"))
                    {
                        let mut step = Map::new();
                        step.insert("operation".into(), Value::String(kind.stable_name().into()));
                        step.insert("request".into(), request_case.value);
                        cases.push(Case {
                            value: Value::Object(step),
                            pointer: request_case.pointer,
                            advertised: if request_case.advertised == "minimal required instance" {
                                format!("batch operation \"{}\"", kind.stable_name())
                            } else {
                                format!(
                                    "batch operation \"{}\" {}",
                                    kind.stable_name(),
                                    request_case.advertised
                                )
                            },
                            enum_visits: request_case.enum_visits,
                            one_of_branch_visits: request_case.one_of_branch_visits,
                        });
                    }
                }
                return cases;
            }
            if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
                let mut cases = Vec::new();
                for (index, branch) in branches.iter().enumerate() {
                    let branch_cases = schema_cases(branch, pointer);
                    if let Some(first) = branch_cases.first() {
                        cases.push(Case {
                            value: first.value.clone(),
                            pointer: pointer.to_owned(),
                            advertised: format!("oneOf branch {index}"),
                            enum_visits: first.enum_visits,
                            one_of_branch_visits: first.one_of_branch_visits + 1,
                        });
                    }
                    cases.extend(branch_cases);
                }
                return cases;
            }
            if let Some(branches) = schema.get("allOf").and_then(Value::as_array) {
                let mut merged = Map::new();
                let mut required = Vec::new();
                for branch in branches {
                    if let Some(properties) = branch.get("properties").and_then(Value::as_object) {
                        merged
                            .entry("properties")
                            .or_insert_with(|| Value::Object(Map::new()))
                            .as_object_mut()
                            .unwrap()
                            .extend(properties.clone());
                    }
                    if let Some(values) = branch.get("required").and_then(Value::as_array) {
                        required.extend(values.iter().cloned());
                    }
                }
                if !required.is_empty() {
                    merged.insert("required".into(), Value::Array(required));
                }
                return schema_cases(&Value::Object(merged), pointer);
            }
            if let Some(values) = schema.get("enum").and_then(Value::as_array) {
                return values
                    .iter()
                    .map(|value| Case {
                        value: value.clone(),
                        pointer: pointer.to_owned(),
                        advertised: value.to_string(),
                        enum_visits: 1,
                        one_of_branch_visits: 0,
                    })
                    .collect();
            }
            if let Some(value) = schema.get("const") {
                return vec![Case {
                    value: value.clone(),
                    pointer: pointer.to_owned(),
                    advertised: value.to_string(),
                    enum_visits: 0,
                    one_of_branch_visits: 0,
                }];
            }
            if schema.get("type") == Some(&Value::String("object".into()))
                || schema.get("properties").is_some()
            {
                let properties = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let required = schema
                    .get("required")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut base = Map::new();
                let mut base_enum_visits = 0;
                let mut base_one_of_branch_visits = 0;
                let required_names = required
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>();
                for name in required_names.iter().copied() {
                    let child_pointer = format!("{pointer}/{name}");
                    let child_cases = schema_cases(
                        properties
                            .get(name)
                            .expect("required property has a schema"),
                        &child_pointer,
                    );
                    let first = child_cases
                        .first()
                        .expect("required property has a minimal instance");
                    base.insert(name.to_owned(), first.value.clone());
                    base_enum_visits += first.enum_visits;
                    base_one_of_branch_visits += first.one_of_branch_visits;
                }
                let mut cases = vec![Case {
                    value: Value::Object(base.clone()),
                    pointer: pointer.to_owned(),
                    advertised: "minimal required instance".into(),
                    enum_visits: base_enum_visits,
                    one_of_branch_visits: base_one_of_branch_visits,
                }];
                for (name, property_schema) in properties {
                    // JSON Schema cannot express same-request containment such as a marker's
                    // time or a frame selector's id belonging to the sibling resolved range.
                    // Do not invent state-dependent values here; this sweep stays at the
                    // schema-expressible boundary and leaves those invariants to domain tests.
                    if !required_names.contains(name.as_str())
                        && state_dependent_property(&base, &name)
                        && name != "reference"
                    {
                        continue;
                    }
                    let child_pointer = format!("{pointer}/{name}");
                    let child_cases = schema_cases(&property_schema, &child_pointer);
                    let children = if required_names.contains(name.as_str()) {
                        child_cases.into_iter().skip(1).collect::<Vec<_>>()
                    } else {
                        child_cases
                    };
                    for child in children {
                        if state_dependent_case(&base, &name, &child.value) {
                            continue;
                        }
                        let mut value = base.clone();
                        value.insert(name.clone(), child.value);
                        cases.push(Case {
                            value: Value::Object(value),
                            pointer: child.pointer,
                            advertised: child.advertised,
                            enum_visits: child.enum_visits,
                            one_of_branch_visits: child.one_of_branch_visits,
                        });
                    }
                }
                return cases;
            }
            if schema.get("type") == Some(&Value::String("array".into())) {
                let item_schema = schema.get("items").unwrap_or(&Value::Null);
                let item_cases = schema_cases(item_schema, &format!("{pointer}/0"));
                return item_cases
                    .into_iter()
                    .map(|item| Case {
                        value: Value::Array(vec![item.value]),
                        pointer: item.pointer,
                        advertised: item.advertised,
                        enum_visits: item.enum_visits,
                        one_of_branch_visits: item.one_of_branch_visits,
                    })
                    .collect();
            }
            let value = match schema.get("type").and_then(Value::as_str) {
                Some("boolean") => Value::Bool(false),
                Some("integer") | Some("number") => schema
                    .get("minimum")
                    .and_then(|minimum| {
                        if minimum.as_f64().is_some_and(|value| value > 0.0) {
                            Some(minimum.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| json!(1)),
                Some("string") => match schema.get("format").and_then(Value::as_str) {
                    Some("uuid") => json!("00000000-0000-0000-0000-000000000001"),
                    Some("uri") | Some("uri-reference") => json!("https://example.com"),
                    _ if schema.get("pattern").is_some() => json!("/tmp/krometrail-upload"),
                    _ => json!("x"),
                },
                _ => Value::Null,
            };
            vec![Case {
                value,
                pointer: pointer.to_owned(),
                advertised: "minimal scalar instance".into(),
                enum_visits: 0,
                one_of_branch_visits: 0,
            }]
        }

        fn state_dependent_property(base: &Map<String, Value>, name: &str) -> bool {
            let has_range = base.contains_key("range") || base.contains_key("range_handle");
            match name {
                "markers" | "focus_times" => has_range,
                "anchor" => {
                    has_range && (base.contains_key("generators") || base.contains_key("region"))
                }
                "reference" => {
                    base.get("generator")
                        .and_then(Value::as_str)
                        .is_some_and(|generator| {
                            matches!(generator, "difference_map" | "motion_history")
                        })
                }
                _ => false,
            }
        }

        fn state_dependent_case(base: &Map<String, Value>, name: &str, value: &Value) -> bool {
            name == "reference"
                && base
                    .get("generator")
                    .and_then(Value::as_str)
                    .is_some_and(|generator| {
                        matches!(generator, "difference_map" | "motion_history")
                    })
                && value.get("frame").and_then(Value::as_str) == Some("frame")
        }

        fn resolved_range() -> Value {
            json!({
                "session_id": "00000000-0000-0000-0000-000000000001",
                "target_id": "00000000-0000-0000-0000-000000000002",
                "anchor_kind": "session_time",
                "resolved_anchor": {
                    "reference": {"source": "interval"},
                    "requested_time": 0,
                    "effective_time": 0
                },
                "requested_range": {"start": 0, "end": 0},
                "resolved_range": {"start": 0, "end": 0},
                "frame_ids": ["00000000-0000-0000-0000-000000000003"],
                "interaction_ids": [],
                "navigation_ids": [],
                "marker_ids": [],
                "gaps": [],
                "retention_warnings": [],
                "options": {
                    "retention": "require_complete",
                    "capture_gaps": "include",
                    "implicit_interaction_window": {"before_ms": 150, "after_ms": 250}
                }
            })
        }

        fn repair_range(value: &mut Value) -> std::result::Result<(), String> {
            let Some(range) = value.get_mut("range").and_then(Value::as_object_mut) else {
                return Ok(());
            };
            let anchor_kind = range
                .get("anchor_kind")
                .and_then(Value::as_str)
                .unwrap_or("session_time");
            let reference_source = range
                .get("resolved_anchor")
                .and_then(Value::as_object)
                .and_then(|anchor| anchor.get("reference"))
                .and_then(Value::as_object)
                .and_then(|reference| reference.get("source"))
                .and_then(Value::as_str)
                .unwrap_or("interval");
            if !matches!(
                reference_source,
                "interval" | "interaction" | "navigation" | "marker" | "source_frames"
            ) {
                return Err(format!(
                    "unknown resolved anchor reference source {reference_source:?}"
                ));
            }
            let (anchor_kind, reference) = match anchor_kind {
                "interaction" => (
                    "interaction",
                    json!({
                        "source": "interaction",
                        "interaction_id": "00000000-0000-0000-0000-000000000004"
                    }),
                ),
                "navigation" => (
                    "navigation",
                    json!({
                        "source": "navigation",
                        "navigation_id": "00000000-0000-0000-0000-000000000005"
                    }),
                ),
                "marker" => (
                    "marker",
                    json!({
                        "source": "marker",
                        "marker_id": "00000000-0000-0000-0000-000000000006"
                    }),
                ),
                "source_frame" => (
                    "source_frame",
                    json!({
                        "source": "source_frames",
                        "start_frame_id": "00000000-0000-0000-0000-000000000003",
                        "end_frame_id": "00000000-0000-0000-0000-000000000003"
                    }),
                ),
                "wall_clock" => ("wall_clock", json!({"source": "interval"})),
                "session_time" => ("session_time", json!({"source": "interval"})),
                unknown => {
                    return Err(format!(
                        "unknown resolved anchor kind {unknown:?} advertised by schema"
                    ));
                }
            };
            let mut valid = resolved_range();
            let valid = valid.as_object_mut().unwrap();
            valid.insert("anchor_kind".into(), json!(anchor_kind));
            if anchor_kind == "interaction" {
                // Interaction-kind resolved ranges echo the governing window.
                valid.insert(
                    "applied_interaction_window".into(),
                    json!({"before_ms": 150, "after_ms": 250}),
                );
            }
            valid["resolved_anchor"]["reference"] = reference;
            *range = valid.clone();
            Ok(())
        }

        fn decode(tool: &str, mut value: Value) -> std::result::Result<(), String> {
            repair_range(&mut value)?;
            let mut response_result = Ok(());
            if let Some(object) = value.as_object_mut() {
                if let Some(response) = object.remove("response") {
                    response_result = serde_json::from_value::<ResponseRequest>(response)
                        .map(|_| ())
                        .map_err(|error| format!("response subtree: {error}"));
                }
                if let Some(range_handle) = object.remove("range_handle") {
                    serde_json::from_value::<crate::schema::ResolvedRangeHandleArgument>(
                        json!({"range_handle": range_handle}),
                    )
                    .map_err(|error| format!("range_handle subtree: {error}"))?;
                    object.insert("range".into(), resolved_range());
                }
                if let Some(region) = object.get_mut("region").and_then(Value::as_object_mut) {
                    region.insert(
                        "source_frame_id".into(),
                        json!("00000000-0000-0000-0000-000000000003"),
                    );
                    if let Some(shape) = region.get_mut("shape").and_then(Value::as_object_mut) {
                        if let Some(mask) = shape.get_mut("mask").and_then(Value::as_object_mut) {
                            mask.insert("bits".into(), json!([128]));
                            mask.insert("dimensions".into(), json!({"width": 1, "height": 1}));
                        }
                    }
                    if let Some(session_id) = region.get_mut("session_id") {
                        *session_id = json!("00000000-0000-0000-0000-000000000001");
                    }
                    if let Some(reference) =
                        region.get_mut("reference").and_then(Value::as_object_mut)
                    {
                        reference.insert(
                            "target_id".into(),
                            json!("00000000-0000-0000-0000-000000000002"),
                        );
                    }
                }
                if let Some(selection) = object.get_mut("selection").and_then(Value::as_object_mut)
                {
                    if let Some(frame_id) = selection.get_mut("frame_id") {
                        *frame_id = json!("00000000-0000-0000-0000-000000000003");
                    }
                    if let Some(frame_ids) =
                        selection.get_mut("frame_ids").and_then(Value::as_array_mut)
                    {
                        for frame_id in frame_ids {
                            *frame_id = json!("00000000-0000-0000-0000-000000000003");
                        }
                    }
                }
                if let Some(frame_id) = object.get_mut("frame_id") {
                    *frame_id = json!("00000000-0000-0000-0000-000000000003");
                }
            }
            let decoded = match tool {
                "start_browser" => {
                    serde_json::from_value::<krometrail_core::LaunchBrowser>(value).map(|_| ())
                }
                "attach_browser" => {
                    serde_json::from_value::<krometrail_core::AttachBrowser>(value).map(|_| ())
                }
                "browser_status" | "stop_browser" | "list_managed_profiles" => {
                    serde_json::from_value::<rmcp::model::EmptyObject>(value).map(|_| ())
                }
                name if BROWSER_OPERATION_REGISTRY
                    .iter()
                    .any(|definition| definition.stable_name == name) =>
                {
                    serde_json::from_value::<BrowserOperationRequest>(
                        json!({"operation": name, "request": value}),
                    )
                    .map(|_| ())
                }
                name if krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY
                    .iter()
                    .any(|definition| definition.stable_name == name) =>
                {
                    serde_json::from_value::<ProgressiveEvidenceRequest>(
                        json!({"operation": name, "request": value}),
                    )
                    .map(|_| ())
                }
                "temporal_debug_bundle" => {
                    serde_json::from_value::<krometrail_core::TemporalDebugBundleRequest>(value)
                        .map(|_| ())
                }
                "resolve_temporal_range" => {
                    serde_json::from_value::<krometrail_core::TemporalQueryRequest>(value)
                        .map(|_| ())
                }
                "generate_temporal_video" => {
                    serde_json::from_value::<krometrail_core::TemporalVideoGenerationRequest>(value)
                        .map(|_| ())
                }
                name if TEMPORAL_CONTEXT_OPERATION_REGISTRY
                    .iter()
                    .any(|definition| definition.stable_name == name) =>
                {
                    serde_json::from_value::<krometrail_core::BrowserEventDetailRequest>(value)
                        .map(|_| ())
                }
                _ => return Err("tool is not in the registered decoder set".into()),
            };
            response_result.and_then(|_| decoded.map_err(|error| error.to_string()))
        }

        fn replace_frequency_mode_enum(value: &mut Value) -> bool {
            match value {
                Value::Object(object) => {
                    if let Some(values) = object.get_mut("enum").and_then(Value::as_array_mut) {
                        if let Some(value) = values
                            .iter_mut()
                            .find(|value| value.as_str() == Some("count"))
                        {
                            *value = Value::String("Count".into());
                            return true;
                        }
                    }
                    object.values_mut().any(replace_frequency_mode_enum)
                }
                Value::Array(values) => values.iter_mut().any(replace_frequency_mode_enum),
                Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
            }
        }

        let config = McpConfig::default();
        let mut contracts = Vec::new();
        for lifecycle in LIFECYCLE_TOOLS {
            let schema = match lifecycle.kind {
                LifecycleKind::Start => type_input_schema::<krometrail_core::LaunchBrowser>(),
                LifecycleKind::Attach => type_input_schema::<krometrail_core::AttachBrowser>(),
                LifecycleKind::Status => {
                    projected_input_schema(type_input_schema::<rmcp::model::EmptyObject>().unwrap())
                }
                LifecycleKind::Stop | LifecycleKind::Profiles => {
                    type_input_schema::<rmcp::model::EmptyObject>()
                }
            }
            .unwrap();
            contracts.push((lifecycle.name, Value::Object(schema.as_ref().clone())));
        }
        for definition in BROWSER_OPERATION_REGISTRY {
            contracts.push((
                definition.stable_name,
                Value::Object(
                    projected_input_schema(
                        operation_input_schema(definition.kind, &config).unwrap(),
                    )
                    .unwrap()
                    .as_ref()
                    .clone(),
                ),
            ));
        }
        for definition in krometrail_core::PROGRESSIVE_EVIDENCE_REGISTRY {
            if definition.exposure == OperationExposure::Tool {
                contracts.push((
                    definition.stable_name,
                    Value::Object(
                        progressive_input_schema(definition.kind)
                            .unwrap()
                            .as_ref()
                            .clone(),
                    ),
                ));
            }
        }
        contracts.extend([
            (
                TEMPORAL_DEBUG_BUNDLE_OPERATION.stable_name,
                Value::Object(
                    projected_input_schema(
                        type_input_schema::<krometrail_core::TemporalDebugBundleRequest>().unwrap(),
                    )
                    .unwrap()
                    .as_ref()
                    .clone(),
                ),
            ),
            (
                TEMPORAL_RANGE_RESOLUTION_OPERATION.stable_name,
                Value::Object(
                    projected_input_schema(
                        type_input_schema::<krometrail_core::TemporalQueryRequest>().unwrap(),
                    )
                    .unwrap()
                    .as_ref()
                    .clone(),
                ),
            ),
            (
                TEMPORAL_VIDEO_OPERATION.stable_name,
                Value::Object(
                    projected_input_schema(
                        range_handle_input_schema(
                            type_input_schema::<krometrail_core::TemporalVideoGenerationRequest>()
                                .unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap()
                    .as_ref()
                    .clone(),
                ),
            ),
        ]);
        for definition in TEMPORAL_CONTEXT_OPERATION_REGISTRY {
            contracts.push((
                definition.stable_name,
                Value::Object(
                    projected_input_schema(
                        range_handle_input_schema(
                            generated_input_schema(definition.kind.input_schema()).unwrap(),
                        )
                        .unwrap(),
                    )
                    .unwrap()
                    .as_ref()
                    .clone(),
                ),
            ));
        }

        let mut negative_control_schema = contracts
            .iter()
            .find(|(tool, _)| *tool == "generate_artifacts")
            .expect("generate_artifacts is registered")
            .1
            .clone();
        let mut unknown_anchor = json!({"range": resolved_range()});
        unknown_anchor["range"]["anchor_kind"] = json!("latest_interaction");
        assert!(
            repair_range(&mut unknown_anchor)
                .expect_err("unknown resolved anchor kinds must fail the sweep")
                .contains("unknown resolved anchor kind")
        );
        assert!(replace_frequency_mode_enum(&mut negative_control_schema));
        let negative_control_failure = schema_cases(&negative_control_schema, "$")
            .into_iter()
            .filter_map(|case| {
                decode("generate_artifacts", case.value.clone())
                    .err()
                    .map(|error| (case, error))
            })
            .find(|(case, error)| {
                case.advertised.contains("\"Count\"") && error.contains("unknown variant `Count`")
            })
            .expect("reintroduced enum mismatch must fail the conformance sweep");
        eprintln!(
            "negative control observed: tool=generate_artifacts pointer={} advertised={} error={}",
            negative_control_failure.0.pointer,
            negative_control_failure.0.advertised,
            negative_control_failure.1
        );

        let mut enum_count = 0;
        let mut one_of_branch_count = 0;
        let mut failures = Vec::new();
        for (tool, schema) in contracts {
            for case in schema_cases(&schema, "$") {
                enum_count += case.enum_visits;
                one_of_branch_count += case.one_of_branch_visits;
                if let Err(error) = decode(tool, case.value.clone()) {
                    failures.push(format!(
                        "tool={tool} pointer={} value={} advertised={} error={error}",
                        case.pointer, case.value, case.advertised
                    ));
                }
            }
        }
        assert!(
            enum_count > 400,
            "schema sweep generated implausibly few string-enum cases: {enum_count}"
        );
        assert!(
            one_of_branch_count > 350,
            "schema sweep generated implausibly few oneOf branch cases: {one_of_branch_count}"
        );
        assert!(
            failures.is_empty(),
            "schema/domain conformance failures:\n{}",
            failures.join("\n")
        );
    }

    #[test]
    fn image_defaults_follow_operation_purpose_and_preserve_overrides() {
        for kind in ProgressiveEvidenceOperationKind::ALL {
            assert_eq!(
                progressive_inline_image_default(*kind),
                matches!(
                    kind,
                    ProgressiveEvidenceOperationKind::FetchSourceFrames
                        | ProgressiveEvidenceOperationKind::GenerateArtifacts
                        | ProgressiveEvidenceOperationKind::GenerateRegionFilmstrip
                )
            );
        }
        for kind in BrowserOperationKind::ALL {
            assert_eq!(
                browser_inline_image_default(*kind),
                matches!(
                    kind,
                    BrowserOperationKind::TakeScreenshot
                        | BrowserOperationKind::ObserveLive
                        | BrowserOperationKind::Scroll
                        | BrowserOperationKind::SetViewport
                        | BrowserOperationKind::ActivatePage
                )
            );
        }
        for kind in [
            BrowserOperationKind::Scroll,
            BrowserOperationKind::SetViewport,
            BrowserOperationKind::ActivatePage,
        ] {
            assert!(browser_inline_image_default(kind));
            assert_eq!(
                ResponseRequest {
                    inline_images: Some(false),
                    ..ResponseRequest::default()
                }
                .with_inline_default(browser_inline_image_default(kind))
                .inline_images,
                Some(false)
            );
        }
        assert_eq!(
            ResponseRequest::default()
                .with_inline_default(true)
                .inline_images,
            Some(true)
        );
        assert_eq!(
            ResponseRequest {
                inline_images: Some(false),
                ..ResponseRequest::default()
            }
            .with_inline_default(true)
            .inline_images,
            Some(false)
        );
        assert_eq!(
            resolve_progressive_response(
                ResponseRequest::default(),
                ProgressiveEvidenceOperationKind::FetchSourceFrames,
            )
            .inline_images,
            None
        );
    }

    #[tokio::test]
    async fn request_cancellation_and_deadline_are_isolated_per_request() {
        let first_token = CancellationToken::new();
        let second_token = CancellationToken::new();
        let first = RequestBudget::new(first_token.clone());
        let second = RequestBudget::new(second_token.clone());
        let task = tokio::spawn(async move {
            first
                .run(std::future::pending::<Result<()>>())
                .await
                .unwrap_err()
        });
        first_token.cancel();
        assert_eq!(task.await.unwrap().code, ErrorCode::Cancelled);
        assert!(!second_token.is_cancelled());
        second.check().unwrap();

        let mut expired = RequestBudget::new(second_token);
        expired.deadline = Instant::now();
        assert_eq!(
            expired
                .run(std::future::pending::<Result<()>>())
                .await
                .unwrap_err()
                .code,
            ErrorCode::Cancelled
        );
    }

    #[test]
    fn invalid_arguments_name_first_nested_path_without_echoing_values() {
        #[allow(dead_code)]
        #[derive(Debug, serde::Deserialize)]
        struct Arguments {
            locator: Locator,
        }
        #[allow(dead_code)]
        #[derive(Debug, serde::Deserialize)]
        struct Locator {
            reference: u64,
        }

        let arguments = serde_json::from_value(serde_json::json!({
            "locator": {"reference": "sensitive-browser-content"}
        }))
        .unwrap();
        let error = parse_arguments::<Arguments>(arguments).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidInput);
        assert!(error.message.as_str().contains("locator.reference"));
        assert!(error.message.as_str().contains("invalid type"));
        assert!(!error.message.as_str().contains("sensitive-browser-content"));

        let error = parse_arguments::<Arguments>(serde_json::Map::new()).unwrap_err();
        assert!(error.message.as_str().contains("missing field `locator`"));
    }
}
