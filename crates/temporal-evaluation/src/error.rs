use serde::{Deserialize, Serialize};

/// Errors returned while loading or validating the committed benchmark contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, thiserror::Error)]
#[error("benchmark contract is invalid: {message}")]
pub struct ContractError {
    message: Box<str>,
}

impl ContractError {
    pub(crate) fn new(message: impl Into<Box<str>>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<serde_json::Error> for ContractError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(error.to_string().into_boxed_str())
    }
}

pub type Result<T> = std::result::Result<T, ContractError>;
