use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, SyncSender},
};

use tokio::sync::oneshot;

use crate::{
    index::deletion::{DeletionBatch, DeletionObjectKind},
    permissions, persistence_error,
};

const REMOVAL_QUEUE_CAPACITY: usize = 16;

pub(crate) struct RemovalWorker {
    commands: SyncSender<Command>,
}

enum Command {
    Stage {
        batch: DeletionBatch,
        reply: oneshot::Sender<krometrail_core::Result<()>>,
    },
    Finalize {
        batch: DeletionBatch,
        reply: oneshot::Sender<krometrail_core::Result<()>>,
    },
    StageBlocking {
        batch: DeletionBatch,
        reply: std::sync::mpsc::Sender<krometrail_core::Result<()>>,
    },
    FinalizeBlocking {
        batch: DeletionBatch,
        reply: std::sync::mpsc::Sender<krometrail_core::Result<()>>,
    },
}

struct WorkerState {
    segments_directory: PathBuf,
    artifacts_directory: PathBuf,
    trash_directory: PathBuf,
}

impl RemovalWorker {
    pub(crate) fn open(
        data_directory: PathBuf,
        segments_directory: PathBuf,
    ) -> krometrail_core::Result<Self> {
        let artifacts_directory = data_directory.join("artifacts");
        let trash_directory = data_directory.join(".trash");
        permissions::ensure_private_directory(&artifacts_directory)
            .map_err(|error| io_error("create the artifact directory", error))?;
        permissions::ensure_private_directory(&trash_directory)
            .map_err(|error| io_error("create the deletion staging directory", error))?;
        let (commands, receiver) = mpsc::sync_channel(REMOVAL_QUEUE_CAPACITY);
        let state = WorkerState {
            segments_directory,
            artifacts_directory,
            trash_directory,
        };
        std::thread::Builder::new()
            .name("krometrail-retention-removal".to_owned())
            .spawn(move || state.run(receiver))
            .map_err(|error| io_error("start the retention removal worker", error))?;
        Ok(Self { commands })
    }

    pub(crate) fn stage_blocking(&self, batch: DeletionBatch) -> krometrail_core::Result<()> {
        let (reply, response) = mpsc::channel();
        self.commands
            .try_send(Command::StageBlocking { batch, reply })
            .map_err(|_| persistence_error("retention removal queue is unavailable"))?;
        response
            .recv()
            .map_err(|_| persistence_error("retention removal worker stopped"))?
    }

    pub(crate) fn finalize_blocking(&self, batch: DeletionBatch) -> krometrail_core::Result<()> {
        let (reply, response) = mpsc::channel();
        self.commands
            .try_send(Command::FinalizeBlocking { batch, reply })
            .map_err(|_| persistence_error("retention removal queue is unavailable"))?;
        response
            .recv()
            .map_err(|_| persistence_error("retention removal worker stopped"))?
    }

    pub(crate) async fn stage(&self, batch: DeletionBatch) -> krometrail_core::Result<()> {
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Command::Stage { batch, reply })
            .map_err(|_| persistence_error("retention removal queue is unavailable"))?;
        response
            .await
            .map_err(|_| persistence_error("retention removal worker stopped"))?
    }

    pub(crate) async fn finalize(&self, batch: DeletionBatch) -> krometrail_core::Result<()> {
        let (reply, response) = oneshot::channel();
        self.commands
            .try_send(Command::Finalize { batch, reply })
            .map_err(|_| persistence_error("retention removal queue is unavailable"))?;
        response
            .await
            .map_err(|_| persistence_error("retention removal worker stopped"))?
    }
}

impl WorkerState {
    fn run(self, receiver: mpsc::Receiver<Command>) {
        for command in receiver {
            match command {
                Command::Stage { batch, reply } => {
                    let _ = reply.send(self.stage(&batch));
                }
                Command::Finalize { batch, reply } => {
                    let _ = reply.send(self.finalize(&batch));
                }
                Command::StageBlocking { batch, reply } => {
                    let _ = reply.send(self.stage(&batch));
                }
                Command::FinalizeBlocking { batch, reply } => {
                    let _ = reply.send(self.finalize(&batch));
                }
            }
        }
    }

    fn stage(&self, batch: &DeletionBatch) -> krometrail_core::Result<()> {
        let staging = self.staging_directory(batch);
        permissions::ensure_private_directory(&staging)
            .map_err(|error| io_error("create a deletion batch staging directory", error))?;
        for (position, object) in batch.objects.iter().enumerate() {
            let source = match object.kind {
                DeletionObjectKind::Segment(_) => {
                    self.segments_directory.join(&object.relative_path)
                }
                DeletionObjectKind::Artifact(_) => {
                    self.artifacts_directory.join(&object.relative_path)
                }
            };
            let staged = staged_path(&staging, position, &object.relative_path);
            match (source.exists(), staged.exists()) {
                (true, false) => fs::rename(&source, &staged)
                    .map_err(|error| io_error("stage retained data for deletion", error))?,
                (false, true) | (false, false) => {}
                (true, true) => {
                    return Err(persistence_error(
                        "deletion object exists in both live and staged storage",
                    ));
                }
            }
        }
        sync_directory(&self.segments_directory).map_err(|error| {
            io_error("sync the segment directory after deletion staging", error)
        })?;
        sync_directory(&self.artifacts_directory).map_err(|error| {
            io_error("sync the artifact directory after deletion staging", error)
        })?;
        sync_directory(&staging)
            .map_err(|error| io_error("sync a deletion batch staging directory", error))?;
        sync_directory(&self.trash_directory)
            .map_err(|error| io_error("sync the deletion staging root", error))
    }

    fn finalize(&self, batch: &DeletionBatch) -> krometrail_core::Result<()> {
        let staging = self.staging_directory(batch);
        for (position, object) in batch.objects.iter().enumerate() {
            let staged = staged_path(&staging, position, &object.relative_path);
            match fs::remove_file(&staged) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(io_error("unlink staged retained data", error)),
            }
        }
        if staging.exists() {
            sync_directory(&staging)
                .map_err(|error| io_error("sync a finalized deletion batch", error))?;
            fs::remove_dir(&staging)
                .map_err(|error| io_error("remove a deletion batch directory", error))?;
        }
        sync_directory(&self.trash_directory)
            .map_err(|error| io_error("sync the finalized deletion staging root", error))
    }

    fn staging_directory(&self, batch: &DeletionBatch) -> PathBuf {
        self.trash_directory.join(batch.batch_id.to_string())
    }
}

fn staged_path(directory: &Path, position: usize, relative_path: &str) -> PathBuf {
    directory.join(format!("{position:08}-{relative_path}"))
}

#[cfg(unix)]
fn sync_directory(directory: &Path) -> std::io::Result<()> {
    std::fs::File::open(directory)?.sync_all()
}

#[cfg(not(unix))]
fn sync_directory(_directory: &Path) -> std::io::Result<()> {
    Ok(())
}

fn io_error(action: &str, error: std::io::Error) -> krometrail_core::KrometrailError {
    persistence_error(format!("could not {action}: {}", error.kind()))
}
