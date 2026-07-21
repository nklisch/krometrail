#![cfg(feature = "cdpkit-transport")]

mod support;

use std::{fs, sync::Arc, time::Duration};

use krometrail_cdp::{
    CdpTransport, CdpTransportFactory, CommandScope, LauncherConfig, ProductionBrowserConnector,
    SystemChromeLauncher,
};
use krometrail_core::{BrowserConnectRequest, BrowserConnector, LaunchBrowser, ManagedProfile};

#[tokio::test]
async fn opt_in_managed_session_stops_without_retaining_temporary_profile() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("managed");
    let root = root_guard.path().to_path_buf();
    let launcher_config = LauncherConfig {
        profile_root: root.clone(),
        startup_timeout: Duration::from_secs(45),
        shutdown_timeout: Duration::from_secs(3),
    };
    let connector = ProductionBrowserConnector::new(
        Arc::new(SystemChromeLauncher::new(launcher_config)),
        Arc::new(
            krometrail_cdp::transport::CdpkitTransportFactory::new()
                .with_command_timeout(Duration::from_secs(3)),
        ),
    );
    let request = LaunchBrowser {
        executable: None,
        profile: ManagedProfile::Temporary,
        initial_url: Some(support::chrome::fixture_url()),
        every_nth_frame: krometrail_core::EveryNthFrame::default(),
        focus: krometrail_core::BrowserFocusPolicy::default(),
    };
    let session = connector
        .connect(BrowserConnectRequest::Launch(request))
        .await
        .expect("opt-in Chrome should launch and pass compatibility");
    assert_eq!(
        session.status().await.unwrap().ownership,
        krometrail_core::BrowserOwnership::Managed
    );
    let outcome = session.stop().await.expect("managed stop");
    assert_eq!(
        outcome.closure(),
        krometrail_core::BrowserClosure::ManagedBrowserClosed
    );
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
    let references = support::chrome::process_references(&root);
    assert!(
        references.is_empty(),
        "managed Chrome still references unique profile root before cleanup: {references:?}"
    );
    assert!(!root.join("tmp").exists() || fs::read_dir(root.join("tmp")).unwrap().next().is_none());
    // Release the session before the root guard so the assertion below observes a browser that
    // has already let go of the profile. Relying on declaration order would silently invert if
    // the bindings were ever reordered.
    drop(session);
    drop(root_guard);
    assert!(
        !root.exists(),
        "unique profile root survived the test that owns it"
    );
}

#[tokio::test]
async fn opt_in_managed_launch_attach_targets_and_external_survival() {
    if !support::chrome::real_browser_tests_enabled() {
        eprintln!("skipping real Chrome test; set KROMETRAIL_REAL_CHROME_TESTS=1");
        return;
    }
    let _browser_lock = support::chrome::real_browser_lock().await;
    let root_guard = support::chrome::temporary_profile_root("targets");
    let root = root_guard.path().to_path_buf();
    let launcher = SystemChromeLauncher::new(LauncherConfig {
        profile_root: root.clone(),
        startup_timeout: Duration::from_secs(45),
        shutdown_timeout: Duration::from_secs(3),
    });
    let request = LaunchBrowser {
        executable: None,
        profile: ManagedProfile::Temporary,
        initial_url: Some(support::chrome::fixture_url()),
        every_nth_frame: krometrail_core::EveryNthFrame::default(),
        focus: krometrail_core::BrowserFocusPolicy::default(),
    };
    let mut launched = launcher
        .launch_owned(&request)
        .await
        .expect("Chrome should launch for target supervision");
    let factory = krometrail_cdp::transport::CdpkitTransportFactory::new()
        .with_command_timeout(Duration::from_secs(3));
    let raw = factory
        .connect(launched.endpoint.browser_websocket_url().as_str())
        .await
        .expect("raw browser connection");
    let target_keys = [
        create_target(raw.as_ref()).await,
        create_target(raw.as_ref()).await,
    ];
    wait_for_page_targets(raw.as_ref(), 3).await;
    let connector = ProductionBrowserConnector::new(Arc::new(launcher), Arc::new(factory));
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new(launched.endpoint.browser_websocket_url().as_str())
                .unwrap(),
        ))
        .await
        .expect("attached supervision session");
    let pages = session.status().await.unwrap().pages;
    assert!(pages.len() >= 3, "expected initial plus two page targets");
    assert!(target_keys.iter().all(|key| {
        pages
            .iter()
            .any(|page| page.target.target.browser_target_key() == key)
    }));

    let mut events = session.subscribe().await.unwrap();
    let created_key = create_target(raw.as_ref()).await;
    let mut observed = false;
    for _ in 0..40 {
        if session
            .status()
            .await
            .unwrap()
            .pages
            .iter()
            .any(|page| page.target.target.browser_target_key() == created_key)
        {
            observed = true;
            break;
        }
        let _ = tokio::time::timeout(Duration::from_millis(25), events.next()).await;
    }
    assert!(observed, "target creation was not reconciled");
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
    raw.send_raw(
        &CommandScope::Browser,
        "Browser.getVersion",
        serde_json::json!({}),
    )
    .await
    .expect("attached stop must leave external browser alive");
    drop(raw);
    launched.shutdown().await.expect("owned browser shutdown");
    drop(launched);
    let references = support::chrome::process_references(&root);
    assert!(
        references.is_empty(),
        "managed Chrome still references unique profile root before cleanup: {references:?}"
    );
    assert!(!root.join("tmp").exists() || root.join("tmp").read_dir().unwrap().next().is_none());
    // `launched` is already released above; drop the guard explicitly so the root's removal is
    // asserted rather than left to end-of-scope ordering.
    drop(session);
    drop(root_guard);
    assert!(
        !root.exists(),
        "unique profile root survived the test that owns it"
    );
}

async fn wait_for_page_targets(transport: &dyn CdpTransport, minimum: usize) {
    for _ in 0..40 {
        let count = transport
            .send_raw(
                &CommandScope::Browser,
                "Target.getTargets",
                serde_json::json!({}),
            )
            .await
            .expect("target snapshot")
            .get("targetInfos")
            .and_then(serde_json::Value::as_array)
            .map(|targets| {
                targets
                    .iter()
                    .filter(|target| {
                        target.get("type").and_then(serde_json::Value::as_str) == Some("page")
                            && target
                                .get("url")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|url| !url.is_empty())
                    })
                    .count()
            })
            .unwrap_or_default();
        if count >= minimum {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("page targets did not become ready");
}

async fn create_target(transport: &dyn CdpTransport) -> String {
    transport
        .send_raw(
            &CommandScope::Browser,
            "Target.createTarget",
            serde_json::json!({"url": support::chrome::fixture_url()}),
        )
        .await
        .expect("create target")
        .get("targetId")
        .and_then(serde_json::Value::as_str)
        .expect("target id")
        .to_owned()
}

#[tokio::test]
async fn opt_in_electron_renderer_endpoint_uses_capability_probe() {
    let Some(endpoint) = std::env::var("KROMETRAIL_ELECTRON_ENDPOINT").ok() else {
        eprintln!("skipping Electron test; set KROMETRAIL_ELECTRON_ENDPOINT");
        return;
    };
    let _browser_lock = support::chrome::real_browser_lock().await;
    let connector = ProductionBrowserConnector::default();
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new(endpoint).unwrap(),
        ))
        .await
        .expect("explicit Electron renderer endpoint should be compatible");
    let status = session.status().await.unwrap();
    assert_eq!(
        status.compatibility.version.product,
        krometrail_core::BrowserProduct::ElectronRenderer
    );
    assert_eq!(
        status.ownership,
        krometrail_core::BrowserOwnership::Attached
    );
    assert_eq!(status.profile, krometrail_core::ProfileRef::External);
    assert!(status.selected_target_id.is_some());
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
}

#[tokio::test]
async fn opt_in_attach_stop_does_not_close_external_browser() {
    let Some(endpoint) = std::env::var("KROMETRAIL_REAL_ATTACH_ENDPOINT").ok() else {
        eprintln!("skipping attach test; set KROMETRAIL_REAL_ATTACH_ENDPOINT");
        return;
    };
    let _browser_lock = support::chrome::real_browser_lock().await;
    let connector = ProductionBrowserConnector::default();
    let session = connector
        .connect(BrowserConnectRequest::Attach(
            krometrail_core::AttachBrowser::new(endpoint).unwrap(),
        ))
        .await
        .expect("explicit external endpoint should be compatible");
    let outcome = session.stop().await.unwrap();
    assert_eq!(outcome.closure(), krometrail_core::BrowserClosure::Detached);
    assert_eq!(outcome.quality(), krometrail_core::ShutdownQuality::Clean);
}
