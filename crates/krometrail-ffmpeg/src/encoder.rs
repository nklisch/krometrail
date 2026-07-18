use std::sync::Arc;

use krometrail_core::VideoEncoderIdentity;

use crate::discovery::QualifiedExecutable;

pub struct QualifiedFfmpegEncoder {
    pub(crate) executable: QualifiedExecutable,
    pub(crate) identity: VideoEncoderIdentity,
    pub(crate) permit: Arc<tokio::sync::Semaphore>,
}

impl QualifiedFfmpegEncoder {
    pub(crate) fn new(executable: QualifiedExecutable, identity: VideoEncoderIdentity) -> Self {
        Self {
            executable,
            identity,
            permit: Arc::new(tokio::sync::Semaphore::new(1)),
        }
    }

    pub const fn identity(&self) -> &VideoEncoderIdentity {
        &self.identity
    }
}
