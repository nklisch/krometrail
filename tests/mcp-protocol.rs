#[path = "support/mcp_process.rs"]
mod process;
use process::{McpProcess, metadata};
use serde_json::{Value, json};

#[test]
fn modern_discovery_pages_cache_policy_and_bounded_errors() {
    let mut p = McpProcess::start();
    let discover = p.request("server/discover", json!({}));
    let d = &discover["result"];
    assert_eq!(
        d["supportedVersions"],
        json!(["2026-07-28", "2025-11-25", "2025-06-18"])
    );
    assert_eq!(d["resultType"], "complete");
    assert_eq!(d["ttlMs"], 0);
    assert_eq!(d["cacheScope"], "private");
    assert_eq!(
        d["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "krometrail"
    );
    assert!(d.get("serverInfo").is_none());
    let tools = p.tools();
    assert!(!tools.is_empty());
    for (method, ttl, key) in [
        ("resources/list", 0, "resources"),
        ("resources/templates/list", 60000, "resourceTemplates"),
    ] {
        let result = p.request(method, json!({}));
        assert!(result["result"][key].is_array());
        assert_eq!(result["result"]["ttlMs"], ttl);
        assert_eq!(result["result"]["cacheScope"], "private");
        assert_eq!(
            p.request(method, json!({"cursor":"unissued"}))["error"]["code"],
            -32602
        );
    }
    for method in [
        "prompts/list",
        "completion/complete",
        "tasks/list",
        "unknown\nsecret-sentinel",
    ] {
        let params = if method == "completion/complete" {
            json!({"ref":{"type":"ref/prompt","name":"unused"},"argument":{"name":"x","value":"x"}})
        } else {
            json!({})
        };
        let error = p.request(method, params);
        assert_eq!(error["error"]["code"], -32601, "{error}");
        assert!(!error.to_string().contains("secret-sentinel"));
    }
    let unknown = format!("secret-sentinel{}", "x".repeat(8192));
    let error = p.request("tools/call", json!({"name":unknown,"arguments":{}}));
    assert_eq!(error["error"]["code"], -32602);
    assert!(!error.to_string().contains("secret-sentinel"));
    for name in ["stop_browser", "list_managed_profiles"] {
        let error = p.request(
            "tools/call",
            json!({"name":name,"arguments":{"session":"unsupported"}}),
        );
        assert_eq!(error["result"]["isError"], true);
        assert_eq!(
            error["result"]["structuredContent"]["error"]["code"],
            "invalid_input"
        );
    }
    for params in [
        json!({"name":"stop_browser","inputResponses":{}}),
        json!({"name":"stop_browser","requestState":"unused"}),
    ] {
        assert_eq!(p.request("tools/call", params)["error"]["code"], -32602);
    }
    assert_eq!(
        p.request(
            "tools/call",
            json!({"name":"list_managed_profiles","arguments":{}})
        )["result"]["isError"],
        false
    );
    p.send(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}));
    assert_eq!(p.recv()["error"]["code"], -32602);
    p.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"_meta":metadata("2099-01-01")}}));
    assert_eq!(p.recv()["error"]["code"], -32022);
    assert!(!p.tools().is_empty());
    p.finish(true);
    // Default diagnostics must not retain raw caller route strings.
    fn inspect(path: &std::path::Path) {
        for entry in std::fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                inspect(&entry.path());
            } else if entry.path().extension().is_some_and(|s| s == "log") {
                let log = std::fs::read_to_string(entry.path()).unwrap();
                assert!(!log.contains("secret-sentinel"));
            }
        }
    }
    inspect(p.root.path());
}

#[test]
fn legacy_versions_and_fallback_preserve_legacy_shape() {
    for (requested, expected) in [
        ("2025-06-18", "2025-06-18"),
        ("2025-11-25", "2025-11-25"),
        ("2099-01-01", "2025-11-25"),
    ] {
        let mut p = McpProcess::start();
        assert_eq!(
            p.initialize(requested)["result"]["protocolVersion"],
            expected
        );
        let legacy = p.tools();
        assert!(legacy.len() > 8);
        assert_eq!(
            p.request("tools/list", json!({"cursor":"unissued"}))["error"]["code"],
            -32602
        );
        p.send(json!({"jsonrpc":"2.0","id":99,"method":"tools/list","params":{"_meta":metadata("2026-07-28")}}));
        let mixed = p.recv();
        assert!(mixed["result"]["tools"].as_array().unwrap().len() <= 8);
        let cursor = mixed["result"]["nextCursor"].as_str().unwrap();
        assert_eq!(
            p.request("tools/list", json!({"cursor":cursor}))["error"]["code"],
            -32602
        );
        assert_eq!(
            p.tools(),
            legacy,
            "per-request override must not change the negotiated default"
        );
        for method in ["resources/list", "resources/templates/list"] {
            let r = p.request(method, json!({}));
            assert!(r["result"].get("ttlMs").is_none());
            assert!(r["result"].get("resultType").is_none());
        }
        p.finish(true);
        let mut modern = McpProcess::start();
        let current = modern.tools();
        assert_eq!(legacy, current);
        modern.finish(true);
    }
}
#[test]
fn opener_failure_is_reported_and_supported_request_recovers_from_unknown_version() {
    let mut p = McpProcess::start();
    p.send(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":metadata("2099-01-01")}}));
    let r = p.recv();
    assert_eq!(r["error"]["code"], -32022);
    assert_eq!(r["error"]["data"]["requested"], "2099-01-01");
    assert!(!p.tools().is_empty());
    p.finish(true);
    for meta in [
        Value::Null,
        json!({"io.modelcontextprotocol/protocolVersion":123}),
    ] {
        let mut p = McpProcess::start();
        p.send(json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{"_meta":meta}}));
        assert_eq!(p.recv()["error"]["code"], -32602);
        p.finish(false);
    }
    let mut empty = McpProcess::start();
    empty.finish(true);
}
#[test]
fn independent_request_ids_can_be_pipelined() {
    let mut p = McpProcess::start();
    p.request("server/discover", json!({}));
    for id in 1..=20 {
        p.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/list","params":{"_meta":metadata("2026-07-28")}}));
    }
    let mut ids = std::collections::BTreeSet::new();
    for _ in 1..=20 {
        let r = p.recv();
        assert!(r["result"]["tools"].is_array());
        assert!(ids.insert(r["id"].as_u64().unwrap()));
    }
    assert_eq!(ids.len(), 20);
    p.finish(true);
}

#[test]
#[cfg(unix)]
fn shutdown_is_bounded_when_stdout_is_not_consumed() {
    let mut p = McpProcess::start_with_output_paused();
    p.request("server/discover", json!({}));
    for id in 0..30 {
        p.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/list","params":{"_meta":metadata("2026-07-28")}}));
    }
    // Allow the startup exchange to reach the intentionally full output pipe.
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(unsafe { libc::kill(p.pid() as i32, libc::SIGTERM) }, 0);
    let stderr = p.finish(false);
    assert!(stderr.contains("response transport did not finish shutdown"));
}

#[test]
fn eof_alone_bounds_an_established_unread_response_pipe() {
    let mut p = McpProcess::start_with_output_paused();
    p.request("server/discover", json!({}));
    for id in 0..30 {
        p.send(json!({"jsonrpc":"2.0","id":id,"method":"tools/list","params":{"_meta":metadata("2026-07-28")}}));
    }
    let stderr = p.finish(false);
    assert!(stderr.contains("response transport did not finish shutdown"));
}
