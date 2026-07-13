#![cfg(feature = "cdpkit-transport")]

mod support;

use krometrail_cdp::{
    EndpointKind, RENDERER_CAPABILITY_PROBES, RendererCapabilityProbe, probe_compatibility,
};
use support::scripted_cdp::ScriptedCdp;

#[test]
fn one_registry_drives_scope_command_and_decoder_metadata() {
    assert_eq!(
        RENDERER_CAPABILITY_PROBES.len(),
        krometrail_core::RendererCapability::ALL.len()
    );
    for probe in RENDERER_CAPABILITY_PROBES {
        assert!(!probe.command.is_empty());
        assert_eq!(probe.id, probe.capability.as_str());
        let _: &RendererCapabilityProbe = probe;
    }
}

#[tokio::test]
async fn chrome_chromium_and_electron_renderers_use_capabilities_not_branding() {
    for (product, user_agent, expected) in [
        ("Chrome/149", "Chrome/149", EndpointKind::Chrome),
        ("Chromium/149", "Chromium/149", EndpointKind::Chromium),
        (
            "Electron/35",
            "Chrome/135 Electron/35",
            EndpointKind::ElectronRenderer,
        ),
    ] {
        let transport = ScriptedCdp::capable(product, user_agent);
        assert!(probe_compatibility(&transport).await.is_ok(), "{product}");
        assert_ne!(expected, EndpointKind::NodeInspector);
    }
}

#[tokio::test]
async fn node_inspector_is_rejected_even_when_product_identity_is_present() {
    let transport = ScriptedCdp::capable("Node.js/22", "Node.js/22");
    let error = probe_compatibility(&transport).await.unwrap_err();
    assert!(matches!(
        error,
        krometrail_cdp::CompatibilityProbeError::NodeInspectorOnly
    ));
}

#[tokio::test]
async fn every_required_command_can_fail_without_claiming_a_wildcard_probe() {
    for probe in RENDERER_CAPABILITY_PROBES {
        let transport = ScriptedCdp::chrome();
        transport.missing(probe.command);
        let result = probe_compatibility(&transport).await;
        assert!(
            result.is_err(),
            "{} unexpectedly passed",
            probe.capability.as_str()
        );
    }
}
