use std::{
    collections::HashSet,
    ffi::OsString,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

use crate::{
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

pub(crate) fn discover_candidates(
    options: &FfmpegDiscoveryOptions,
) -> Result<Vec<PathBuf>, AdapterFailure> {
    if let Some(explicit) = &options.explicit_executable {
        if !explicit.is_absolute() {
            return Err(invalid_candidate());
        }
        return canonical_candidate(explicit)
            .map(|candidate| vec![candidate])
            .ok_or_else(invalid_candidate);
    }

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    if let Some(search_path) = &options.search_path {
        for directory in std::env::split_paths(search_path) {
            if candidates.len() == MAX_FFMPEG_DISCOVERY_CANDIDATES {
                break;
            }
            let path = directory.join(executable_name());
            if let Some(candidate) = canonical_candidate(&path)
                && seen.insert(candidate.clone())
            {
                candidates.push(candidate);
            }
        }
    }
    for default in platform_defaults() {
        if candidates.len() == MAX_FFMPEG_DISCOVERY_CANDIDATES {
            break;
        }
        if let Some(candidate) = canonical_candidate(Path::new(default))
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
    pub(crate) fn load(path: PathBuf) -> Result<Self, AdapterFailure> {
        let before = executable_stamp(&path)?;
        let executable_sha256 = hash_executable(&path, before.len)?;
        let after = executable_stamp(&path)?;
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

    pub(crate) fn validate_unchanged(&self) -> Result<(), AdapterFailure> {
        let current = executable_stamp(&self.path).map_err(|_| {
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

fn canonical_candidate(path: &Path) -> Option<PathBuf> {
    let canonical = path.canonicalize().ok()?;
    let metadata = canonical.metadata().ok()?;
    (metadata.is_file() && is_executable(&metadata)).then_some(canonical)
}

fn executable_stamp(path: &Path) -> Result<ExecutableStamp, AdapterFailure> {
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

fn hash_executable(path: &Path, expected_len: u64) -> Result<[u8; 32], AdapterFailure> {
    let mut file = File::open(path).map_err(|_| invalid_candidate())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
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

#[cfg(windows)]
fn is_executable(_metadata: &std::fs::Metadata) -> bool {
    true
}

#[cfg(not(any(unix, windows)))]
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

#[cfg(windows)]
fn stamp_from_metadata(metadata: &std::fs::Metadata) -> Result<ExecutableStamp, AdapterFailure> {
    use std::os::windows::fs::MetadataExt;
    Ok(ExecutableStamp {
        len: metadata.file_size(),
        identity_a: u64::from(metadata.file_attributes()),
        identity_b: metadata.creation_time(),
        modified_a: metadata.last_write_time() as i64,
        modified_b: 0,
    })
}

#[cfg(not(any(unix, windows)))]
fn stamp_from_metadata(_metadata: &std::fs::Metadata) -> Result<ExecutableStamp, AdapterFailure> {
    Err(invalid_candidate())
}

#[cfg(windows)]
const fn executable_name() -> &'static str {
    "ffmpeg.exe"
}

#[cfg(not(windows))]
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

#[cfg(windows)]
const fn platform_defaults() -> &'static [&'static str] {
    &[r"C:\ffmpeg\bin\ffmpeg.exe"]
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
const fn platform_defaults() -> &'static [&'static str] {
    &[]
}

fn invalid_candidate() -> AdapterFailure {
    AdapterFailure::new(
        AdapterFailureStage::ExecutableIdentity,
        AdapterFailureKind::InvalidCandidate,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    mod support {
        include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/support/mod.rs"));
    }

    #[test]
    fn explicit_relative_or_missing_candidate_fails_without_fallback() {
        assert!(
            discover_candidates(&FfmpegDiscoveryOptions::with_explicit_executable(
                PathBuf::from("ffmpeg")
            ))
            .is_err()
        );
        assert!(
            discover_candidates(&FfmpegDiscoveryOptions::with_explicit_executable(
                std::env::temp_dir().join("krometrail-definitely-missing-ffmpeg")
            ))
            .is_err()
        );
    }

    #[test]
    fn path_discovery_deduplicates_canonical_candidates() {
        let fixture = support::FixtureExecutable::new("valid");
        let directory = fixture.path().parent().unwrap();
        let search = std::env::join_paths([directory, directory, directory]).unwrap();
        let candidates =
            discover_candidates(&FfmpegDiscoveryOptions::with_search_path(search)).unwrap();
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

    #[test]
    fn executable_identity_is_bounded_and_detects_replacement() {
        let fixture = support::FixtureExecutable::new("valid");
        let executable = QualifiedExecutable::load(fixture.path().canonicalize().unwrap()).unwrap();
        assert_ne!(executable.executable_sha256(), &[0; 32]);
        std::fs::write(fixture.path(), b"changed executable").unwrap();
        assert_eq!(
            executable.validate_unchanged().unwrap_err().kind,
            AdapterFailureKind::ChangedCandidate
        );
    }
}
