use serde::{Deserialize, Serialize};

stable_registry! {
    /// Stable machine-readable classification for contract validation failures.
    pub enum ErrorCode {
        InvalidDimensions => "invalid_dimensions",
        PixelLengthMismatch => "pixel_length_mismatch",
        EmptySequence => "empty_sequence",
        DuplicateIdentifier => "duplicate_identifier",
        OutOfOrder => "out_of_order",
        IncompatibleFrame => "incompatible_frame",
        AnnotationOutOfRange => "annotation_out_of_range",
        InvalidRegion => "invalid_region",
        InvalidMask => "invalid_mask",
        InvalidScale => "invalid_scale",
        EmptyAnalysisDomain => "empty_analysis_domain",
        ResourceLimitExceeded => "resource_limit_exceeded",
        InvalidParameter => "invalid_parameter",
        InvalidManifest => "invalid_manifest",
        InvalidOutputHash => "invalid_output_hash",
    }
}

/// Infrastructure-neutral validation error.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error, Serialize, Deserialize)]
#[error("{code}: {message}")]
pub struct VisionError {
    pub code: ErrorCode,
    pub message: Box<str>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

impl VisionError {
    pub(crate) fn new(code: ErrorCode, message: &'static str) -> Self {
        Self {
            code,
            message: message.into(),
            index: None,
        }
    }

    pub(crate) fn at(code: ErrorCode, message: &'static str, index: usize) -> Self {
        Self {
            code,
            message: message.into(),
            index: Some(index),
        }
    }
}

pub type Result<T, E = VisionError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_code_registry_has_stable_names_and_serde() {
        for code in ErrorCode::ALL {
            let json = serde_json::to_string(code).unwrap();
            assert_eq!(json, format!("\"{}\"", code.as_str()));
            assert_eq!(code.to_string(), code.as_str());
            assert_eq!(serde_json::from_str::<ErrorCode>(&json).unwrap(), *code);
        }
    }
}
