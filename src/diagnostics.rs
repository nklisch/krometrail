use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    filter::Targets, fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _,
};

const LOG_NAME: &str = "krometrail.log";
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const RETAINED_GENERATIONS: usize = 3;

pub(crate) struct DiagnosticRuntime {
    context: krometrail_mcp::DiagnosticContext,
    _guard: WorkerGuard,
}

impl DiagnosticRuntime {
    pub(crate) fn context(&self) -> krometrail_mcp::DiagnosticContext {
        self.context.clone()
    }
}

pub(crate) fn initialize(data_directory: &Path) -> io::Result<Option<DiagnosticRuntime>> {
    let directory = data_directory.join("diagnostics");
    fs::create_dir_all(&directory)?;
    make_directory_private(&directory)?;
    let directory = fs::canonicalize(directory)?;
    let log_path = directory.join(LOG_NAME);
    let writer = BoundedRotatingWriter::open(log_path.clone())?;
    let (writer, guard) = tracing_appender::non_blocking(writer);
    let filter = Targets::new()
        .with_target("krometrail", Level::INFO)
        .with_target("krometrail_cdp", Level::INFO)
        .with_target("krometrail_ffmpeg", Level::INFO)
        .with_target("krometrail_store", Level::INFO)
        .with_target("krometrail_mcp", Level::INFO);
    let subscriber = tracing_subscriber::registry().with(filter).with(
        fmt::layer()
            .json()
            .with_current_span(true)
            .with_span_list(false)
            .with_writer(writer),
    );
    if subscriber.try_init().is_err() {
        return Ok(None);
    }
    tracing::info!(event = "diagnostics.started", "diagnostics.started");
    Ok(Some(DiagnosticRuntime {
        context: krometrail_mcp::DiagnosticContext::new(Some(log_path)),
        _guard: guard,
    }))
}

struct BoundedRotatingWriter {
    path: PathBuf,
    file: Option<File>,
    length: u64,
}

impl BoundedRotatingWriter {
    fn open(path: PathBuf) -> io::Result<Self> {
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        make_file_private(&file)?;
        let length = file.metadata()?.len();
        Ok(Self {
            path,
            file: Some(file),
            length,
        })
    }

    fn rotate(&mut self) -> io::Result<()> {
        self.file.take();
        for generation in (1..=RETAINED_GENERATIONS).rev() {
            let source = generation_path(&self.path, generation - 1);
            let destination = generation_path(&self.path, generation);
            if source.exists() {
                let _ = fs::remove_file(&destination);
                fs::rename(source, destination)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&self.path)?;
        make_file_private(&file)?;
        self.file = Some(file);
        self.length = 0;
        Ok(())
    }
}

#[cfg(unix)]
fn make_directory_private(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn make_directory_private(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn make_file_private(file: &File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn make_file_private(_file: &File) -> io::Result<()> {
    Ok(())
}

impl Write for BoundedRotatingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.length > 0 && self.length.saturating_add(bytes.len() as u64) > MAX_FILE_BYTES {
            self.rotate()?;
        }
        let bytes = if bytes.len() as u64 > MAX_FILE_BYTES {
            &bytes[..MAX_FILE_BYTES as usize]
        } else {
            bytes
        };
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic log rotation failed"))?;
        let written = file.write(bytes)?;
        self.length = self.length.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("diagnostic log rotation failed"))?
            .flush()
    }
}

fn generation_path(path: &Path, generation: usize) -> PathBuf {
    if generation == 0 {
        path.to_path_buf()
    } else {
        path.with_extension(format!("log.{generation}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_under_a_fixed_aggregate_bound() {
        let root = std::env::temp_dir().join(format!("krometrail-log-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join(LOG_NAME);
        let mut writer = BoundedRotatingWriter::open(path.clone()).unwrap();
        let block = vec![b'x'; MAX_FILE_BYTES as usize];
        for _ in 0..6 {
            writer.write_all(&block).unwrap();
        }
        writer.flush().unwrap();
        let aggregate = fs::read_dir(&root)
            .unwrap()
            .map(|entry| entry.unwrap().metadata().unwrap().len())
            .sum::<u64>();
        assert!(aggregate <= MAX_FILE_BYTES * (RETAINED_GENERATIONS as u64 + 1));
        assert!(path.exists());
        assert!(generation_path(&path, RETAINED_GENERATIONS).exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn log_path_is_rooted_in_the_configured_data_directory() {
        let data = Path::new("/tmp/configured-krometrail-data");
        assert_eq!(
            data.join("diagnostics").join(LOG_NAME),
            Path::new("/tmp/configured-krometrail-data/diagnostics/krometrail.log")
        );
    }

    #[test]
    fn unavailable_destination_returns_an_error_for_best_effort_fallback() {
        let occupied =
            std::env::temp_dir().join(format!("krometrail-log-file-{}", uuid::Uuid::new_v4()));
        fs::write(&occupied, b"occupied").unwrap();
        assert!(BoundedRotatingWriter::open(occupied.join(LOG_NAME)).is_err());
        fs::remove_file(occupied).unwrap();
    }
}
