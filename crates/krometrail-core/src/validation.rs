use serde::{Deserialize, Deserializer, de::Error as _};

use crate::error::Result;

macro_rules! delegate_json_schema {
    ($public:ty => $wire:ty) => {
        impl schemars::JsonSchema for $public {
            fn schema_name() -> std::borrow::Cow<'static, str> {
                <$wire as schemars::JsonSchema>::schema_name()
            }

            fn schema_id() -> std::borrow::Cow<'static, str> {
                <$wire as schemars::JsonSchema>::schema_id()
            }

            fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
                <$wire as schemars::JsonSchema>::json_schema(generator)
            }
        }
    };
}

pub(crate) use delegate_json_schema;

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
