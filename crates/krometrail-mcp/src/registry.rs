use std::{
    collections::BTreeSet,
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use futures_util::FutureExt as _;
use krometrail_core::{
    AttachBrowser, BROWSER_OPERATION_REGISTRY, BrowserEventDetailRequest, BrowserOperationContext,
    BrowserOperationKind, BrowserOperationRequest, CancellationSignal, CapabilityId,
    CurrentReferenceGeometry, ErrorCode, KrometrailError, LaunchBrowser, NonEmptyText,
    OperationExposure, OperationMutability, PortFuture, ProgressiveEvidenceContext,
    ProgressiveEvidenceOperationKind, ProgressiveEvidenceRequest, Result, RetryAdvice,
    TEMPORAL_CONTEXT_OPERATION_REGISTRY, TEMPORAL_DEBUG_BUNDLE_OPERATION,
    TemporalContextOperationKind, TemporalDebugBundleContext, TemporalDebugBundleRequest,
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
        ToolResponse, into_call_tool_result, map_lifecycle_result, map_operation_result,
        map_progressive_result, map_temporal_bundle_result, visible_error,
    },
    schema::{generated_input_schema, operation_input_schema, type_input_schema},
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
            let input_schema = operation_input_schema(kind, config)?;
            let annotations = operation_annotations(definition.mutability);
            let mut tool =
                Tool::new(name, definition.description, input_schema).annotate(annotations);
            tool.output_schema = Some(type_input_schema::<ToolResponse>()?);
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
            type_input_schema::<TemporalDebugBundleRequest>()?,
        )
        .annotate(temporal_annotations(definition.mutability, false));
        tool.output_schema = Some(type_input_schema::<ToolResponse>()?);
        let dependencies = Arc::clone(&dependencies);
        router.add_route(ToolRoute::new_dyn(tool, move |context| {
            let dependencies = Arc::clone(&dependencies);
            async move { call_bundle(context, dependencies, name).await }.boxed()
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
        let input_schema = generated_input_schema(kind.input_schema())?;
        let annotations = temporal_annotations(
            definition.mutability,
            kind == ProgressiveEvidenceOperationKind::UnpinResolvedRange,
        );
        let mut tool = Tool::new(name, definition.description, input_schema).annotate(annotations);
        tool.output_schema = Some(type_input_schema::<ToolResponse>()?);
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
            let input_schema = generated_input_schema(kind.input_schema())?;
            let mut tool = Tool::new(name, definition.description, input_schema)
                .annotate(temporal_annotations(definition.mutability, false));
            tool.output_schema = Some(type_input_schema::<ToolResponse>()?);
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
        let _ = operation_input_schema(definition.kind, config)?;
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
        let _ = generated_input_schema(definition.kind.input_schema())?;
    }
    register_route_name(
        &mut names,
        TEMPORAL_DEBUG_BUNDLE_OPERATION.stable_name,
        TEMPORAL_DEBUG_BUNDLE_OPERATION.description,
    )?;
    let _ = type_input_schema::<TemporalDebugBundleRequest>()?;
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
        let _ = generated_input_schema(definition.kind.input_schema())?;
    }
    Ok(())
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
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let request = match parse_arguments::<TemporalDebugBundleRequest>(
        context.arguments.unwrap_or_default(),
    ) {
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
        Ok(bundle) => map_temporal_bundle_result(
            name,
            bundle,
            dependencies.progressive_evidence.as_ref(),
            budget.deadline,
            budget.cancellation.clone(),
        )
        .await
        .map_err(|_| {
            rmcp::ErrorData::internal_error("temporal bundle response mapping failed", None)
        })
        .and_then(into_call_tool_result),
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
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let request = match progressive_request(name, context.arguments) {
        Ok(request) if request.kind() == kind => request,
        Ok(_) => {
            return Err(rmcp::ErrorData::internal_error(
                "tool registry request association failed",
                None,
            ));
        }
        Err(error) => return Ok(call_error_result(name, error)),
    };
    let cancellation: Arc<dyn CancellationSignal> = budget.cancellation.clone();
    let result = budget
        .run(dependencies.progressive_evidence.execute(
            request,
            ProgressiveEvidenceContext {
                deadline: Some(budget.deadline),
                cancellation: Some(cancellation),
                current_reference_geometry: Some(current_geometry),
            },
        ))
        .await;
    match result {
        Ok(result) => map_progressive_result(
            name,
            result,
            dependencies.progressive_evidence.as_ref(),
            budget.deadline,
            budget.cancellation.clone(),
        )
        .await
        .map_err(|_| rmcp::ErrorData::internal_error("progressive response mapping failed", None))
        .and_then(into_call_tool_result),
        Err(error) => Ok(call_error_result(name, error)),
    }
}

async fn call_context(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    dependencies: Arc<McpDependencies>,
    kind: TemporalContextOperationKind,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let budget = RequestBudget::new(context.request_context.ct.clone());
    if let Err(error) = budget.check() {
        return Ok(call_error_result(name, error));
    }
    let request = match kind {
        TemporalContextOperationKind::QueryBrowserEvents => {
            match parse_arguments::<BrowserEventDetailRequest>(
                context.arguments.unwrap_or_default(),
            ) {
                Ok(request) => request.into_context_request(),
                Err(error) => return Ok(call_error_result(name, error)),
            }
        }
    };
    let result = budget
        .run(dependencies.temporal_context.context(request))
        .await;
    match result {
        Ok(value) => map_lifecycle_result(name, value)
            .map_err(|_| {
                rmcp::ErrorData::internal_error("browser event response mapping failed", None)
            })
            .and_then(into_call_tool_result),
        Err(error) => Ok(call_error_result(name, error)),
    }
}

fn progressive_request(
    operation: &'static str,
    arguments: Option<rmcp::model::JsonObject>,
) -> Result<ProgressiveEvidenceRequest> {
    serde_json::from_value(json!({
        "operation": operation,
        "request": Value::Object(arguments.unwrap_or_default()),
    }))
    .map_err(|_| invalid_arguments())
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
        LifecycleKind::Status | LifecycleKind::Stop => type_input_schema::<EmptyObject>()?,
    };
    let annotations = match tool.kind {
        LifecycleKind::Status => ToolAnnotations::new()
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
    route.output_schema = Some(type_input_schema::<ToolResponse>()?);
    Ok(ToolRoute::new_dyn(route, move |context| {
        async move { call_lifecycle(context, tool).await }.boxed()
    }))
}

async fn call_operation(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    kind: BrowserOperationKind,
    name: &'static str,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let request = match tagged_request(name, context.arguments) {
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
        Ok(result) => map_operation_result(name, result)
            .map_err(|_| {
                rmcp::ErrorData::internal_error("browser tool response mapping failed", None)
            })
            .and_then(into_call_tool_result),
        Err(error) => Ok(visible_error(name, error)),
    }
}

async fn call_lifecycle(
    context: ToolCallContext<'_, KrometrailMcpServer>,
    tool: LifecycleTool,
) -> std::result::Result<rmcp::model::CallToolResult, rmcp::ErrorData> {
    let arguments = context.arguments.unwrap_or_default();
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
        LifecycleKind::Status => context
            .service
            .sessions()
            .status()
            .await
            .and_then(serializable),
        LifecycleKind::Stop => context
            .service
            .sessions()
            .stop()
            .await
            .and_then(serializable),
    };
    match result {
        Ok(value) => map_lifecycle_result(tool.name, value)
            .map_err(|_| {
                rmcp::ErrorData::internal_error("lifecycle tool response mapping failed", None)
            })
            .and_then(into_call_tool_result),
        Err(error) => Ok(visible_error(tool.name, error)),
    }
}

fn tagged_request(
    operation: &'static str,
    arguments: Option<rmcp::model::JsonObject>,
) -> Result<BrowserOperationRequest> {
    serde_json::from_value(json!({
        "operation": operation,
        "request": Value::Object(arguments.unwrap_or_default()),
    }))
    .map_err(|_| invalid_arguments())
}

fn parse_arguments<T: DeserializeOwned>(arguments: rmcp::model::JsonObject) -> Result<T> {
    serde_json::from_value(Value::Object(arguments)).map_err(|_| invalid_arguments())
}

fn serializable<T: serde::Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|_| {
        KrometrailError::new(
            ErrorCode::Internal,
            NonEmptyText::new("tool response could not be serialized").unwrap(),
        )
    })
}

fn invalid_arguments() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::InvalidInput,
        NonEmptyText::new("tool arguments do not match the advertised input schema").unwrap(),
    )
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

    #[test]
    fn route_registry_and_schema_validation_fail_closed() {
        validate_route_registry(&McpConfig::default()).unwrap();
        let mut names = BTreeSet::new();
        register_route_name(&mut names, "duplicate", "first").unwrap();
        assert!(register_route_name(&mut names, "duplicate", "second").is_err());
        assert!(generated_input_schema(schemars::schema_for!(String)).is_err());
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
}
