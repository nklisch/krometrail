use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
};

use krometrail_core::{ArtifactId, CancellationSignal, ErrorCode, KrometrailError, NonEmptyText};
use tokio::sync::oneshot;

use crate::persistence_error;

const FILE_QUEUE_CAPACITY: usize = 8;

pub(crate) struct ArtifactFiles {
    commands: SyncSender<Command>,
    directory: PathBuf,
}

enum Command {
    Publish {
        artifact_id: ArtifactId,
        bytes: Arc<[u8]>,
        cancellation: Arc<AtomicBool>,
        external_cancellation: Option<Arc<dyn CancellationSignal>>,
        fail_after: Option<PublicationPhase>,
        reply: oneshot::Sender<krometrail_core::Result<()>>,
    },
    Read {
        relative_path: String,
        reply: oneshot::Sender<krometrail_core::Result<Arc<[u8]>>>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PublicationPhase {
    TempSync,
    Rename,
    DirectorySync,
}

impl ArtifactFiles {
    pub(crate) fn open(directory: PathBuf) -> krometrail_core::Result<Self> {
        fs::create_dir_all(&directory)
            .map_err(|error| io_error("create the artifact directory", error))?;
        let (commands, receiver) = mpsc::sync_channel(FILE_QUEUE_CAPACITY);
        let worker_directory = directory.clone();
        std::thread::Builder::new()
            .name("krometrail-artifact-files".to_owned())
            .spawn(move || run(worker_directory, receiver))
            .map_err(|error| io_error("start the artifact file worker", error))?;
        Ok(Self {
            commands,
            directory,
        })
    }

    pub(crate) async fn publish(
        &self,
        artifact_id: ArtifactId,
        bytes: Arc<[u8]>,
        cancellation: Arc<AtomicBool>,
        external_cancellation: Option<Arc<dyn CancellationSignal>>,
    ) -> krometrail_core::Result<()> {
        self.publish_with_failpoint(
            artifact_id,
            bytes,
            cancellation,
            external_cancellation,
            None,
        )
        .await
    }

    async fn publish_with_failpoint(
        &self,
        artifact_id: ArtifactId,
        bytes: Arc<[u8]>,
        cancellation: Arc<AtomicBool>,
        external_cancellation: Option<Arc<dyn CancellationSignal>>,
        fail_after: Option<PublicationPhase>,
    ) -> krometrail_core::Result<()> {
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Command::Publish {
                artifact_id,
                bytes,
                cancellation,
                external_cancellation,
                fail_after,
                reply,
            })
            .map_err(|_| persistence_error("artifact file queue is unavailable"))?;
        response
            .await
            .map_err(|_| persistence_error("artifact file worker stopped"))?
    }

    pub(crate) async fn read(&self, relative_path: String) -> krometrail_core::Result<Arc<[u8]>> {
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Command::Read {
                relative_path,
                reply,
            })
            .map_err(|_| persistence_error("artifact file queue is unavailable"))?;
        response
            .await
            .map_err(|_| persistence_error("artifact file worker stopped"))?
    }

    pub(crate) fn directory(&self) -> &Path {
        &self.directory
    }

    pub(crate) fn final_path(&self, artifact_id: ArtifactId) -> PathBuf {
        self.directory.join(format!("{artifact_id}.png"))
    }

    pub(crate) fn temp_path(&self, artifact_id: ArtifactId) -> PathBuf {
        self.directory.join(format!("{artifact_id}.tmp"))
    }
}

fn run(directory: PathBuf, receiver: mpsc::Receiver<Command>) {
    for command in receiver {
        match command {
            Command::Publish {
                artifact_id,
                bytes,
                cancellation,
                external_cancellation,
                fail_after,
                reply,
            } => {
                let _ = reply.send(publish_file(
                    &directory,
                    artifact_id,
                    &bytes,
                    &cancellation,
                    external_cancellation.as_deref(),
                    fail_after,
                ));
            }
            Command::Read {
                relative_path,
                reply,
            } => {
                let _ = reply.send(read_file(&directory, &relative_path));
            }
        }
    }
}

fn publish_file(
    directory: &Path,
    artifact_id: ArtifactId,
    bytes: &[u8],
    cancellation: &AtomicBool,
    external_cancellation: Option<&dyn CancellationSignal>,
    fail_after: Option<PublicationPhase>,
) -> krometrail_core::Result<()> {
    if is_cancelled(cancellation, external_cancellation) {
        return Err(cancelled_error());
    }
    let temp = directory.join(format!("{artifact_id}.tmp"));
    let final_path = directory.join(format!("{artifact_id}.png"));
    match fs::remove_file(&temp) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("remove a stale artifact temporary file", error)),
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)
        .map_err(|error| io_error("create an artifact temporary file", error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("write artifact bytes", error))?;
    file.sync_all()
        .map_err(|error| io_error("sync artifact bytes", error))?;
    if fail_after == Some(PublicationPhase::TempSync) {
        return Err(injected_error());
    }
    if is_cancelled(cancellation, external_cancellation) {
        drop(file);
        let _ = fs::remove_file(&temp);
        return Err(cancelled_error());
    }
    drop(file);
    fs::rename(&temp, &final_path)
        .map_err(|error| io_error("rename artifact publication", error))?;
    if fail_after == Some(PublicationPhase::Rename) {
        return Err(injected_error());
    }
    sync_directory(directory).map_err(|error| io_error("sync the artifact directory", error))?;
    if fail_after == Some(PublicationPhase::DirectorySync) {
        return Err(injected_error());
    }
    Ok(())
}

fn is_cancelled(session: &AtomicBool, external: Option<&dyn CancellationSignal>) -> bool {
    session.load(Ordering::Acquire) || external.is_some_and(CancellationSignal::is_cancelled)
}

fn read_file(directory: &Path, relative_path: &str) -> krometrail_core::Result<Arc<[u8]>> {
    // Index decoding already rejects separators. Keep the worker defensive because it is the
    // last boundary before filesystem access.
    if relative_path.is_empty() || relative_path.contains(['/', '\\']) {
        return Err(persistence_error("artifact path is invalid"));
    }
    fs::read(directory.join(relative_path))
        .map(Arc::<[u8]>::from)
        .map_err(|error| io_error("read artifact bytes", error))
}

#[cfg(unix)]
pub(crate) fn sync_directory(directory: &Path) -> std::io::Result<()> {
    fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
pub(crate) fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}

fn cancelled_error() -> KrometrailError {
    KrometrailError::new(
        ErrorCode::Cancelled,
        NonEmptyText::new("artifact publication was cancelled")
            .expect("static cancellation error is non-empty"),
    )
}

fn injected_error() -> KrometrailError {
    persistence_error("injected artifact publication failure")
}

fn io_error(action: &str, error: std::io::Error) -> KrometrailError {
    persistence_error(format!("could not {action}: {}", error.kind()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn failpoints_leave_only_explicit_recoverable_file_states() {
        for phase in [
            PublicationPhase::TempSync,
            PublicationPhase::Rename,
            PublicationPhase::DirectorySync,
        ] {
            let directory = TempDir::new().unwrap();
            let files = ArtifactFiles::open(directory.path().to_path_buf()).unwrap();
            let id = ArtifactId::from_uuid(uuid::Uuid::from_u128(phase as u128 + 1));
            assert!(
                files
                    .publish_with_failpoint(
                        id,
                        Arc::from(&b"png"[..]),
                        Arc::new(AtomicBool::new(false)),
                        None,
                        Some(phase)
                    )
                    .await
                    .is_err()
            );
            match phase {
                PublicationPhase::TempSync => {
                    assert!(files.temp_path(id).exists());
                    assert!(!files.final_path(id).exists());
                }
                PublicationPhase::Rename | PublicationPhase::DirectorySync => {
                    assert!(!files.temp_path(id).exists());
                    assert!(files.final_path(id).exists());
                }
            }
        }
    }
}
