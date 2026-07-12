use serde::{Deserialize, Serialize};

use crate::{
    error::{Result, invalid},
    ids::TargetId,
    validation::deserialize_validated,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserVersion {
    product: String,
    revision: String,
    protocol: String,
}

#[derive(Deserialize)]
struct BrowserVersionWire {
    product: String,
    revision: String,
    protocol: String,
}

impl BrowserVersion {
    pub fn new(
        product: impl Into<String>,
        revision: impl Into<String>,
        protocol: impl Into<String>,
    ) -> Result<Self> {
        let version = Self {
            product: product.into(),
            revision: revision.into(),
            protocol: protocol.into(),
        };
        version.validate()?;
        Ok(version)
    }

    pub fn product(&self) -> &str {
        &self.product
    }

    pub fn revision(&self) -> &str {
        &self.revision
    }

    pub fn protocol(&self) -> &str {
        &self.protocol
    }

    pub(crate) fn validate(&self) -> Result<()> {
        for (field, value) in [
            ("product", &self.product),
            ("revision", &self.revision),
            ("protocol", &self.protocol),
        ] {
            if value.trim().is_empty() {
                return Err(invalid(format!("browser {field} must not be empty")));
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for BrowserVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserVersionWire| {
            Self::new(wire.product, wire.revision, wire.protocol)
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ProfileIdentity(String);

impl ProfileIdentity {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(invalid("profile identity must not be empty"));
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for ProfileIdentity {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: String| Self::new(value))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PageTarget {
    id: TargetId,
    browser_target_key: String,
    url: String,
    title: String,
}

#[derive(Deserialize)]
struct PageTargetWire {
    id: TargetId,
    browser_target_key: String,
    url: String,
    title: String,
}

impl PageTarget {
    pub fn new(
        id: TargetId,
        browser_target_key: impl Into<String>,
        url: impl Into<String>,
        title: impl Into<String>,
    ) -> Result<Self> {
        let target = Self {
            id,
            browser_target_key: browser_target_key.into(),
            url: url.into(),
            title: title.into(),
        };
        target.validate()?;
        Ok(target)
    }

    pub fn id(&self) -> TargetId {
        self.id
    }

    pub fn browser_target_key(&self) -> &str {
        &self.browser_target_key
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    fn validate(&self) -> Result<()> {
        if self.browser_target_key.trim().is_empty() {
            return Err(invalid("browser target key must not be empty"));
        }
        if self.url.trim().is_empty() {
            return Err(invalid("target URL must not be empty"));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for PageTarget {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: PageTargetWire| {
            Self::new(wire.id, wire.browser_target_key, wire.url, wire.title)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    #[test]
    fn validates_browser_profile_and_target_boundaries() {
        assert!(BrowserVersion::new("", "revision", "protocol").is_err());
        assert!(ProfileIdentity::new(" ").is_err());
        assert!(
            PageTarget::new(
                TargetId::from_uuid(UUID.parse().unwrap()),
                "",
                "https://example.test",
                ""
            )
            .is_err()
        );
        assert!(
            PageTarget::new(TargetId::from_uuid(UUID.parse().unwrap()), "target", "", "").is_err()
        );
    }

    #[test]
    fn rejects_malformed_serialized_validated_browser_values() {
        assert!(
            serde_json::from_str::<BrowserVersion>(
                r#"{"product":"","revision":"r","protocol":"p"}"#
            )
            .is_err()
        );
        assert!(serde_json::from_str::<ProfileIdentity>(r#""  "#).is_err());
        assert!(serde_json::from_str::<PageTarget>(&format!(r#"{{"id":"{UUID}","browser_target_key":"","url":"https://example.test","title":""}}"#)).is_err());
    }
}
