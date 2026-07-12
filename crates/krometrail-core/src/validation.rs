use serde::{Deserialize, Deserializer, de::Error as _};

use crate::error::Result;

/// Deserialize a public wire shape only after its domain constructor validates it.
/// Keeping this adapter small makes boundary implementations uniform without hiding
/// the aggregate-specific invariants in a generic schema layer.
pub(crate) fn deserialize_validated<'de, D, Wire, Value, Validate>(
    deserializer: D,
    validate: Validate,
) -> std::result::Result<Value, D::Error>
where
    D: Deserializer<'de>,
    Wire: Deserialize<'de>,
    Validate: FnOnce(Wire) -> Result<Value>,
{
    let wire = Wire::deserialize(deserializer)?;
    validate(wire).map_err(|error| D::Error::custom(error.to_string()))
}
