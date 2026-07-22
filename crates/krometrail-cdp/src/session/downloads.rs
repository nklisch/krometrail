use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use krometrail_core::{
    BrowserSessionEvent, CancellationSignal, DownloadDisplayName, DownloadId, DownloadInventory,
    DownloadSequence, DownloadState, ErrorCode, ErrorContext, IdSource, KrometrailError,
    MAX_MANAGED_DOWNLOAD_BYTES, MAX_MANAGED_DOWNLOADS, ManagedDownload, ManagedDownloadRead,
    NonEmptyText, ReadManagedDownloadRequest, Result, RetryAdvice, SanitizedUrl, SessionId,
    WaitForDownloadRequest,
};
use serde_json::{Value, json};
use tokio::sync::Notify;

use crate::targets::supervisor::SubscriberRegistry;
use crate::transport::{CdpTransport, CommandScope, NamedEvent, TransportEvents};

/// The eagerly-activated managed download control. `activate` runs once at
/// managed session start — before any interaction can dispatch — so download
/// events are subscribed and managed download behavior is enabled for the
/// whole session. Activation failure stores the unavailable error and never
/// fails session start: explicit download operations report the stored error
/// and interaction facts degrade to absent.
pub(crate) struct ManagedDownloadControl {
    session_id: SessionId,
    base_root: PathBuf,
    ids: Arc<dyn IdSource>,
    subscribers: Arc<SubscriberRegistry>,
    active: std::sync::OnceLock<Arc<ManagedDownloadAuthority>>,
    unavailable: Mutex<Option<KrometrailError>>,
}

impl ManagedDownloadControl {
    pub(crate) fn new(
        base_root: PathBuf,
        session_id: SessionId,
        ids: Arc<dyn IdSource>,
        subscribers: Arc<SubscriberRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            session_id,
            base_root,
            ids,
            subscribers,
            active: std::sync::OnceLock::new(),
            unavailable: Mutex::new(None),
        })
    }

    /// Called once at managed session start. Subscribe-before-enable ordering
    /// is preserved by [`ManagedDownloadAuthority::configure`].
    pub(crate) async fn activate(&self, transport: Arc<dyn CdpTransport>) -> Result<()> {
        if self.active.get().is_some() {
            return Ok(());
        }
        match ManagedDownloadAuthority::configure(
            transport,
            &self.base_root,
            self.session_id,
            Arc::clone(&self.ids),
            Arc::clone(&self.subscribers),
        )
        .await
        {
            Ok(authority) => {
                let _ = self.active.set(authority);
                self.unavailable
                    .lock()
                    .expect("download failure lock")
                    .take();
                Ok(())
            }
            Err(error) => {
                *self.unavailable.lock().expect("download failure lock") = Some(error.clone());
                Err(error)
            }
        }
    }

    fn activated(&self) -> Result<&Arc<ManagedDownloadAuthority>> {
        if let Some(error) = self
            .unavailable
            .lock()
            .expect("download failure lock")
            .clone()
        {
            return Err(error);
        }
        self.active.get().ok_or_else(|| {
            download_error(
                ErrorCode::Unsupported,
                self.session_id,
                "managed download control is unavailable in this session",
                "restart the managed browser session and retry",
            )
        })
    }

    /// Sync pre-dispatch cursor capture for postcondition assembly.
    /// `None` only when activation failed for this session.
    pub(crate) fn cursor(&self) -> Option<DownloadSequence> {
        self.active.get().map(|authority| authority.cursor())
    }

    /// Sync begun-after delta for postcondition assembly.
    pub(crate) fn begun_after(
        &self,
        cursor: DownloadSequence,
    ) -> Vec<krometrail_core::DownloadFact> {
        self.active
            .get()
            .map_or_else(Vec::new, |authority| authority.begun_after(cursor))
    }

    pub(crate) fn list(&self) -> Result<DownloadInventory> {
        Ok(self.activated()?.list())
    }

    pub(crate) async fn wait_with_cancellation(
        &self,
        request: WaitForDownloadRequest,
        cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> Result<DownloadInventory> {
        self.activated()?
            .wait_with_cancellation(request, cancellation)
            .await
    }

    pub(crate) async fn cancel(
        &self,
        transport: &dyn CdpTransport,
        id: DownloadId,
    ) -> Result<DownloadState> {
        self.activated()?.cancel(transport, id).await
    }

    pub(crate) async fn read(
        &self,
        request: ReadManagedDownloadRequest,
    ) -> Result<ManagedDownloadRead> {
        if self
            .unavailable
            .lock()
            .expect("download failure lock")
            .is_some()
        {
            return Err(resource_not_found(self.session_id));
        }
        match self.active.get() {
            Some(authority) => authority.read(request).await,
            None => Err(resource_not_found(self.session_id)),
        }
    }

    pub(crate) async fn rebind(&self, transport: Arc<dyn CdpTransport>) -> Result<()> {
        if let Some(error) = self
            .unavailable
            .lock()
            .expect("download failure lock")
            .clone()
        {
            return Err(error);
        }
        match self.active.get() {
            Some(authority) => match authority.rebind(Arc::clone(&transport)).await {
                Ok(()) => Ok(()),
                Err(error) => {
                    let _ = authority.shutdown(Some(transport.as_ref())).await;
                    *self.unavailable.lock().expect("download failure lock") = Some(error.clone());
                    Err(error)
                }
            },
            None => Ok(()),
        }
    }

    pub(crate) async fn shutdown(&self, transport: Option<&dyn CdpTransport>) -> Result<()> {
        match self.active.get() {
            Some(authority) => authority.shutdown(transport).await,
            None => Ok(()),
        }
    }
}

pub(crate) struct ManagedDownloadAuthority {
    session_id: SessionId,
    root: PathBuf,
    ids: Arc<dyn IdSource>,
    state: Mutex<State>,
    gate: tokio::sync::Mutex<()>,
    changed: Notify,
    subscribers: Option<Arc<SubscriberRegistry>>,
    lease: Mutex<Option<File>>,
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
    /// The sequence assigned at `begin`, retained while `public.sequence`
    /// bumps on every transition: postcondition deltas key on begin ordering
    /// so a pre-action download's later progress is never attributed to the
    /// current interaction.
    begun_sequence: DownloadSequence,
    verified_size: Option<u64>,
    media_type: NonEmptyText,
}

// Sequence seeding mirrors the page cursor: sequence 1 is the
// empty-inventory cursor, so the cursor is never absent and waiting after an
// initial empty inventory cannot miss the first download.
const INITIAL_NEXT_SEQUENCE: u64 = 2;

impl ManagedDownloadAuthority {
    pub(crate) async fn configure(
        transport: Arc<dyn CdpTransport>,
        base_root: &Path,
        session_id: SessionId,
        ids: Arc<dyn IdSource>,
        subscribers: Arc<SubscriberRegistry>,
    ) -> Result<Arc<Self>> {
        // Subscribe before enabling downloads so no begin/progress event can race the tracker.
        let (begins, progress) = subscribe_download_events(transport.as_ref(), session_id).await?;
        let PreparedSessionRoot { root, lease } = prepare_session_root(base_root, session_id)?;
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
            drop(lease);
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
                next_sequence: INITIAL_NEXT_SEQUENCE,
                by_guid: BTreeMap::new(),
                overflow_rejection: None,
                transport_generation: 1,
            }),
            gate: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
            subscribers: Some(subscribers),
            lease: Mutex::new(Some(lease)),
        });
        spawn_begin_pump(Arc::clone(&authority), Arc::clone(&transport), 1, begins);
        spawn_progress_pump(Arc::clone(&authority), Arc::clone(&transport), 1, progress);
        Ok(authority)
    }

    pub(crate) fn list(&self) -> DownloadInventory {
        let state = self.state.lock().expect("download state lock");
        inventory(self.session_id, &state)
    }

    /// The never-absent inventory cursor: the last assigned sequence, with
    /// the seeded value covering an empty inventory.
    pub(crate) fn cursor(&self) -> DownloadSequence {
        let state = self.state.lock().expect("download state lock");
        state_cursor(&state)
    }

    /// Downloads whose begin was recorded after `cursor`, in begin order,
    /// with their state at the observation point.
    pub(crate) fn begun_after(
        &self,
        cursor: DownloadSequence,
    ) -> Vec<krometrail_core::DownloadFact> {
        let state = self.state.lock().expect("download state lock");
        let mut facts = state
            .by_guid
            .values()
            .filter(|entry| entry.begun_sequence > cursor)
            .map(|entry| krometrail_core::DownloadFact {
                download_id: entry.public.id,
                sequence: entry.public.sequence,
                state: entry.public.state,
            })
            .collect::<Vec<_>>();
        facts.sort_by_key(|fact| fact.sequence);
        facts
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
        let (generation, stale, published) = {
            let mut state = self.state.lock().expect("download state lock");
            state.transport_generation = state.transport_generation.saturating_add(1);
            let generation = state.transport_generation;
            let stale = state
                .by_guid
                .iter()
                .filter(|(_, entry)| !is_terminal(entry.public.state))
                .map(|(guid, _)| guid.clone())
                .collect::<Vec<_>>();
            for guid in &stale {
                let sequence = next_sequence(&mut state);
                if let Some(entry) = state.by_guid.get_mut(guid) {
                    entry.public.sequence = sequence;
                    entry.public.state = DownloadState::Failed;
                }
            }
            let published = stale
                .iter()
                .filter_map(|guid| state.by_guid.get(guid).map(|entry| entry.public.clone()))
                .collect::<Vec<_>>();
            (generation, stale, published)
        };
        for download in &published {
            self.publish(download);
        }
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
            let after = request.after.get();
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
    ) -> Result<DownloadState> {
        let _gate = self.gate.lock().await;
        let guid = {
            let state = self.state.lock().expect("download state lock");
            let (guid, entry) = state
                .by_guid
                .iter()
                .find(|(_, entry)| entry.public.id == id)
                .ok_or_else(|| {
                    download_error(
                        ErrorCode::NotFound,
                        self.session_id,
                        "managed download was not found",
                        "list active-session downloads and use one returned download_id",
                    )
                })?;
            if is_terminal(entry.public.state) {
                return Ok(entry.public.state);
            }
            guid.clone()
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
        Ok(DownloadState::Cancelled)
    }

    pub(crate) async fn read(
        &self,
        request: ReadManagedDownloadRequest,
    ) -> Result<ManagedDownloadRead> {
        if request.session_id != self.session_id {
            return Err(resource_not_found(self.session_id));
        }
        let (guid, expected, media_type) = {
            let state = self.state.lock().expect("download state lock");
            let (guid, entry) = state
                .by_guid
                .iter()
                .find(|(_, entry)| {
                    entry.public.id == request.download_id
                        && entry.public.state == DownloadState::Completed
                })
                .ok_or_else(|| resource_not_found(self.session_id))?;
            (
                guid.clone(),
                entry
                    .verified_size
                    .ok_or_else(|| resource_not_found(self.session_id))?,
                entry.media_type.clone(),
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
            media_type,
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
                .iter()
                .filter(|(_, entry)| !is_terminal(entry.public.state))
                .map(|(guid, _)| guid.clone())
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
        self.lease.lock().expect("download lease lock").take();
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
        let (overflow, published) = {
            let mut state = self.state.lock().expect("download state lock");
            if !state.accepting
                || state.transport_generation != generation
                || state.by_guid.contains_key(&guid)
            {
                return;
            }
            if state.by_guid.len() >= MAX_MANAGED_DOWNLOADS {
                let sequence = next_sequence(&mut state);
                let rejected = ManagedDownload {
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
                };
                state.overflow_rejection = Some(rejected.clone());
                (true, rejected)
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
                        public: public.clone(),
                        begun_sequence: sequence,
                        verified_size: None,
                        media_type: download_media_type(params),
                    },
                );
                (false, public)
            }
        };
        self.publish(&published);
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
        let published = if let Some(entry) = state.by_guid.get_mut(guid) {
            entry.public.sequence = sequence;
            entry.public.state = DownloadState::Completed;
            entry.public.received_bytes = size;
            entry.public.total_bytes = total;
            entry.public.resource_uri = Some(format!(
                "krometrail://local/{}/downloads/{}",
                self.session_id, entry.public.id
            ));
            entry.verified_size = Some(size);
            Some(entry.public.clone())
        } else {
            None
        };
        drop(state);
        if let Some(download) = published.as_ref() {
            self.publish(download);
        }
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
        let published = if let Some(entry) = state.by_guid.get_mut(guid) {
            if is_terminal(entry.public.state) {
                return;
            }
            entry.public.sequence = sequence;
            entry.public.state = state_value;
            if let Some(received) = received {
                entry.public.received_bytes = received;
            }
            entry.public.total_bytes = total.or(entry.public.total_bytes);
            Some(entry.public.clone())
        } else {
            None
        };
        drop(state);
        if let Some(download) = published.as_ref() {
            self.publish(download);
        }
        self.changed.notify_waiters();
    }

    fn publish(&self, download: &ManagedDownload) {
        if let Some(subscribers) = self.subscribers.as_ref() {
            subscribers.publish(BrowserSessionEvent::DownloadStateChanged {
                download_id: download.id,
                target_id: download.target_id,
                state: download.state,
                received_bytes: download.received_bytes,
                total_bytes: download.total_bytes,
            });
        }
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

const ACTIVE_MARKER: &str = ".krometrail-active";
const SCAVENGE_LOCK: &str = ".krometrail-scavenge";

struct PreparedSessionRoot {
    root: PathBuf,
    lease: File,
}

fn prepare_session_root(base: &Path, session_id: SessionId) -> Result<PreparedSessionRoot> {
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
    let scavenge_lock = open_lock_file(&base.join(SCAVENGE_LOCK), true).map_err(|_| {
        cleanup_error(
            session_id,
            "managed download cleanup coordination could not be opened",
        )
    })?;
    if !try_lock_file(&scavenge_lock).map_err(|_| {
        cleanup_error(
            session_id,
            "managed download cleanup is already active or unavailable",
        )
    })? {
        return Err(cleanup_error(
            session_id,
            "managed download cleanup is already active or unavailable",
        ));
    }
    scavenge_stale_session_roots(&base, session_id)?;
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
    let lease = open_lock_file(&root.join(ACTIVE_MARKER), true).map_err(|_| {
        let error = cleanup_error(
            session_id,
            "private download session ownership could not be created",
        );
        let _ = std::fs::remove_dir_all(&root);
        error
    })?;
    if !try_lock_file(&lease).map_err(|_| {
        let error = cleanup_error(
            session_id,
            "private download session ownership could not be acquired",
        );
        let _ = std::fs::remove_dir_all(&root);
        error
    })? {
        let _ = std::fs::remove_dir_all(&root);
        return Err(cleanup_error(
            session_id,
            "private download session ownership is already active",
        ));
    }
    let _ = unlock_file(&scavenge_lock);
    Ok(PreparedSessionRoot { root, lease })
}

fn scavenge_stale_session_roots(base: &Path, current: SessionId) -> Result<()> {
    let entries = std::fs::read_dir(base).map_err(|_| {
        cleanup_error(
            current,
            "managed download cleanup could not enumerate its private root",
        )
    })?;
    for entry in entries {
        let entry = entry.map_err(|_| {
            cleanup_error(
                current,
                "managed download cleanup could not inspect a private-root entry",
            )
        })?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let Some(candidate) = canonical_session_id(&name) else {
            continue;
        };
        if candidate == current {
            continue;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|_| {
            cleanup_error(
                current,
                "managed download cleanup could not verify a session entry",
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_dir() {
            return Err(cleanup_error(
                current,
                "managed download cleanup rejected an invalid session entry",
            ));
        }
        let canonical = std::fs::canonicalize(&path).map_err(|_| {
            cleanup_error(
                current,
                "managed download cleanup could not verify a session directory",
            )
        })?;
        if canonical.parent() != Some(base)
            || canonical.file_name().and_then(|v| v.to_str()) != Some(name.as_str())
        {
            return Err(cleanup_error(
                current,
                "managed download cleanup rejected an escaped session directory",
            ));
        }
        let marker = canonical.join(ACTIVE_MARKER);
        let active = match std::fs::symlink_metadata(&marker) {
            Ok(marker_metadata) => {
                if marker_metadata.file_type().is_symlink()
                    || !marker_metadata.file_type().is_file()
                {
                    return Err(cleanup_error(
                        current,
                        "managed download cleanup rejected an invalid ownership marker",
                    ));
                }
                probe_active_marker(&marker).map_err(|_| {
                    cleanup_error(
                        current,
                        "managed download cleanup could not verify session ownership",
                    )
                })?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(_) => {
                return Err(cleanup_error(
                    current,
                    "managed download cleanup could not inspect session ownership",
                ));
            }
        };
        if !active {
            std::fs::remove_dir_all(&canonical).map_err(|_| {
                cleanup_error(
                    current,
                    "managed download cleanup could not remove a stale session directory",
                )
            })?;
        }
    }
    Ok(())
}

fn canonical_session_id(name: &str) -> Option<SessionId> {
    let value = uuid::Uuid::parse_str(name).ok()?;
    (!value.is_nil() && value.to_string() == name).then(|| SessionId::from_uuid(value))
}

fn open_lock_file(path: &Path, create: bool) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(create);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    options.open(path)
}

#[cfg(unix)]
fn try_lock_file(file: &File) -> std::io::Result<bool> {
    use std::os::fd::AsRawFd;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        Ok(true)
    } else {
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            Ok(false)
        } else {
            Err(error)
        }
    }
}

#[cfg(windows)]
fn try_lock_file(_file: &File) -> std::io::Result<bool> {
    Ok(true)
}

#[cfg(unix)]
fn unlock_file(file: &File) -> std::io::Result<()> {
    use std::os::fd::AsRawFd;
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(windows)]
fn unlock_file(_file: &File) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn probe_active_marker(path: &Path) -> std::io::Result<bool> {
    let marker = open_lock_file(path, false)?;
    let active = !try_lock_file(&marker)?;
    if !active {
        unlock_file(&marker)?;
    }
    Ok(active)
}

#[cfg(windows)]
fn probe_active_marker(path: &Path) -> std::io::Result<bool> {
    match OpenOptions::new().read(true).write(true).open(path) {
        Ok(_) => Ok(false),
        Err(error) if error.raw_os_error() == Some(32) => Ok(true),
        Err(error) => Err(error),
    }
}

fn cleanup_error(session_id: SessionId, message: &'static str) -> KrometrailError {
    download_error(
        ErrorCode::PersistenceFailed,
        session_id,
        message,
        "remove invalid stale entries from the private Krometrail download root and retry",
    )
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

fn state_cursor(state: &State) -> DownloadSequence {
    DownloadSequence::new(state.next_sequence.saturating_sub(1)).expect("seeded download cursor")
}

fn inventory(session_id: SessionId, state: &State) -> DownloadInventory {
    let cursor = state_cursor(state);
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

fn download_media_type(params: &Value) -> NonEmptyText {
    let media_type = params
        .get("mimeType")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(normalize_browser_media_type)
        .or_else(|| {
            params
                .get("suggestedFilename")
                .and_then(Value::as_str)
                .and_then(extension_media_type)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| "application/octet-stream".to_owned());
    NonEmptyText::new(media_type).expect("download media type is non-empty")
}

fn normalize_browser_media_type(media_type: &str) -> String {
    let essence = media_type
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match essence.as_str() {
        "text/html" | "application/xhtml+xml" => "text/plain".to_owned(),
        "image/svg+xml" => "application/octet-stream".to_owned(),
        "application/javascript"
        | "text/javascript"
        | "application/ecmascript"
        | "text/ecmascript"
        | "application/x-javascript" => "text/plain".to_owned(),
        _ => media_type.to_owned(),
    }
}

fn extension_media_type(filename: &str) -> Option<&'static str> {
    let extension = filename.rsplit_once('.')?.1;
    match extension.to_ascii_lowercase().as_str() {
        "txt" => Some("text/plain"),
        "json" => Some("application/json"),
        "csv" => Some("text/csv"),
        "md" => Some("text/markdown"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "pdf" => Some("application/pdf"),
        "zip" => Some("application/zip"),
        // Keep downloaded HTML inert when an MCP host renders a local resource.
        "html" | "htm" => Some("text/plain"),
        _ => None,
    }
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
        self.lease.lock().expect("download lease lock").take();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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

    struct CancelSignal(AtomicBool);
    impl CancellationSignal for CancelSignal {
        fn is_cancelled(&self) -> bool {
            self.0.load(Ordering::Acquire)
        }
        fn cancelled(&self) -> krometrail_core::PortFuture<'_, ()> {
            Box::pin(std::future::ready(()))
        }
    }
    struct Transport {
        calls: Mutex<Vec<(String, Value)>>,
    }

    struct ActivationTransport {
        activity: Mutex<Vec<String>>,
        fail_behavior_once: AtomicBool,
    }

    impl ActivationTransport {
        fn new(fail_behavior_once: bool) -> Self {
            Self {
                activity: Mutex::new(Vec::new()),
                fail_behavior_once: AtomicBool::new(fail_behavior_once),
            }
        }

        fn activity(&self) -> Vec<String> {
            self.activity.lock().unwrap().clone()
        }

        fn fail_next_behavior(&self) {
            self.fail_behavior_once.store(true, Ordering::Release);
        }
    }

    impl CdpTransport for ActivationTransport {
        fn send_raw(
            &self,
            _scope: &CommandScope,
            method: &str,
            _params: Value,
        ) -> TransportFuture<'_, std::result::Result<Value, TransportError>> {
            self.activity
                .lock()
                .unwrap()
                .push(format!("command:{method}"));
            let fail = method == "Browser.setDownloadBehavior"
                && self.fail_behavior_once.swap(false, Ordering::AcqRel);
            Box::pin(std::future::ready(if fail {
                Err(TransportError::CommandFailed)
            } else {
                Ok(json!({}))
            }))
        }

        fn subscribe_named(
            &self,
            _scope: &CommandScope,
            method: &str,
        ) -> TransportFuture<'_, std::result::Result<Box<dyn TransportEvents>, TransportError>>
        {
            self.activity
                .lock()
                .unwrap()
                .push(format!("subscribe:{method}"));
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

    fn control(base: PathBuf) -> Arc<ManagedDownloadControl> {
        ManagedDownloadControl::new(
            base,
            SessionId::from_uuid(uuid::Uuid::from_u128(200)),
            Arc::new(Ids(AtomicU64::new(0))),
            Arc::new(SubscriberRegistry::new(4)),
        )
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
        authority_with_subscribers(base, None)
    }

    fn authority_with_subscribers(
        base: &Path,
        subscribers: Option<Arc<SubscriberRegistry>>,
    ) -> Arc<ManagedDownloadAuthority> {
        let session_id = SessionId::from_uuid(uuid::Uuid::from_u128(100));
        let PreparedSessionRoot { root, lease } = prepare_session_root(base, session_id).unwrap();
        Arc::new(ManagedDownloadAuthority {
            session_id,
            root,
            ids: Arc::new(Ids(AtomicU64::new(0))),
            state: Mutex::new(State {
                accepting: true,
                next_sequence: INITIAL_NEXT_SEQUENCE,
                by_guid: BTreeMap::new(),
                overflow_rejection: None,
                transport_generation: 1,
            }),
            gate: tokio::sync::Mutex::new(()),
            changed: Notify::new(),
            subscribers,
            lease: Mutex::new(Some(lease)),
        })
    }

    #[test]
    fn download_media_type_prefers_browser_value_then_bounded_filename_mapping() {
        assert_eq!(
            download_media_type(&json!({"mimeType":"text/plain","suggestedFilename":"file.bin"}))
                .as_str(),
            "text/plain"
        );
        for (reported, expected) in [
            ("text/html", "text/plain"),
            ("application/xhtml+xml; charset=utf-8", "text/plain"),
            ("image/svg+xml", "application/octet-stream"),
            ("text/javascript", "text/plain"),
        ] {
            assert_eq!(
                download_media_type(&json!({
                    "mimeType": reported,
                    "suggestedFilename": "payload.bin"
                }))
                .as_str(),
                expected
            );
        }
        assert_eq!(
            download_media_type(&json!({
                "mimeType": "application/json; charset=utf-8",
                "suggestedFilename": "payload.bin"
            }))
            .as_str(),
            "application/json; charset=utf-8"
        );
        for (filename, expected) in [
            ("hello.txt", "text/plain"),
            ("data.JSON", "application/json"),
            ("table.csv", "text/csv"),
            ("notes.md", "text/markdown"),
            ("image.png", "image/png"),
            ("photo.jpeg", "image/jpeg"),
            ("archive.zip", "application/zip"),
            ("page.html", "text/plain"),
        ] {
            assert_eq!(
                download_media_type(&json!({"suggestedFilename": filename})).as_str(),
                expected
            );
        }
        assert_eq!(
            download_media_type(&json!({"suggestedFilename":"payload.bin"})).as_str(),
            "application/octet-stream"
        );
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
                &json!({"guid":"opaque-guid","url":"https://example.test/private?token=secret","mimeType":"text/html","suggestedFilename":"../report.txt"}),
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
        assert_eq!(read.media_type.as_str(), "text/plain");
        authority.shutdown(Some(&transport)).await.unwrap();
        authority.shutdown(Some(&transport)).await.unwrap();
        assert!(sibling.is_dir());
        assert!(!authority.root.exists());
    }

    #[tokio::test]
    async fn eager_activation_subscribes_before_enabling_and_seeds_the_cursor() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        let control = control(base.clone());
        assert_eq!(control.cursor(), None, "cursor precedes activation only");
        let transport = Arc::new(ActivationTransport::new(false));
        let transport_port: Arc<dyn CdpTransport> = transport.clone();

        control.activate(transport_port.clone()).await.unwrap();
        // Subscribe-before-enable: no begin/progress event can race the tracker.
        assert_eq!(
            transport.activity(),
            vec![
                "subscribe:Browser.downloadWillBegin",
                "subscribe:Browser.downloadProgress",
                "command:Browser.setDownloadBehavior",
            ]
        );
        assert!(base.join(control.session_id.to_string()).is_dir());

        // The cursor and inventory are available from activation with no
        // further transport activity: sequence 1 is the empty-inventory cursor.
        let inventory = control.list().unwrap();
        assert!(inventory.downloads.is_empty());
        assert_eq!(inventory.cursor.get(), 1);
        assert_eq!(control.cursor(), Some(inventory.cursor));
        assert!(control.begun_after(inventory.cursor).is_empty());
        assert_eq!(transport.activity().len(), 3);

        control.activate(transport_port).await.unwrap();
        assert_eq!(transport.activity().len(), 3, "activation runs once");
    }

    #[tokio::test]
    async fn activation_failure_degrades_without_failing_the_session() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        let control = control(base.clone());
        let transport = Arc::new(ActivationTransport::new(true));
        let transport_port: Arc<dyn CdpTransport> = transport.clone();

        let error = control.activate(transport_port).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::BrowserCompatibilityFailed);
        assert!(!base.join(control.session_id.to_string()).exists());

        // Explicit operations report the stored error; interaction facts
        // degrade through the absent cursor.
        let listed = control.list().unwrap_err();
        assert_eq!(listed.code, ErrorCode::BrowserCompatibilityFailed);
        assert_eq!(control.cursor(), None);
        assert!(
            control
                .begun_after(DownloadSequence::new(1).unwrap())
                .is_empty()
        );
    }

    #[tokio::test]
    async fn reconnect_failure_isolates_and_disables_only_download_control() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        let control = control(base.clone());
        let transport = Arc::new(ActivationTransport::new(false));
        let transport_port: Arc<dyn CdpTransport> = transport.clone();
        control.activate(transport_port.clone()).await.unwrap();

        transport.fail_next_behavior();
        let error = control.rebind(transport_port).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::BrowserCompatibilityFailed);
        assert!(!base.join(control.session_id.to_string()).exists());

        let retry = control.list().unwrap_err();
        assert_eq!(retry.code, ErrorCode::BrowserCompatibilityFailed);
        assert!(!base.join(control.session_id.to_string()).exists());
    }

    #[tokio::test]
    async fn begun_after_keys_on_begin_ordering_not_transition_sequences() {
        let temp = tempfile::tempdir().unwrap();
        let authority = authority(temp.path());
        let transport = Transport {
            calls: Mutex::new(Vec::new()),
        };
        // A pre-action download begins before the cursor capture.
        authority
            .begin(
                1,
                &transport,
                &json!({"guid":"pre-guid","url":"https://example.test/pre","suggestedFilename":"pre"}),
            )
            .await;
        let cursor = authority.cursor();

        // The pre-action download progresses (bumping its public sequence
        // past the cursor) while a post-cursor download begins.
        authority.transition("pre-guid", DownloadState::InProgress, Some(64), None);
        authority
            .begin(
                1,
                &transport,
                &json!({"guid":"post-guid","url":"https://example.test/post","suggestedFilename":"post"}),
            )
            .await;

        let facts = authority.begun_after(cursor);
        assert_eq!(facts.len(), 1, "pre-action progress is never attributed");
        assert_eq!(facts[0].state, DownloadState::InProgress);
        let post = authority
            .list()
            .downloads
            .into_iter()
            .find(|download| download.id == facts[0].download_id)
            .unwrap();
        assert!(post.sequence > cursor);
    }

    #[test]
    fn activation_scavenges_only_unlocked_canonical_session_directories() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        std::fs::create_dir_all(&base).unwrap();
        let stale = SessionId::from_uuid(uuid::Uuid::from_u128(301));
        let current = SessionId::from_uuid(uuid::Uuid::from_u128(302));
        let stale_root = base.join(stale.to_string());
        std::fs::create_dir(&stale_root).unwrap();
        std::fs::write(stale_root.join("private-bytes"), b"secret").unwrap();
        let unrelated = base.join("user-sibling");
        std::fs::create_dir(&unrelated).unwrap();

        let prepared = prepare_session_root(&base, current).unwrap();
        assert!(!stale_root.exists());
        assert!(unrelated.is_dir());
        assert!(prepared.root.is_dir());
        drop(prepared.lease);
        std::fs::remove_dir_all(prepared.root).unwrap();
    }

    #[test]
    fn scavenger_preserves_current_and_locked_active_sessions() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        let active_id = SessionId::from_uuid(uuid::Uuid::from_u128(311));
        let next_id = SessionId::from_uuid(uuid::Uuid::from_u128(312));
        let active = prepare_session_root(&base, active_id).unwrap();
        let next = prepare_session_root(&base, next_id).unwrap();
        assert!(active.root.is_dir());
        assert!(next.root.is_dir());

        scavenge_stale_session_roots(&base.canonicalize().unwrap(), next_id).unwrap();
        assert!(active.root.is_dir());
        assert!(next.root.is_dir());
        drop(active.lease);
        drop(next.lease);
    }

    #[cfg(unix)]
    #[test]
    fn scavenger_rejects_canonical_symlinks_without_following_or_disclosing_paths() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        std::fs::create_dir(&base).unwrap();
        let outside = temp.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("sentinel"), b"secret").unwrap();
        let candidate = SessionId::from_uuid(uuid::Uuid::from_u128(321));
        symlink(&outside, base.join(candidate.to_string())).unwrap();
        let current = SessionId::from_uuid(uuid::Uuid::from_u128(322));

        let error =
            scavenge_stale_session_roots(&base.canonicalize().unwrap(), current).unwrap_err();
        assert_eq!(error.code, ErrorCode::PersistenceFailed);
        assert!(outside.join("sentinel").is_file());
        let encoded = serde_json::to_string(&error).unwrap();
        assert!(!encoded.contains(temp.path().to_str().unwrap()));
    }

    #[test]
    fn scavenger_rejects_canonical_non_directories() {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().join("downloads");
        std::fs::create_dir(&base).unwrap();
        let candidate = SessionId::from_uuid(uuid::Uuid::from_u128(331));
        let path = base.join(candidate.to_string());
        std::fs::write(&path, b"unrelated").unwrap();
        let current = SessionId::from_uuid(uuid::Uuid::from_u128(332));

        let error =
            scavenge_stale_session_roots(&base.canonicalize().unwrap(), current).unwrap_err();
        assert_eq!(error.code, ErrorCode::PersistenceFailed);
        assert!(path.is_file());
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

    #[tokio::test]
    async fn lifecycle_events_are_privacy_safe_and_include_rejections() {
        use krometrail_core::BrowserSessionEvents;

        let temp = tempfile::tempdir().unwrap();
        let subscribers = Arc::new(SubscriberRegistry::new(4));
        let mut events: Box<dyn BrowserSessionEvents> = subscribers.subscribe();
        let authority = authority_with_subscribers(temp.path(), Some(subscribers));
        let transport = Transport {
            calls: Mutex::new(Vec::new()),
        };
        authority
            .begin(
                1,
                &transport,
                &json!({"guid":"event-guid","url":"https://example.test/private?token=secret","suggestedFilename":"secret-name.txt"}),
            )
            .await;
        let event = events.next().await.unwrap().unwrap();
        let encoded = serde_json::to_string(&event).unwrap();
        assert!(matches!(
            event,
            BrowserSessionEvent::DownloadStateChanged {
                state: DownloadState::InProgress,
                ..
            }
        ));
        assert!(!encoded.contains("secret-name"));
        assert!(!encoded.contains("private"));
        assert!(!encoded.contains("token"));
    }

    #[tokio::test]
    async fn overflow_is_bounded_visible_and_wait_returns_resource_limit() {
        let temp = tempfile::tempdir().unwrap();
        let authority = authority(temp.path());
        let transport = Transport {
            calls: Mutex::new(Vec::new()),
        };
        for index in 0..(MAX_MANAGED_DOWNLOADS + 2) {
            authority
                .begin(
                    1,
                    &transport,
                    &json!({"guid":format!("guid-{index}"),"url":"https://example.test/a","suggestedFilename":"a"}),
                )
                .await;
        }
        let inventory = authority.list();
        assert_eq!(inventory.downloads.len(), MAX_MANAGED_DOWNLOADS + 1);
        let rejected = inventory.downloads.last().unwrap();
        assert_eq!(rejected.state, DownloadState::Rejected);
        let error = authority
            .wait_with_cancellation(
                WaitForDownloadRequest {
                    after: DownloadSequence::new(1).unwrap(),
                    download_id: Some(rejected.id),
                    terminal: true,
                    timeout: 100,
                },
                None,
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::ResourceLimitExceeded);
        assert_eq!(
            transport
                .calls
                .lock()
                .unwrap()
                .iter()
                .filter(|(method, _)| method == "Browser.cancelDownload")
                .count(),
            2
        );
    }

    #[tokio::test]
    async fn wait_honors_caller_cancellation_without_a_download_change() {
        let temp = tempfile::tempdir().unwrap();
        let authority = authority(temp.path());
        let signal: Arc<dyn CancellationSignal> = Arc::new(CancelSignal(AtomicBool::new(true)));
        let error = authority
            .wait_with_cancellation(
                WaitForDownloadRequest {
                    after: DownloadSequence::new(1).unwrap(),
                    download_id: None,
                    terminal: true,
                    timeout: 1_000,
                },
                Some(signal),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
    }
}
