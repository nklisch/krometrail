use std::{
    collections::HashSet,
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{
    control::OperationControl,
    error::{AdapterFailure, AdapterFailureKind, AdapterFailureStage},
    policy::MAX_FFMPEG_DISCOVERY_CANDIDATES,
};

const MAX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct FfmpegDiscoveryOptions {
    explicit_executable: Option<PathBuf>,
    search_path: Option<OsString>,
}

impl FfmpegDiscoveryOptions {
    pub fn from_process_environment() -> Self {
        Self {
            explicit_executable: std::env::var_os("KROMETRAIL_FFMPEG_PATH").map(PathBuf::from),
            search_path: std::env::var_os("PATH"),
        }
    }

    pub fn with_explicit_executable(path: PathBuf) -> Self {
        Self {
            explicit_executable: Some(path),
            search_path: None,
        }
    }

    pub fn with_search_path(search_path: OsString) -> Self {
        Self {
            explicit_executable: None,
            search_path: Some(search_path),
        }
    }

    pub(crate) const fn is_explicit(&self) -> bool {
        self.explicit_executable.is_some()
    }
}

pub(crate) async fn discover_candidates(
    options: &FfmpegDiscoveryOptions,
    control: &OperationControl,
) -> Result<Vec<PathBuf>, AdapterFailure> {
    let options = options.clone();
    control
        .run_blocking(AdapterFailureStage::ExecutableIdentity, move |control| {
            discover_candidates_blocking(&options, &control)
        })
        .await
}

fn discover_candidates_blocking(
    options: &FfmpegDiscoveryOptions,
    control: &OperationControl,
) -> Result<Vec<PathBuf>, AdapterFailure> {
    control.check(AdapterFailureStage::ExecutableIdentity)?;
    if let Some(explicit) = &options.explicit_executable {
        if !explicit.is_absolute() {
            return Err(invalid_candidate());
        }
        return canonical_candidate(explicit, control)?
            .map(|candidate| vec![candidate])
            .ok_or_else(invalid_candidate);
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let mut probes = 0_usize;
    if let Some(search_path) = &options.search_path {
        for directory in std::env::split_paths(search_path) {
            control.check(AdapterFailureStage::ExecutableIdentity)?;
            if probes == MAX_FFMPEG_DISCOVERY_CANDIDATES {
                break;
            }
            probes += 1;
            let path = directory.join(executable_name());
            if let Some(candidate) = canonical_candidate(&path, control)?
                && seen.insert(candidate.clone())
            {
                candidates.push(candidate);
            }
        }
    }
    for default in platform_defaults() {
        control.check(AdapterFailureStage::ExecutableIdentity)?;
        if probes == MAX_FFMPEG_DISCOVERY_CANDIDATES {
            break;
        }
        probes += 1;
        if let Some(candidate) = canonical_candidate(Path::new(default), control)?
            && seen.insert(candidate.clone())
        {
            candidates.push(candidate);
        }
    }
    Ok(candidates)
}

#[derive(Clone)]
pub(crate) struct QualifiedExecutable {
    path: PathBuf,
    stamp: ExecutableStamp,
    executable_sha256: [u8; 32],
}

impl QualifiedExecutable {
    pub(crate) async fn load(
        path: PathBuf,
        control: &OperationControl,
    ) -> Result<Self, AdapterFailure> {
        control
            .run_blocking(AdapterFailureStage::ExecutableIdentity, move |control| {
                Self::load_blocking(path, &control)
            })
            .await
    }

    fn load_blocking(path: PathBuf, control: &OperationControl) -> Result<Self, AdapterFailure> {
        control.check(AdapterFailureStage::ExecutableIdentity)?;
        let before = executable_stamp(&path, control)?;
        let executable_sha256 = hash_executable(&path, before.len, control)?;
        let after = executable_stamp(&path, control)?;
        if before != after {
            return Err(AdapterFailure::new(
                AdapterFailureStage::ExecutableIdentity,
                AdapterFailureKind::ChangedCandidate,
            ));
        }
        Ok(Self {
            path,
            stamp: after,
            executable_sha256,
        })
    }

    pub(crate) async fn validate_unchanged(
        &self,
        control: &OperationControl,
    ) -> Result<(), AdapterFailure> {
        let executable = self.clone();
        control
            .run_blocking(AdapterFailureStage::ExecutableIdentity, move |control| {
                executable.validate_unchanged_blocking(&control)
            })
            .await
    }

    fn validate_unchanged_blocking(
        &self,
        control: &OperationControl,
    ) -> Result<(), AdapterFailure> {
        let current = executable_stamp(&self.path, control).map_err(|failure| {
            if matches!(
                failure.kind,
                AdapterFailureKind::Cancelled | AdapterFailureKind::Deadline
            ) {
                return failure;
            }
            AdapterFailure::new(
                AdapterFailureStage::ExecutableIdentity,
                AdapterFailureKind::ChangedCandidate,
            )
        })?;
        if current != self.stamp {
            return Err(AdapterFailure::new(
                AdapterFailureStage::ExecutableIdentity,
                AdapterFailureKind::ChangedCandidate,
            ));
        }
        Ok(())
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) const fn executable_sha256(&self) -> &[u8; 32] {
        &self.executable_sha256
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ExecutableStamp {
    len: u64,
    identity_a: u64,
    identity_b: u64,
    modified_a: i64,
    modified_b: i64,
}

fn canonical_candidate(
    path: &Path,
    control: &OperationControl,
) -> Result<Option<PathBuf>, AdapterFailure> {
    control.check(AdapterFailureStage::ExecutableIdentity)?;
    let Ok(canonical) = path.canonicalize() else {
        return Ok(None);
    };
    control.check(AdapterFailureStage::ExecutableIdentity)?;
    let Ok(metadata) = canonical.metadata() else {
        return Ok(None);
    };
    Ok((metadata.is_file() && is_executable(&metadata)).then_some(canonical))
}

fn executable_stamp(
    path: &Path,
    control: &OperationControl,
) -> Result<ExecutableStamp, AdapterFailure> {
    control.check(AdapterFailureStage::ExecutableIdentity)?;
    let metadata = path.metadata().map_err(|_| invalid_candidate())?;
    if !metadata.is_file()
        || !is_executable(&metadata)
        || metadata.len() == 0
        || metadata.len() > MAX_EXECUTABLE_BYTES
    {
        return Err(invalid_candidate());
    }
    stamp_from_metadata(&metadata)
}

fn hash_executable(
    path: &Path,
    expected_len: u64,
    control: &OperationControl,
) -> Result<[u8; 32], AdapterFailure> {
    let mut file = File::open(path).map_err(|_| invalid_candidate())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        control.check(AdapterFailureStage::ExecutableIdentity)?;
        let read = file.read(&mut buffer).map_err(|_| invalid_candidate())?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .filter(|total| *total <= MAX_EXECUTABLE_BYTES)
            .ok_or_else(invalid_candidate)?;
        digest.update(&buffer[..read]);
    }
    if total != expected_len {
        return Err(AdapterFailure::new(
            AdapterFailureStage::ExecutableIdentity,
            AdapterFailureKind::ChangedCandidate,
        ));
    }
    Ok(digest.finalize().into())
}

#[cfg(unix)]
fn is_executable(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn stamp_from_metadata(metadata: &std::fs::Metadata) -> Result<ExecutableStamp, AdapterFailure> {
    use std::os::unix::fs::MetadataExt;
    Ok(ExecutableStamp {
        len: metadata.len(),
        identity_a: metadata.dev(),
        identity_b: metadata.ino(),
        modified_a: metadata.mtime(),
        modified_b: metadata.mtime_nsec(),
    })
}

#[cfg(not(unix))]
fn stamp_from_metadata(_metadata: &std::fs::Metadata) -> Result<ExecutableStamp, AdapterFailure> {
    Err(invalid_candidate())
}

const fn executable_name() -> &'static str {
    "ffmpeg"
}

#[cfg(target_os = "macos")]
const fn platform_defaults() -> &'static [&'static str] {
    &[
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ]
}

#[cfg(target_os = "linux")]
const fn platform_defaults() -> &'static [&'static str] {
    &[
        "/usr/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/snap/bin/ffmpeg",
    ]
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const fn platform_defaults() -> &'static [&'static str] {
    &[]
}

fn invalid_candidate() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::ExecutableIdentity,
        AdapterFailureKind::InvalidCandidate,
    )
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use krometrail_core::{CancellationSignal, PortFuture};
    use std::{sync::Arc, time::Duration};

    #[allow(dead_code)]
    mod support {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/mod.rs"));
    }

    struct NeverCancelled;

    impl CancellationSignal for NeverCancelled {
        fn is_cancelled(&self) -> bool {
            false
        }

        fn cancelled(&self) -> PortFuture<'_, ()> {
            Box::pin(std::future::pending())
        }
    }

    fn control() -> OperationControl {
        OperationControl::new(
            Arc::new(NeverCancelled),
            std::time::Instant::now() + Duration::from_secs(5),
        )
    }

    #[tokio::test]
    async fn explicit_relative_or_missing_candidate_fails_without_fallback() {
        assert!(
            discover_candidates(
                &FfmpegDiscoveryOptions::with_explicit_executable(PathBuf::from("ffmpeg")),
                &control()
            )
            .await
            .is_err()
        );
        assert!(
            discover_candidates(
                &FfmpegDiscoveryOptions::with_explicit_executable(
                    std::env::temp_dir().join("krometrail-definitely-missing-ffmpeg")
                ),
                &control()
            )
            .await
            .is_err()
        );
    }

    #[tokio::test]
    async fn path_discovery_deduplicates_canonical_candidates() {
        let fixture = support::FixtureExecutable::new("valid");
        let directory = fixture.path().parent().unwrap();
        let search = std::env::join_paths([directory, directory, directory]).unwrap();
        let candidates = discover_candidates(
            &FfmpegDiscoveryOptions::with_search_path(search),
            &control(),
        )
        .await
        .unwrap();
        assert_eq!(
            candidates.first().unwrap(),
            &fixture.path().canonicalize().unwrap()
        );
        assert_eq!(
            candidates
                .iter()
                .filter(|candidate| *candidate == &fixture.path().canonicalize().unwrap())
                .count(),
            1
        );
        assert!(candidates.len() <= MAX_FFMPEG_DISCOVERY_CANDIDATES);
    }

    #[tokio::test]
    async fn executable_identity_is_bounded_and_detects_replacement() {
        let fixture = support::FixtureExecutable::new("valid");
        let executable =
            QualifiedExecutable::load(fixture.path().canonicalize().unwrap(), &control())
                .await
                .unwrap();
        assert_ne!(executable.executable_sha256(), &[0; 32]);
        std::fs::write(fixture.path(), b"changed executable").unwrap();
        assert_eq!(
            executable
                .validate_unchanged(&control())
                .await
                .unwrap_err()
                .kind,
            AdapterFailureKind::ChangedCandidate
        );
    }
}
