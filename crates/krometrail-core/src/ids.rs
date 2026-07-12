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
	};
}

// Add a new identifier here once; display, parsing, serde, and ordering all
// remain consistent because the implementations are generated together.
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

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    macro_rules! round_trip_ids {
		($($id:ident),+ $(,)?) => {
			$(
				let value = $id::from_uuid(UUID.parse().unwrap());
				assert_eq!(value.to_string(), UUID);
				assert_eq!(value, UUID.parse::<$id>().unwrap());
				let json = serde_json::to_string(&value).unwrap();
				assert_eq!(json, format!("\"{UUID}\""));
				assert_eq!(serde_json::from_str::<$id>(&json).unwrap(), value);
			)+
		};
	}

    #[test]
    fn all_registered_ids_round_trip() {
        round_trip_ids!(
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
    }

    #[test]
    fn invalid_id_text_is_rejected() {
        assert!("not-an-id".parse::<SessionId>().is_err());
    }
}
