use sha2::{Digest, Sha256};

use crate::{ContractError, Result};

/// Serializes a contract with deterministic object-key ordering and a final newline.
pub fn canonical_json<T: serde::Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)?;
    let value = sort_value(value);
    let mut bytes = serde_json::to_vec_pretty(&value)?;
    bytes.push(b'\n');
    Ok(bytes)
}

/// Computes the repository's portable, prefixed SHA-256 representation.
pub fn sha256_prefixed(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("sha256:{digest:x}")
}

fn sort_value(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(sort_value).collect())
        }
        serde_json::Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            let mut sorted = serde_json::Map::new();
            for (key, value) in entries {
                sorted.insert(key, sort_value(value));
            }
            serde_json::Value::Object(sorted)
        }
        value => value,
    }
}

pub(crate) fn require_canonical<T: serde::Serialize>(bytes: &[u8], value: &T) -> Result<()> {
    let expected = canonical_json(value)?;
    if expected != bytes {
        return Err(ContractError::new(
            "benchmark-definition.json is not in canonical form",
        ));
    }
    Ok(())
}
