use std::path::PathBuf;

use krometrail_core::{ErrorCode, NonEmptyText, Result, RetryAdvice, UploadFilesRequest};
use serde_json::json;

use super::{
    BoundTarget,
    interaction::{ResolvedTarget, send_cdp},
    navigation::OperationCancellation,
    operation_error,
};
use crate::transport::CdpTransport;

pub(super) async fn upload_files(
    transport: &dyn CdpTransport,
    bound: &BoundTarget,
    request: &UploadFilesRequest,
    target: &ResolvedTarget,
    cancel: &OperationCancellation,
    generation: u64,
) -> Result<()> {
    let node = target.node(bound.target_id)?;
    let requested = request
        .files
        .iter()
        .map(|path| (PathBuf::from(path.as_str()), path.basename().to_owned()))
        .collect::<Vec<_>>();
    let canonical = tokio::task::spawn_blocking(move || {
        requested
            .into_iter()
            .map(|(path, basename)| {
                let canonical = std::fs::canonicalize(&path)
                    .map_err(|error| (error.kind(), basename.clone()))?;
                let metadata = std::fs::metadata(&canonical)
                    .map_err(|error| (error.kind(), basename.clone()))?;
                if !metadata.is_file() {
                    return Err((std::io::ErrorKind::InvalidInput, basename));
                }
                std::fs::File::open(&canonical).map_err(|error| (error.kind(), basename))?;
                canonical
                    .into_os_string()
                    .into_string()
                    .map_err(|_| (std::io::ErrorKind::InvalidData, "file".to_owned()))
            })
            .collect::<std::result::Result<Vec<_>, _>>()
    })
    .await
    .map_err(|_| upload_error(bound.target_id, ErrorCode::InteractionFailed, "file", false))?
    .map_err(|(kind, basename)| {
        upload_error(
            bound.target_id,
            if kind == std::io::ErrorKind::NotFound {
                ErrorCode::NotFound
            } else {
                ErrorCode::InteractionFailed
            },
            &basename,
            kind == std::io::ErrorKind::NotFound,
        )
    })?;

    send_cdp(
        transport,
        bound,
        "DOM.setFileInputFiles",
        json!({"files":canonical,"backendNodeId":node.backend_node_id}),
        cancel,
        generation,
    )
    .await?;
    Ok(())
}

fn upload_error(
    target_id: krometrail_core::TargetId,
    code: ErrorCode,
    basename: &str,
    missing: bool,
) -> krometrail_core::KrometrailError {
    let label = if missing {
        "upload_path_missing"
    } else {
        "upload_path_unreadable"
    };
    let message = format!("{label}: local file {basename:?} is not available for upload");
    operation_error(code, target_id, message)
        .with_retry(RetryAdvice::Safe)
        .with_recovery(
            NonEmptyText::new("provide an existing readable local file and retry").unwrap(),
        )
}
