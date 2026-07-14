use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    error::{NonEmptyText, Result, invalid},
    ids::TargetId,
    validation::deserialize_validated,
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserInstallationSource {
    ExplicitRequest,
    EnvironmentOverride,
    PlatformDefault,
    PathLookup,
}

impl BrowserInstallationSource {
    pub const ALL: &'static [Self] = &[
        Self::ExplicitRequest,
        Self::EnvironmentOverride,
        Self::PlatformDefault,
        Self::PathLookup,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitRequest => "explicit_request",
            Self::EnvironmentOverride => "environment_override",
            Self::PlatformDefault => "platform_default",
            Self::PathLookup => "path_lookup",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProduct {
    Chrome,
    Chromium,
    ElectronRenderer,
    OtherChromium,
}

impl BrowserProduct {
    pub const ALL: &'static [Self] = &[
        Self::Chrome,
        Self::Chromium,
        Self::ElectronRenderer,
        Self::OtherChromium,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Chromium => "chromium",
            Self::ElectronRenderer => "electron_renderer",
            Self::OtherChromium => "other_chromium",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct BrowserProductVersion(NonEmptyText);

impl BrowserProductVersion {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        NonEmptyText::new(value)
            .map(Self)
            .map_err(|_| invalid("browser product version must not be empty"))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl<'de> Deserialize<'de> for BrowserProductVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |value: String| Self::new(value))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserInstallation {
    pub executable: PathBuf,
    pub source: BrowserInstallationSource,
    pub product: BrowserProduct,
    pub version: BrowserProductVersion,
}

#[derive(Deserialize)]
struct BrowserInstallationWire {
    executable: PathBuf,
    source: BrowserInstallationSource,
    product: BrowserProduct,
    version: BrowserProductVersion,
}

impl BrowserInstallation {
    pub fn new(
        executable: impl Into<PathBuf>,
        source: BrowserInstallationSource,
        product: BrowserProduct,
        version: BrowserProductVersion,
    ) -> Result<Self> {
        let executable = executable.into();
        if executable.as_os_str().is_empty() {
            return Err(invalid("browser executable must not be empty"));
        }
        Ok(Self {
            executable,
            source,
            product,
            version,
        })
    }
}

impl<'de> Deserialize<'de> for BrowserInstallation {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserInstallationWire| {
            Self::new(wire.executable, wire.source, wire.product, wire.version)
        })
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManagedProfilePersistence {
    Reusable,
    Temporary,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ManagedProfileRef {
    pub identity: ProfileIdentity,
    pub persistence: ManagedProfilePersistence,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileRef {
    Managed(ManagedProfileRef),
    External,
}

impl ProfileRef {
    pub const fn external() -> Self {
        Self::External
    }

    pub fn managed(identity: ProfileIdentity) -> Self {
        Self::Managed(ManagedProfileRef {
            identity,
            persistence: ManagedProfilePersistence::Reusable,
        })
    }

    pub fn temporary(identity: ProfileIdentity) -> Self {
        Self::Managed(ManagedProfileRef {
            identity,
            persistence: ManagedProfilePersistence::Temporary,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct BrowserVersion {
    pub product: BrowserProduct,
    pub product_version: BrowserProductVersion,
    pub revision: NonEmptyText,
    pub protocol_version: NonEmptyText,
    pub user_agent: NonEmptyText,
    pub js_version: NonEmptyText,
}

#[derive(Deserialize)]
struct BrowserVersionWire {
    product: BrowserProduct,
    product_version: BrowserProductVersion,
    revision: NonEmptyText,
    protocol_version: NonEmptyText,
    user_agent: NonEmptyText,
    js_version: NonEmptyText,
}

impl BrowserVersion {
    pub fn new(
        product: BrowserProduct,
        product_version: BrowserProductVersion,
        revision: impl Into<String>,
        protocol_version: impl Into<String>,
        user_agent: impl Into<String>,
        js_version: impl Into<String>,
    ) -> Result<Self> {
        let version = Self {
            product,
            product_version,
            revision: non_empty("browser revision", revision)?,
            protocol_version: non_empty("browser protocol version", protocol_version)?,
            user_agent: non_empty("browser user agent", user_agent)?,
            js_version: non_empty("browser JavaScript version", js_version)?,
        };
        version.validate()?;
        Ok(version)
    }

    pub fn product(&self) -> BrowserProduct {
        self.product
    }

    pub fn product_version(&self) -> &BrowserProductVersion {
        &self.product_version
    }

    pub fn revision(&self) -> &str {
        self.revision.as_str()
    }

    pub fn protocol_version(&self) -> &str {
        self.protocol_version.as_str()
    }

    pub fn user_agent(&self) -> &str {
        self.user_agent.as_str()
    }

    pub fn js_version(&self) -> &str {
        self.js_version.as_str()
    }

    pub(crate) fn validate(&self) -> Result<()> {
        Ok(())
    }
}

impl<'de> Deserialize<'de> for BrowserVersion {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserialize_validated(deserializer, |wire: BrowserVersionWire| {
            Self::new(
                wire.product,
                wire.product_version,
                wire.revision.as_str(),
                wire.protocol_version.as_str(),
                wire.user_agent.as_str(),
                wire.js_version.as_str(),
            )
        })
    }
}

fn non_empty(name: &str, value: impl Into<String>) -> Result<NonEmptyText> {
    NonEmptyText::new(value).map_err(|_| invalid(format!("{name} must not be empty")))
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
    use crate::ids::TargetId;

    const UUID: &str = "123e4567-e89b-12d3-a456-426614174000";

    fn version() -> BrowserVersion {
        BrowserVersion::new(
            BrowserProduct::Chrome,
            BrowserProductVersion::new("128.0.1").unwrap(),
            "revision",
            "1.3",
            "Chrome/128",
            "12.8",
        )
        .unwrap()
    }

    #[test]
    fn validates_browser_identity_installation_profile_and_target_boundaries() {
        assert!(BrowserProductVersion::new(" ").is_err());
        assert!(
            BrowserInstallation::new(
                "",
                BrowserInstallationSource::PathLookup,
                BrowserProduct::Chrome,
                BrowserProductVersion::new("128").unwrap()
            )
            .is_err()
        );
        assert!(ProfileIdentity::new(" ").is_err());
        assert!(
            BrowserVersion::new(
                BrowserProduct::Chrome,
                BrowserProductVersion::new("128").unwrap(),
                "",
                "1.3",
                "ua",
                "js"
            )
            .is_err()
        );
        assert!(
            PageTarget::new(
                TargetId::from_uuid(UUID.parse().unwrap()),
                "",
                "https://example.test",
                ""
            )
            .is_err()
        );
    }

    #[test]
    fn managed_and_external_profiles_have_distinct_wire_values() {
        let managed = ProfileRef::managed(ProfileIdentity::new("profile").unwrap());
        let managed_json = serde_json::to_string(&managed).unwrap();
        assert_eq!(
            managed_json,
            r#"{"managed":{"identity":"profile","persistence":"reusable"}}"#
        );
        assert_eq!(
            serde_json::from_str::<ProfileRef>(&managed_json).unwrap(),
            managed
        );
        let temporary = ProfileRef::temporary(ProfileIdentity::new("temporary-1").unwrap());
        let temporary_json = serde_json::to_string(&temporary).unwrap();
        assert!(temporary_json.contains("temporary"));
        assert_eq!(
            serde_json::from_str::<ProfileRef>(&temporary_json).unwrap(),
            temporary
        );

        let external = serde_json::to_string(&ProfileRef::External).unwrap();
        assert_eq!(external, r#""external""#);
        assert_eq!(
            serde_json::from_str::<ProfileRef>(&external).unwrap(),
            ProfileRef::External
        );
    }

    #[test]
    fn complete_browser_version_round_trips_and_rejects_missing_values() {
        let value = version();
        let encoded = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<BrowserVersion>(&encoded).unwrap(),
            value
        );
        assert!(serde_json::from_str::<BrowserVersion>(
			r#"{"product":"chrome","product_version":"128","revision":"r","protocol_version":"1.3","user_agent":"","js_version":"js"}"#
		)
		.is_err());
    }
}
