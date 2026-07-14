use std::sync::Arc;

use futures_util::FutureExt as _;
use krometrail_core::{
    AttachBrowser, BROWSER_OPERATION_REGISTRY, BrowserOperationContext, BrowserOperationKind,
    BrowserOperationRequest, CancellationSignal, CapabilityId, ErrorCode, KrometrailError,
    LaunchBrowser, NonEmptyText, OperationMutability, PortFuture, Result,
};
use rmcp::{
    handler::server::tool::{ToolCallContext, ToolRoute, ToolRouter},
    model::{EmptyObject, Tool, ToolAnnotations},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use crate::{
    config::McpConfig,
    response::{provisional_success, visible_error},
    schema::{operation_input_schema, type_input_schema},
    server::KrometrailMcpServer,
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

#[derive(Clone)]
struct McpCancellation(CancellationToken);

impl CancellationSignal for McpCancellation {
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    fn cancelled(&self) -> PortFuture<'_, ()> {
        Box::pin(self.0.cancelled())
    }
}

pub(crate) fn build_router(config: &McpConfig) -> Result<ToolRouter<KrometrailMcpServer>> {
    let mut router = ToolRouter::new();
    if !config.is_enabled(CapabilityId::Control) {
        return Ok(router);
    }

    for lifecycle in LIFECYCLE_TOOLS {
        router.add_route(lifecycle_route(*lifecycle)?);
    }
    for definition in BROWSER_OPERATION_REGISTRY {
        let kind = definition.kind;
        let name = definition.stable_name;
        let input_schema = operation_input_schema(kind, config)?;
        let annotations = operation_annotations(definition.mutability);
        let tool = Tool::new(name, definition.description, input_schema).annotate(annotations);
        router.add_route(ToolRoute::new_dyn(tool, move |context| {
            async move { call_operation(context, kind, name).await }.boxed()
        }));
    }
    Ok(router)
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
    let route = Tool::new(tool.name, tool.description, schema).annotate(annotations);
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
        Ok(result) => Ok(provisional_success(
            name,
            json!({"operation": result.kind().stable_name()}),
        )),
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
    Ok(match result {
        Ok(value) => provisional_success(tool.name, value),
        Err(error) => visible_error(tool.name, error),
    })
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
