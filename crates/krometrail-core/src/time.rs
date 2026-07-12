//! Explicit domain clocks. No arithmetic is provided between unrelated clocks.

use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid_time},
    validation::deserialize_validated,
};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObservedTime(u64);

impl ObservedTime {
    pub const fn from_nanos(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionTime(u64);

impl SessionTime {
    pub const ZERO: Self = Self(0);

    pub const fn from_nanos(value: u64) -> Self {
        Self(value)
    }

    pub const fn as_nanos(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SourceTime(i128);

impl SourceTime {
    pub const fn from_nanos(value: i128) -> Self {
        Self(value)
    }

    pub const fn as_nanos(self) -> i128 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SessionOrigin {
    observed: ObservedTime,
}

impl SessionOrigin {
    pub const fn new(observed: ObservedTime) -> Self {
        Self { observed }
    }

    pub fn normalize(self, observed: ObservedTime) -> Result<SessionTime> {
        observed
            .as_nanos()
            .checked_sub(self.observed.as_nanos())
            .map(SessionTime::from_nanos)
            .ok_or_else(|| invalid_time("observed time precedes the session origin"))
    }

    pub const fn observed(self) -> ObservedTime {
        self.observed
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct SessionRange {
    start: SessionTime,
    end: SessionTime,
}

#[derive(Deserialize)]
struct SessionRangeWire {
    start: SessionTime,
    end: SessionTime,
}

impl SessionRange {
    pub fn new(start: SessionTime, end: SessionTime) -> Result<Self> {
        if start > end {
            return Err(invalid_time("session range start must not exceed its end"));
        }
        Ok(Self { start, end })
    }

    pub const fn start(self) -> SessionTime {
        self.start
    }

    pub const fn end(self) -> SessionTime {
        self.end
    }

    pub const fn contains(self, value: SessionTime) -> bool {
        value.as_nanos() >= self.start.as_nanos() && value.as_nanos() <= self.end.as_nanos()
    }
}

impl<'de> Deserialize<'de> for SessionRange {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: SessionRangeWire| {
            Self::new(wire.start, wire.end)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_monotonically_without_cross_clock_arithmetic() {
        let origin = SessionOrigin::new(ObservedTime::from_nanos(100));
        assert_eq!(
            origin.normalize(ObservedTime::from_nanos(100)).unwrap(),
            SessionTime::ZERO
        );
        assert_eq!(
            origin
                .normalize(ObservedTime::from_nanos(250))
                .unwrap()
                .as_nanos(),
            150
        );
    }

    #[test]
    fn rejects_observed_time_before_origin() {
        let result =
            SessionOrigin::new(ObservedTime::from_nanos(10)).normalize(ObservedTime::from_nanos(9));
        assert_eq!(result.unwrap_err().code, crate::ErrorCode::InvalidTime);
    }

    #[test]
    fn validates_ranges_and_uses_inclusive_bounds() {
        let start = SessionTime::from_nanos(5);
        let end = SessionTime::from_nanos(10);
        let range = SessionRange::new(start, end).unwrap();
        assert_eq!(range.start(), start);
        assert_eq!(range.end(), end);
        assert!(range.contains(start));
        assert!(range.contains(end));
        assert!(!range.contains(SessionTime::from_nanos(11)));
        assert!(SessionRange::new(end, start).is_err());
    }

    #[test]
    fn rejects_malformed_serialized_ranges() {
        let malformed = r#"{"start":10,"end":5}"#;
        assert!(serde_json::from_str::<SessionRange>(malformed).is_err());
        let valid =
            SessionRange::new(SessionTime::from_nanos(5), SessionTime::from_nanos(10)).unwrap();
        let encoded = serde_json::to_string(&valid).unwrap();
        assert_eq!(
            serde_json::from_str::<SessionRange>(&encoded).unwrap(),
            valid
        );
    }
}
