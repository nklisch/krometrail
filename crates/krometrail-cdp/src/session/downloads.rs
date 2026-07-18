use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use krometrail_core::{
    CancelDownloadResult, CancellationSignal, DownloadDisplayName, DownloadId, DownloadInventory,
    DownloadSequence, DownloadState, ErrorCode, ErrorContext, IdSource, KrometrailError,
    MAX_MANAGED_DOWNLOAD_BYTES, MAX_MANAGED_DOWNLOADS, ManagedDownload, ManagedDownloadRead,
    NonEmptyText, ReadManagedDownloadRequest, Result, RetryAdvice, SanitizedUrl, SessionId,
    WaitForDownloadRequest,
};
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::transport::{CdpTransport, CommandScope, NamedEvent, TransportEvents};

pub(crate) struct ManagedDownloadAuthority {
    session_id: SessionId,
    root: PathBuf,
    ids: Arc<dyn IdSource>,
    state: Mutex<State>,
    gate: tokio::sync::Mutex<()>,
    changed: Notify,
}

struct State {
    accepting: bool,
    next_sequence: u64,
    by_guid: BTreeMap<String, Entry>,
    overflow_rejection: Option<ManagedDownload>,
    transport_generation: u64,
}

#[derive(Clone)]
struct Entry {
    public: ManagedDownload,
    guid: String,
    verified_size: Option<u64>,
}

impl ManagedDownloadAuthority {
    pub(crate) async fn configure(
        transport: Arc<dyn CdpTransport>,
        base_root: &Path,
        session_id: SessionId,
        ids: Arc<dyn IdSource>,
    ) -> Result<Arc<Self>> {
        // Subscribe before enabling downloads so no begin/progress event can race the tracker.
        let (begins, progress) = subscribe_download_events(transport.as_ref(), session_id).await?;
        let root = prepare_session_root(base_root, session_id)?;
        if transport
            .send_raw(
                &CommandScope::Browser,
                "Browser.setDownloadBehavior",
                json!({
                    "behavior": "allowAndName", "downloadPath": root, "eventsEnabled": true
                }),
            )
            .await
            .is_err()
        {
            let _ = std::fs::remove_dir_all(&root);
            return Err(download_error(
                ErrorCode::BrowserCompatibilityFailed,
                session_id,
                "managed download behavior could not be enabled",
                "update Chrome or restart the managed browser",
            ));
        }

        let authority = Arc::new(Self {
            session_id,
            root,
            ids,
            state: Mutex::new(State {
                accepting: true,
                next_sequence: 1,
                by_guid: BTreeMap::new(),
                overflow_rejection: None,
                transport_generation: 1,
            }),
            gate: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
        });
        spawn_begin_pump(Arc::clone(&authority), Arc::clone(&transport), 1, begins);
        spawn_progress_pump(Arc::clone(&authority), Arc::clone(&transport), 1, progress);
        Ok(authority)
    }

    pub(crate) fn list(&self) -> DownloadInventory {
        let state = self.state.lock().expect("download state lock");
        inventory(self.session_id, &state)
    }

    pub(crate) async fn rebind(self: &Arc<Self>, transport: Arc<dyn CdpTransport>) -> Result<()> {
        let _gate = self.gate.lock().await;
        let (begins, progress) =
            subscribe_download_events(transport.as_ref(), self.session_id).await?;
        transport
            .send_raw(
                &CommandScope::Browser,
                "Browser.setDownloadBehavior",
                json!({
                    "behavior": "allowAndName", "downloadPath": self.root, "eventsEnabled": true
                }),
            )
            .await
            .map_err(|_| {
                download_error(
                    ErrorCode::BrowserCompatibilityFailed,
                    self.session_id,
                    "managed download behavior could not be restored after reconnect",
                    "restart the managed browser session",
                )
            })?;
        let (generation, stale) = {
            let mut state = self.state.lock().expect("download state lock");
            state.transport_generation = state.transport_generation.saturating_add(1);
            let generation = state.transport_generation;
            let stale = state
                .by_guid
                .values()
                .filter(|entry| !is_terminal(entry.public.state))
                .map(|entry| entry.guid.clone())
                .collect::<Vec<_>>();
            for guid in &stale {
                let sequence = next_sequence(&mut state);
                if let Some(entry) = state.by_guid.get_mut(guid) {
                    entry.public.sequence = sequence;
                    entry.public.state = DownloadState::Failed;
                }
            }
            (generation, stale)
        };
        for guid in stale {
            let _ = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Browser.cancelDownload",
                    json!({"guid": guid}),
                )
                .await;
            remove_file(&self.root.join(guid));
        }
        spawn_begin_pump(Arc::clone(self), Arc::clone(&transport), generation, begins);
        spawn_progress_pump(Arc::clone(self), transport, generation, progress);
        self.changed.notify_waiters();
        Ok(())
    }

    pub(crate) async fn wait(&self, request: WaitForDownloadRequest) -> Result<DownloadInventory> {
        self.wait_with_cancellation(request, None).await
    }

    pub(crate) async fn wait_with_cancellation(
        &self,
        request: WaitForDownloadRequest,
        cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> Result<DownloadInventory> {
        request.validate()?;
        let timeout = Duration::from_millis(request.timeout);
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let notified = self.changed.notified();
            let snapshot = self.list();
            let after = request.after.map_or(0, DownloadSequence::get);
            let matched = snapshot.downloads.iter().any(|download| {
                download.sequence.get() > after
                    && request.download_id.is_none_or(|id| id == download.id)
                    && (!request.terminal || is_terminal(download.state))
            });
            if matched {
                if snapshot.downloads.iter().any(|download| {
                    download.sequence.get() > after
                        && download.state == DownloadState::Rejected
                        && request.download_id.is_none_or(|id| id == download.id)
                }) {
                    return Err(download_error(
                        ErrorCode::ResourceLimitExceeded,
                        self.session_id,
                        "managed download was rejected by a configured bound",
                        "finish or cancel active downloads and retry with a smaller file",
                    ));
                }
                return Ok(snapshot);
            }
            if let Some(signal) = cancellation.as_ref() {
                tokio::select! {
                    () = notified => {},
                    () = signal.cancelled() => return Err(download_error(ErrorCode::Cancelled, self.session_id, "managed download wait was cancelled", "list downloads to reconcile current state before retrying")),
                    () = tokio::time::sleep_until(deadline) => return Err(download_error(ErrorCode::WaitTimedOut, self.session_id, "managed download wait timed out", "list downloads, capture the returned cursor, and retry with a longer timeout")),
                }
            } else if tokio::time::timeout_at(deadline, notified).await.is_err() {
                return Err(download_error(
                    ErrorCode::WaitTimedOut,
                    self.session_id,
                    "managed download wait timed out",
                    "list downloads, capture the returned cursor, and retry with a longer timeout",
                ));
            }
        }
    }

    pub(crate) async fn cancel(
        &self,
        transport: &dyn CdpTransport,
        id: DownloadId,
    ) -> Result<CancelDownloadResult> {
        let _gate = self.gate.lock().await;
        let guid = {
            let state = self.state.lock().expect("download state lock");
            let entry = state
                .by_guid
                .values()
                .find(|entry| entry.public.id == id)
                .ok_or_else(|| {
                    download_error(
                        ErrorCode::NotFound,
                        self.session_id,
                        "managed download was not found",
                        "list active-session downloads and use one returned download_id",
                    )
                })?;
            if is_terminal(entry.public.state) {
                return Ok(CancelDownloadResult {
                    download_id: id,
                    state: entry.public.state,
                });
            }
            entry.guid.clone()
        };
        transport
            .send_raw(
                &CommandScope::Browser,
                "Browser.cancelDownload",
                json!({"guid": guid}),
            )
            .await
            .map_err(|_| {
                download_error(
                    ErrorCode::InteractionFailed,
                    self.session_id,
                    "managed download cancellation failed",
                    "list downloads to reconcile its current state before retrying",
                )
            })?;
        self.transition(&guid, DownloadState::Cancelled, None, None);
        remove_file(&self.root.join(&guid));
        Ok(CancelDownloadResult {
            download_id: id,
            state: DownloadState::Cancelled,
        })
    }

    pub(crate) async fn read(
        &self,
        request: ReadManagedDownloadRequest,
    ) -> Result<ManagedDownloadRead> {
        if request.session_id != self.session_id {
            return Err(resource_not_found(self.session_id));
        }
        let (guid, expected) = {
            let state = self.state.lock().expect("download state lock");
            let entry = state
                .by_guid
                .values()
                .find(|entry| {
                    entry.public.id == request.download_id
                        && entry.public.state == DownloadState::Completed
                })
                .ok_or_else(|| resource_not_found(self.session_id))?;
            (
                entry.guid.clone(),
                entry
                    .verified_size
                    .ok_or_else(|| resource_not_found(self.session_id))?,
            )
        };
        if expected > request.max_bytes || expected > MAX_MANAGED_DOWNLOAD_BYTES {
            return Err(resource_not_found(self.session_id));
        }
        let root = self.root.clone();
        let session_id = self.session_id;
        let bytes = tokio::task::spawn_blocking(move || {
            let path = verified_file(&root, &guid, expected)?;
            std::fs::read(path).map_err(|_| resource_not_found(session_id))
        })
        .await
        .map_err(|_| resource_not_found(self.session_id))??;
        if bytes.len() as u64 != expected {
            return Err(resource_not_found(self.session_id));
        }
        Ok(ManagedDownloadRead {
            session_id: self.session_id,
            download_id: request.download_id,
            media_type: NonEmptyText::new("application/octet-stream").unwrap(),
            bytes,
        })
    }

    pub(crate) async fn shutdown(&self, transport: Option<&dyn CdpTransport>) -> Result<()> {
        let _gate = self.gate.lock().await;
        let active = {
            let mut state = self.state.lock().expect("download state lock");
            state.accepting = false;
            state
                .by_guid
                .values()
                .filter(|entry| !is_terminal(entry.public.state))
                .map(|entry| entry.guid.clone())
                .collect::<Vec<_>>()
        };
        if let Some(transport) = transport {
            for guid in active {
                let _ = transport
                    .send_raw(
                        &CommandScope::Browser,
                        "Browser.cancelDownload",
                        json!({"guid": guid}),
                    )
                    .await;
            }
        }
        match std::fs::remove_dir_all(&self.root) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(download_error(
                    ErrorCode::ShutdownIncomplete,
                    self.session_id,
                    "managed download cleanup was incomplete",
                    "remove the private Krometrail download session directory before retrying",
                ));
            }
        }
        Ok(())
    }

    async fn begin(&self, generation: u64, transport: &dyn CdpTransport, params: &Value) {
        let _gate = self.gate.lock().await;
        let Some(guid) = params
            .get("guid")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return;
        };
        if guid.is_empty() || guid.contains(['/', '\\']) {
            return;
        }
        let overflow = {
            let mut state = self.state.lock().expect("download state lock");
            if !state.accepting
                || state.transport_generation != generation
                || state.by_guid.contains_key(&guid)
            {
                return;
            }
            if state.by_guid.len() >= MAX_MANAGED_DOWNLOADS {
                let sequence = next_sequence(&mut state);
                state.overflow_rejection = Some(ManagedDownload {
                    id: DownloadId::from_uuid(*self.ids.next().as_uuid()),
                    sequence,
                    target_id: None,
                    state: DownloadState::Rejected,
                    suggested_filename: DownloadDisplayName::sanitize(
                        params
                            .get("suggestedFilename")
                            .and_then(Value::as_str)
                            .unwrap_or("download"),
                    ),
                    source_url: sanitize_download_url(params),
                    received_bytes: 0,
                    total_bytes: None,
                    resource_uri: None,
                });
                true
            } else {
                let sequence = next_sequence(&mut state);
                let id = DownloadId::from_uuid(*self.ids.next().as_uuid());
                let public = ManagedDownload {
                    id,
                    sequence,
                    target_id: None,
                    state: DownloadState::InProgress,
                    suggested_filename: DownloadDisplayName::sanitize(
                        params
                            .get("suggestedFilename")
                            .and_then(Value::as_str)
                            .unwrap_or("download"),
                    ),
                    source_url: sanitize_download_url(params),
                    received_bytes: 0,
                    total_bytes: None,
                    resource_uri: None,
                };
                state.by_guid.insert(
                    guid.clone(),
                    Entry {
                        public,
                        guid: guid.clone(),
                        verified_size: None,
                    },
                );
                false
            }
        };
        if overflow {
            let _ = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Browser.cancelDownload",
                    json!({"guid": guid}),
                )
                .await;
            remove_file(&self.root.join(&guid));
            self.changed.notify_waiters();
            return;
        }
        self.changed.notify_waiters();
    }

    async fn progress(&self, generation: u64, transport: &dyn CdpTransport, params: &Value) {
        let _gate = self.gate.lock().await;
        if self
            .state
            .lock()
            .expect("download state lock")
            .transport_generation
            != generation
        {
            return;
        }
        let Some(guid) = params.get("guid").and_then(Value::as_str) else {
            return;
        };
        let received = params
            .get("receivedBytes")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let total = params
            .get("totalBytes")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0);
        if received > MAX_MANAGED_DOWNLOAD_BYTES
            || total.is_some_and(|value| value > MAX_MANAGED_DOWNLOAD_BYTES)
        {
            let _ = transport
                .send_raw(
                    &CommandScope::Browser,
                    "Browser.cancelDownload",
                    json!({"guid": guid}),
                )
                .await;
            self.transition(guid, DownloadState::Rejected, Some(received), total);
            remove_file(&self.root.join(guid));
            return;
        }
        match params.get("state").and_then(Value::as_str) {
            Some("completed") => {
                let path = self.root.join(guid);
                let verified = wait_for_verified_file(&self.root, guid, received).await;
                match verified {
                    Ok(size) => self.complete(guid, size, total),
                    Err(()) => {
                        self.transition(guid, DownloadState::Failed, Some(received), total);
                        remove_file(&path);
                    }
                }
            }
            Some("canceled") => {
                self.transition(guid, DownloadState::Cancelled, Some(received), total);
                remove_file(&self.root.join(guid));
            }
            _ => self.transition(guid, DownloadState::InProgress, Some(received), total),
        }
    }

    fn complete(&self, guid: &str, size: u64, total: Option<u64>) {
        let mut state = self.state.lock().expect("download state lock");
        if state
            .by_guid
            .get(guid)
            .is_none_or(|entry| is_terminal(entry.public.state))
        {
            return;
        }
        let sequence = next_sequence(&mut state);
        if let Some(entry) = state.by_guid.get_mut(guid) {
            entry.public.sequence = sequence;
            entry.public.state = DownloadState::Completed;
            entry.public.received_bytes = size;
            entry.public.total_bytes = total;
            entry.public.resource_uri = Some(format!(
                "krometrail://local/{}/downloads/{}",
                self.session_id, entry.public.id
            ));
            entry.verified_size = Some(size);
        }
        drop(state);
        self.changed.notify_waiters();
    }

    fn transition(
        &self,
        guid: &str,
        state_value: DownloadState,
        received: Option<u64>,
        total: Option<u64>,
    ) {
        let mut state = self.state.lock().expect("download state lock");
        let sequence = next_sequence(&mut state);
        if let Some(entry) = state.by_guid.get_mut(guid) {
            if is_terminal(entry.public.state) {
                return;
            }
            entry.public.sequence = sequence;
            entry.public.state = state_value;
            if let Some(received) = received {
                entry.public.received_bytes = received;
            }
            entry.public.total_bytes = total.or(entry.public.total_bytes);
        }
        drop(state);
        self.changed.notify_waiters();
    }
}

fn spawn_begin_pump(
    authority: Arc<ManagedDownloadAuthority>,
    transport: Arc<dyn CdpTransport>,
    generation: u64,
    mut events: Box<dyn TransportEvents>,
) {
    tokio::spawn(async move {
        while let Ok(Some(NamedEvent { params, .. })) = events.next().await {
            authority
                .begin(generation, transport.as_ref(), &params)
                .await;
        }
    });
}

fn spawn_progress_pump(
    authority: Arc<ManagedDownloadAuthority>,
    transport: Arc<dyn CdpTransport>,
    generation: u64,
    mut events: Box<dyn TransportEvents>,
) {
    tokio::spawn(async move {
        while let Ok(Some(NamedEvent { params, .. })) = events.next().await {
            authority
                .progress(generation, transport.as_ref(), &params)
                .await;
        }
    });
}

fn prepare_session_root(base: &Path, session_id: SessionId) -> Result<PathBuf> {
    std::fs::create_dir_all(base).map_err(|_| {
        download_error(
            ErrorCode::PersistenceFailed,
            session_id,
            "managed download root could not be created",
            "check Krometrail data-directory permissions and retry",
        )
    })?;
    let base = std::fs::canonicalize(base).map_err(|_| {
        download_error(
            ErrorCode::PersistenceFailed,
            session_id,
            "managed download root could not be verified",
            "check Krometrail data-directory permissions and retry",
        )
    })?;
    let root = base.join(session_id.to_string());
    std::fs::create_dir(&root).map_err(|_| {
        download_error(
            ErrorCode::PersistenceFailed,
            session_id,
            "private download session directory could not be created",
            "stop any stale session and retry",
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).map_err(|_| {
            let error = download_error(
                ErrorCode::PersistenceFailed,
                session_id,
                "private download permissions could not be applied",
                "check Krometrail data-directory permissions and retry",
            );
            let _ = std::fs::remove_dir_all(&root);
            error
        })?;
    }
    let root = std::fs::canonicalize(&root).map_err(|_| {
        let error = download_error(
            ErrorCode::PersistenceFailed,
            session_id,
            "private download session directory could not be verified",
            "check Krometrail data-directory permissions and retry",
        );
        let _ = std::fs::remove_dir_all(&root);
        error
    })?;
    if !root.starts_with(base) {
        let _ = std::fs::remove_dir_all(&root);
        return Err(download_error(
            ErrorCode::PersistenceFailed,
            session_id,
            "private download directory escaped its managed root",
            "remove the invalid Krometrail download root and retry",
        ));
    }
    Ok(root)
}

async fn wait_for_verified_file(
    root: &Path,
    guid: &str,
    reported: u64,
) -> std::result::Result<u64, ()> {
    for _ in 0..10 {
        if let Ok(path) = verified_file(root, guid, reported) {
            return std::fs::metadata(path)
                .map(|value| value.len())
                .map_err(|_| ());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    Err(())
}

fn verified_file(root: &Path, guid: &str, expected: u64) -> Result<PathBuf> {
    let path = root.join(guid);
    let metadata = std::fs::symlink_metadata(&path).map_err(|_| resource_not_found_for_path())?;
    if !metadata.file_type().is_file()
        || metadata.len() != expected
        || metadata.len() > MAX_MANAGED_DOWNLOAD_BYTES
    {
        return Err(resource_not_found_for_path());
    }
    let canonical = std::fs::canonicalize(&path).map_err(|_| resource_not_found_for_path())?;
    if !canonical.starts_with(root) {
        return Err(resource_not_found_for_path());
    }
    Ok(canonical)
}

fn inventory(session_id: SessionId, state: &State) -> DownloadInventory {
    let cursor = state
        .by_guid
        .values()
        .map(|entry| entry.public.sequence)
        .max();
    let mut downloads = state
        .by_guid
        .values()
        .map(|entry| entry.public.clone())
        .collect::<Vec<_>>();
    if let Some(rejected) = state.overflow_rejection.clone() {
        downloads.push(rejected);
    }
    downloads.sort_by_key(|entry| entry.sequence);
    DownloadInventory {
        session_id,
        cursor,
        downloads,
    }
}

async fn subscribe_download_events(
    transport: &dyn CdpTransport,
    session_id: SessionId,
) -> Result<(Box<dyn TransportEvents>, Box<dyn TransportEvents>)> {
    let begins = transport
        .subscribe_named(&CommandScope::Browser, "Browser.downloadWillBegin")
        .await
        .map_err(|_| {
            download_error(
                ErrorCode::BrowserCompatibilityFailed,
                session_id,
                "browser download events are unavailable",
                "update Chrome or use a supported managed Chromium installation",
            )
        })?;
    let progress = transport
        .subscribe_named(&CommandScope::Browser, "Browser.downloadProgress")
        .await
        .map_err(|_| {
            download_error(
                ErrorCode::BrowserCompatibilityFailed,
                session_id,
                "browser download progress events are unavailable",
                "update Chrome or use a supported managed Chromium installation",
            )
        })?;
    Ok((begins, progress))
}

fn sanitize_download_url(params: &Value) -> SanitizedUrl {
    SanitizedUrl::sanitize(
        params
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or("about:blank"),
    )
    .unwrap_or_else(|_| SanitizedUrl::sanitize("about:blank").expect("fallback URL sanitizes"))
}
fn next_sequence(state: &mut State) -> DownloadSequence {
    let value = DownloadSequence::new(state.next_sequence).expect("positive sequence");
    state.next_sequence = state.next_sequence.saturating_add(1);
    value
}
fn is_terminal(state: DownloadState) -> bool {
    matches!(
        state,
        DownloadState::Completed
            | DownloadState::Cancelled
            | DownloadState::Failed
            | DownloadState::Rejected
    )
}
fn remove_file(path: &Path) {
    let _ = std::fs::remove_file(path);
}
fn resource_not_found_for_path() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::NotFound,
        NonEmptyText::new("managed download resource is unavailable").unwrap(),
    )
}
fn resource_not_found(session_id: SessionId) -> KrometrailError {
    download_error(
        ErrorCode::NotFound,
        session_id,
        "managed download resource is unavailable",
        "list completed downloads in the active browser session and use its canonical resource URI",
    )
}
fn download_error(
    code: ErrorCode,
    session_id: SessionId,
    message: &'static str,
    recovery: &'static str,
) -> KrometrailError {
    KrometrailError::new(code, NonEmptyText::new(message).unwrap())
        .with_context(ErrorContext {
            session_id: Some(session_id),
            ..ErrorContext::default()
        })
        .with_retry(RetryAdvice::AfterRecovery)
        .with_recovery(NonEmptyText::new(recovery).unwrap())
}

impl Drop for ManagedDownloadAuthority {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::transport::{TransportClose, TransportError, TransportFuture};

    struct Ids(AtomicU64);
    impl IdSource for Ids {
        fn next(&self) -> krometrail_core::IdValue {
            krometrail_core::IdValue::from_uuid(uuid::Uuid::from_u128(
                self.0.fetch_add(1, Ordering::Relaxed) as u128 + 1,
            ))
        }
    }
    struct NoEvents;
    impl TransportEvents for NoEvents {
        fn next(
            &mut self,
        ) -> TransportFuture<'_, std::result::Result<Option<NamedEvent>, TransportError>> {
            Box::pin(std::future::ready(Ok(None)))
        }
    }
    struct Transport {
        calls: Mutex<Vec<(String, Value)>>,
    }
    impl CdpTransport for Transport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            self.calls.lock().unwrap().push((method.to_owned(), params));
            Box::pin(std::future::ready(Ok(json!({}))))
        }
        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            _method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            Box::pin(std::future::ready(Ok(
                Box::new(NoEvents) as Box<dyn TransportEvents>
            )))
        }
        fn close_reason(&self) -> Option<TransportClose> {
            None
        }
        fn is_closed(&self) -> bool {
            false
        }
    }

    fn authority(base: &Path) -> Arc<ManagedDownloadAuthority> {
        let session_id = SessionId::from_uuid(uuid::Uuid::from_u128(100));
        let root = prepare_session_root(base, session_id).unwrap();
        Arc::new(ManagedDownloadAuthority {
            session_id,
            root,
            ids: Arc::new(Ids(AtomicU64::new(0))),
            state: Mutex::new(State {
                accepting: true,
                next_sequence: 1,
                by_guid: BTreeMap::new(),
                overflow_rejection: None,
                transport_generation: 1,
            }),
            gate: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
        })
    }

    #[tokio::test]
    async fn completion_is_published_only_after_exact_contained_file_and_cleanup_is_scoped() {
        let temp = tempfile::tempdir().unwrap();
        let sibling = temp.path().join("sibling");
        std::fs::create_dir(&sibling).unwrap();
        let authority = authority(temp.path());
        authority
            .begin(
                1,
                &Transport {
                    calls: Mutex::new(Vec::new()),
                },
                &json!({"guid":"opaque-guid","url":"https://example.test/private?token=secret","suggestedFilename":"../report.txt"}),
            )
            .await;
        std::fs::write(authority.root.join("opaque-guid"), b"exact bytes").unwrap();
        let transport = Transport {
            calls: Mutex::new(Vec::new()),
        };
        authority.progress(1, &transport, &json!({"guid":"opaque-guid","state":"completed","receivedBytes":11,"totalBytes":11})).await;
        let item = authority.list().downloads.into_iter().next().unwrap();
        assert_eq!(item.state, DownloadState::Completed);
        assert!(
            item.resource_uri
                .as_deref()
                .unwrap()
                .starts_with("krometrail://local/")
        );
        let read = authority
            .read(ReadManagedDownloadRequest {
                session_id: authority.session_id,
                download_id: item.id,
                max_bytes: 11,
            })
            .await
            .unwrap();
        assert_eq!(read.bytes, b"exact bytes");
        authority.shutdown(Some(&transport)).await.unwrap();
        authority.shutdown(Some(&transport)).await.unwrap();
        assert!(sibling.is_dir());
        assert!(!authority.root.exists());
    }

    #[tokio::test]
    async fn oversize_progress_cancels_once_and_never_publishes_a_resource() {
        let temp = tempfile::tempdir().unwrap();
        let authority = authority(temp.path());
        authority.begin(1, &Transport { calls: Mutex::new(Vec::new()) }, &json!({"guid":"large-guid","url":"https://example.test/a","suggestedFilename":"large.bin"})).await;
        let transport = Transport {
            calls: Mutex::new(Vec::new()),
        };
        authority.progress(1, &transport, &json!({"guid":"large-guid","state":"inProgress","receivedBytes":MAX_MANAGED_DOWNLOAD_BYTES + 1})).await;
        let item = authority.list().downloads.into_iter().next().unwrap();
        assert_eq!(item.state, DownloadState::Rejected);
        assert!(item.resource_uri.is_none());
        assert_eq!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "Browser.cancelDownload")
                .count(),
            1
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_completion_fails_without_reading_or_removing_its_target() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        std::fs::write(&outside, b"outside").unwrap();
        let authority = authority(&temp.path().join("managed"));
        authority
            .begin(
                1,
                &Transport {
                    calls: Mutex::new(Vec::new()),
                },
                &json!({"guid":"link-guid","url":"https://example.test/a","suggestedFilename":"a"}),
            )
            .await;
        symlink(&outside, authority.root.join("link-guid")).unwrap();
        let transport = Transport {
            calls: Mutex::new(Vec::new()),
        };
        authority
            .progress(
                1,
                &transport,
                &json!({"guid":"link-guid","state":"completed","receivedBytes":7}),
            )
            .await;
        assert_eq!(authority.list().downloads[0].state, DownloadState::Failed);
        assert_eq!(std::fs::read(outside).unwrap(), b"outside");
    }

    #[tokio::test]
    async fn wait_observes_a_change_after_the_captured_cursor_without_a_race() {
        let temp = tempfile::tempdir().unwrap();
        let authority = authority(temp.path());
        authority
            .begin(
                1,
                &Transport {
                    calls: Mutex::new(Vec::new()),
                },
                &json!({"guid":"wait-guid","url":"https://example.test/a","suggestedFilename":"a"}),
            )
            .await;
        let before = authority.list();
        let id = before.downloads[0].id;
        let waiting = {
            let authority = Arc::clone(&authority);
            tokio::spawn(async move {
                authority
                    .wait_with_cancellation(
                        WaitForDownloadRequest {
                            after: before.cursor,
                            download_id: Some(id),
                            terminal: true,
                            timeout: 1000,
                        },
                        None,
                    )
                    .await
            })
        };
        authority.transition("wait-guid", DownloadState::Cancelled, Some(0), None);
        let result = waiting.await.unwrap().unwrap();
        assert_eq!(result.downloads[0].state, DownloadState::Cancelled);
    }
}
