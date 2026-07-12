//! Opaque identifiers used to keep domain entities distinct at compile time.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IdValue(uuid::Uuid);

impl IdValue {
    pub const fn from_uuid(value: uuid::Uuid) -> Self {
        Self(value)
    }

    pub const fn as_uuid(&self) -> &uuid::Uuid {
        &self.0
    }
}

impl fmt::Display for IdValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for IdValue {
    type Err = uuid::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        uuid::Uuid::parse_str(value).map(Self)
    }
}

macro_rules! typed_ids {
    ($($name:ident),+ $(,)?) => {
        $(
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
            #[serde(transparent)]
            pub struct $name(IdValue);

            impl $name {
                pub const fn from_uuid(value: uuid::Uuid) -> Self {
                    Self(IdValue::from_uuid(value))
                }

                pub const fn as_uuid(&self) -> &uuid::Uuid {
                    self.0.as_uuid()
                }
            }

            impl fmt::Display for $name {
                fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    self.0.fmt(formatter)
                }
            }

            impl FromStr for $name {
                type Err = uuid::Error;

                fn from_str(value: &str) -> Result<Self, Self::Err> {
                    value.parse().map(Self)
                }
            }
        )+

        // Keep exhaustive contract coverage attached to the declaration that
        // creates the public ID types. Adding an ID here cannot leave its
        // display, parsing, or serialization contract untested.
        #[cfg(test)]
        mod generated_contract_tests {
            use super::*;

            const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

            #[test]
            fn all_registered_ids_round_trip() {
                let uuid = UUID.parse().unwrap();
                $(
                    let value = $name::from_uuid(uuid);
                    assert_eq!(value.to_string(), UUID);
                    assert_eq!(value, UUID.parse::<$name>().unwrap());
                    let json = serde_json::to_string(&value).unwrap();
                    assert_eq!(json, format!("\"{UUID}\""));
                    assert_eq!(serde_json::from_str::<$name>(&json).unwrap(), value);
                )+
            }
        }
    };
}

// Add a new identifier here once; display, parsing, serde, ordering, and
// exhaustive round-trip coverage are generated from this declaration.
typed_ids!(
    SessionId,
    TargetId,
    FrameId,
    InteractionId,
    MarkerId,
    SegmentId,
    ArtifactId,
    GapId,
    NavigationId,
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_id_text_is_rejected() {
        assert!("not-an-id".parse::<SessionId>().is_err());
    }
}
