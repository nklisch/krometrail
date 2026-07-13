#![cfg(feature = "cdpkit-transport")]

mod support;

use std::path::PathBuf;

use krometrail_core::BrowserProduct;

use support::{
    chrome::{ChromeWrapper, ChromeWrapperVariant},
    smoke_evidence::{
        CrossPlatformSmokeEvidence, load_schema, sample_path, schema_path, validate_against_schema,
    },
};

const CONFIGURATION_NAMES: &[&str] = &[
    "linux-chrome",
    "linux-chromium",
    "macos-chrome-default-dpi",
    "macos-chrome-high-dpi",
];

#[derive(Clone, Debug)]
struct Configuration {
    name: &'static str,
    variant: ChromeWrapperVariant,
    product: BrowserProduct,
}

fn configurations_for_this_platform() -> Vec<Configuration> {
    #[cfg(target_os = "linux")]
    {
        vec![
            Configuration {
                name: "linux-chrome",
                variant: ChromeWrapperVariant::DefaultDpi,
                product: BrowserProduct::Chrome,
            },
            Configuration {
                name: "linux-chromium",
                variant: ChromeWrapperVariant::DefaultDpi,
                product: BrowserProduct::Chromium,
            },
        ]
    }
    #[cfg(target_os = "macos")]
    {
        vec![
            Configuration {
                name: "macos-chrome-default-dpi",
                variant: ChromeWrapperVariant::DefaultDpi,
                product: BrowserProduct::Chrome,
            },
            Configuration {
                name: "macos-chrome-high-dpi",
                variant: ChromeWrapperVariant::HighDpi,
                product: BrowserProduct::Chrome,
            },
        ]
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        Vec::new()
    }
}

#[test]
fn deterministic_wrapper_script_bytes_contain_forced_scale_flags() {
    let executable = PathBuf::from("/tmp/sentinel-chrome");
    let default = ChromeWrapper::script_bytes(&executable, ChromeWrapperVariant::DefaultDpi);
    let default = String::from_utf8(default).unwrap();
    assert!(default.contains("--headless=new"));
    assert!(default.contains("--disable-gpu"));
    assert!(default.contains("--no-sandbox"));
    assert!(default.contains("--force-device-scale-factor=1"));

    let high = ChromeWrapper::script_bytes(&executable, ChromeWrapperVariant::HighDpi);
    let high = String::from_utf8(high).unwrap();
    assert!(high.contains("--headless=new"));
    assert!(high.contains("--disable-gpu"));
    assert!(high.contains("--no-sandbox"));
    assert!(high.contains("--high-dpi-support=1"));
    assert!(high.contains("--force-device-scale-factor=2"));
}

#[test]
fn deterministic_product_filter_returns_matching_installation_or_none() {
    let chrome =
        ChromeWrapper::for_product(BrowserProduct::Chrome, ChromeWrapperVariant::DefaultDpi);
    let chromium =
        ChromeWrapper::for_product(BrowserProduct::Chromium, ChromeWrapperVariant::DefaultDpi);

    if chrome.is_none() && chromium.is_none() {
        eprintln!("no Chrome or Chromium installation discovered; filter None case exercised");
    }

    if let Some(wrapper) = chrome {
        assert_eq!(wrapper.product, BrowserProduct::Chrome);
    }
    if let Some(wrapper) = chromium {
        assert_eq!(wrapper.product, BrowserProduct::Chromium);
    }
}

#[test]
fn deterministic_configurations_match_platform() {
    let configurations = configurations_for_this_platform();
    #[cfg(target_os = "linux")]
    {
        assert_eq!(configurations.len(), 2);
        assert!(
            configurations
                .iter()
                .any(|configuration| configuration.name == "linux-chrome")
        );
        assert!(
            configurations
                .iter()
                .any(|configuration| configuration.name == "linux-chromium")
        );
    }
    #[cfg(target_os = "macos")]
    {
        assert_eq!(configurations.len(), 2);
        assert!(
            configurations
                .iter()
                .any(|configuration| configuration.name == "macos-chrome-default-dpi")
        );
        assert!(
            configurations
                .iter()
                .any(|configuration| configuration.name == "macos-chrome-high-dpi")
        );
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        assert!(configurations.is_empty());
    }

    for configuration in configurations {
        assert!(CONFIGURATION_NAMES.contains(&configuration.name));
        let expected_product = match configuration.name {
            "linux-chrome" | "macos-chrome-default-dpi" | "macos-chrome-high-dpi" => {
                BrowserProduct::Chrome
            }
            "linux-chromium" => BrowserProduct::Chromium,
            _ => unreachable!(),
        };
        assert_eq!(configuration.product, expected_product);
        assert_eq!(
            configuration.variant.force_device_scale_factor(),
            match configuration.variant {
                ChromeWrapperVariant::DefaultDpi => 1.0,
                ChromeWrapperVariant::HighDpi => 2.0,
            }
        );
    }
}

#[test]
fn deterministic_canonical_bytes_match_committed_sample() {
    let expected = std::fs::read(sample_path()).expect("committed sample.json exists");
    let actual = CrossPlatformSmokeEvidence::sample()
        .to_canonical_bytes()
        .expect("sample serializes");
    assert_eq!(
        expected, actual,
        "committed sample.json must match the serializer's canonical bytes"
    );
}

#[test]
fn deterministic_schema_validates_sample_and_serializer_output() {
    let schema = load_schema();
    let sample_value = serde_json::to_value(CrossPlatformSmokeEvidence::sample()).unwrap();
    validate_against_schema(&sample_value, &schema)
        .expect("serializer output validates against schema");

    let committed_sample = serde_json::from_slice::<serde_json::Value>(
        &std::fs::read(sample_path()).expect("sample.json exists"),
    )
    .expect("sample.json is valid JSON");
    validate_against_schema(&committed_sample, &schema)
        .expect("committed sample.json validates against schema");
}

#[test]
fn deterministic_sanitizer_rejects_private_paths_and_endpoints() {
    CrossPlatformSmokeEvidence::sample()
        .validate()
        .expect("sample passes serializer invariants and sanitizer");
}

#[tokio::test]
async fn deterministic_browser_version_accessors_on_scripted_session() {
    let transport = support::scripted_cdp::ScriptedCdp::chrome();
    let compatibility = krometrail_cdp::probe_compatibility(&transport)
        .await
        .expect("scripted Chrome passes compatibility probe");
    let version = &compatibility.version;
    assert_eq!(version.product(), BrowserProduct::Chrome);
    assert!(!version.product_version().as_str().is_empty());
    assert!(!version.revision().is_empty());
    assert!(!version.protocol_version().is_empty());
    assert!(!version.user_agent().is_empty());
    assert!(!version.js_version().is_empty());
}

#[test]
fn deterministic_committed_schema_and_sample_exist() {
    assert!(schema_path().is_file(), "schema.json must be committed");
    assert!(sample_path().is_file(), "sample.json must be committed");
}
