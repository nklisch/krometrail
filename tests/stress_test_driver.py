"""Exploratory manual dogfooding driver, retained for follow-up work.

Not a CI test or qualification gate: requests and response assumptions need
revalidation against the current schema. This script visits public sites, uses
a machine-specific binary path, lacks bounded response reads, and does not
reliably return a failing process status for failed or incomplete runs.
Do not interpret its PASS summary as verified task completion. The reliability
live-browser and agent-journey backlog items own a proper local-fixture harness.
"""

import subprocess
import json
import time
import sys
import traceback
from typing import Any, Dict, List, Optional

class KrometrailClient:
    def __init__(self, bin_path: str = "/storage/cargo-target/debug/krometrail"):
        self.proc = subprocess.Popen(
            [bin_path, "mcp"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            bufsize=1
        )
        self.msg_id = 0
        self.session_id: Optional[str] = None
        self.selected_target_id: Optional[str] = None
        self._initialize()

    def _send(self, msg: dict):
        line = json.dumps(msg) + "\n"
        self.proc.stdin.write(line)
        self.proc.stdin.flush()

    def _read(self) -> dict:
        line = self.proc.stdout.readline()
        if not line:
            err = self.proc.stderr.read()
            raise RuntimeError(f"EOF from server. Stderr: {err}")
        return json.loads(line)

    def _initialize(self):
        self.msg_id += 1
        self._send({
            "jsonrpc": "2.0",
            "id": self.msg_id,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "stress-test-client", "version": "1.0.0"}
            }
        })
        init_res = self._read()
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def call_tool(self, name: str, arguments: Optional[dict] = None) -> dict:
        self.msg_id += 1
        req = {
            "jsonrpc": "2.0",
            "id": self.msg_id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments or {}
            }
        }
        t0 = time.perf_counter()
        self._send(req)
        res = self._read()
        dt = time.perf_counter() - t0

        # Track session_id and selected_target_id if present
        res_data = res.get("result", {})
        structured = res_data.get("structuredContent", {})
        if structured:
            res_val = structured.get("result", {})
            if isinstance(res_val, dict):
                if "session_id" in res_val:
                    self.session_id = res_val["session_id"]
                if "selected_target_id" in res_val:
                    self.selected_target_id = res_val["selected_target_id"]
                if "state" in res_val and isinstance(res_val["state"], dict):
                    if "target_id" in res_val["state"]:
                        self.selected_target_id = res_val["state"]["target_id"]

        return {"result": res, "elapsed_s": dt}

    def close(self):
        try:
            self.proc.stdin.close()
            self.proc.terminate()
            self.proc.wait(timeout=5)
        except Exception:
            pass

def extract_structured(call_res: dict) -> (bool, Any, str):
    res_obj = call_res.get("result", {})
    if "result" in res_obj:
        inner = res_obj["result"]
        structured = inner.get("structuredContent", {})
        status = structured.get("status", "unknown")
        is_error = inner.get("isError", False) or status == "failed"

        if is_error:
            err_msg = structured.get("error", {}).get("message", "Unknown error")
            return False, structured, err_msg

        return True, structured.get("result", {}), status
    elif "error" in res_obj:
        return False, res_obj["error"], res_obj["error"].get("message", "JSON-RPC error")
    return False, res_obj, "Unknown response"

def run_suite():
    print("================================================================================")
    print("      KROMETRAIL FULL CAPABILITY, ERGONOMICS & STRESS TEST REPORT               ")
    print("================================================================================")

    client = KrometrailClient()
    metrics = {
        "passed": 0,
        "failed": 0,
        "operations": [],
        "errors": []
    }

    def record(name: str, call_res: dict, extra: str = ""):
        ok, res_data, msg = extract_structured(call_res)
        dt_ms = round(call_res["elapsed_s"] * 1000, 1)
        if ok:
            metrics["passed"] += 1
            metrics["operations"].append({"name": name, "status": "PASS", "dt_ms": dt_ms, "detail": extra or msg})
            print(f"  ✓ [PASS] {name:<36} ({dt_ms:7.1f}ms) {extra}")
        else:
            metrics["failed"] += 1
            metrics["operations"].append({"name": name, "status": "FAIL", "dt_ms": dt_ms, "detail": msg})
            print(f"  ✗ [FAIL] {name:<36} ({dt_ms:7.1f}ms) ERROR: {msg}")

    try:
        # -------------------------------------------------------------------------
        # PHASE 0: LIFECYCLE & DISCOVERY
        # -------------------------------------------------------------------------
        print("\n[PHASE 0] Lifecycle, Supervised Browser Launch & Identity Discovery")
        res = client.call_tool("start_browser", {"profile": "temporary", "focus": "foreground"})
        record("start_browser", res, f"Session: {client.session_id}, Target: {client.selected_target_id}")

        res = client.call_tool("browser_status")
        record("browser_status", res, f"Supervised Target: {client.selected_target_id}")

        res = client.call_tool("list_managed_profiles")
        ok, data, _ = extract_structured(res)
        record("list_managed_profiles", res, f"Profiles found: {len(data) if isinstance(data, list) else 1}")

        # -------------------------------------------------------------------------
        # PHASE 1: TARGET 1 - WIKIPEDIA (RUST PROGRAMMING LANGUAGE) - HEAVY DOM & TEXT
        # -------------------------------------------------------------------------
        print("\n[PHASE 1] Target 1: Wikipedia - Heavy DOM, Accessibility & Semantic Resolution")
        url_wiki = "https://en.wikipedia.org/wiki/Rust_(programming_language)"
        res = client.call_tool("navigate_page", {"url": url_wiki})
        record("navigate_page (Wikipedia)", res, f"Navigated to {url_wiki[:40]}...")

        res = client.call_tool("inspect_page")
        ok, data, _ = extract_structured(res)
        record("inspect_page", res, f"Title: '{data.get('title', '')[:35]}...'")

        res = client.call_tool("snapshot_page", {"anchor": "viewport"})
        ok, data, _ = extract_structured(res)
        targets = data.get("targets", []) if isinstance(data, dict) else []
        record("snapshot_page (viewport)", res, f"Actionable target elements: {len(targets)}")

        res = client.call_tool("query_page", {"query": {"kind": "text", "text": {"value": "Rust"}}})
        ok, data, _ = extract_structured(res)
        matches = data.get("matches", []) if isinstance(data, dict) else []
        record("query_page (semantic text 'Rust')", res, f"Exact semantic matches: {len(matches)}")

        # Viewport presets & responsive layout emulation
        print("\n  Sub-suite: Viewport Presets & Responsive Ergonomics")
        res = client.call_tool("set_viewport", {"viewport": {"mode": "preset", "preset": "mobile_phone"}})
        record("set_viewport (preset mobile_phone)", res)

        res = client.call_tool("set_viewport", {"viewport": {"mode": "preset", "preset": "responsive_tablet"}})
        record("set_viewport (preset tablet)", res)

        res = client.call_tool("set_viewport", {"viewport": {"mode": "preset", "preset": "responsive_desktop"}})
        record("set_viewport (preset desktop)", res)

        res = client.call_tool("set_viewport", {"viewport": {"mode": "override", "metrics": {"width": 1440, "height": 900, "device_scale_factor": 2.0, "mobile": False, "touch": False}}})
        record("set_viewport (custom 1440x900@2x)", res)

        res = client.call_tool("set_viewport", {"viewport": {"mode": "clear"}})
        record("set_viewport (clear to default)", res)

        # Scrolling, Screenshots, and In-Page JavaScript Evaluation
        print("\n  Sub-suite: Scrolling, Screenshot Pipeline & In-Page Evaluation")
        res = client.call_tool("scroll", {"delta": {"kind": "by_offset", "value": {"dx": 0, "dy": 800}}})
        record("scroll (by_offset +800px)", res)

        res = client.call_tool("take_screenshot", {"target": {"kind": "viewport"}, "format": "png"})
        ok, data, _ = extract_structured(res)
        img_len = len(data.get("image", {}).get("data", "")) if isinstance(data, dict) else 0
        record("take_screenshot (viewport PNG)", res, f"PNG base64 length: {img_len} bytes")

        res = client.call_tool("take_screenshot", {"target": {"kind": "viewport"}, "format": "jpeg", "jpeg_quality": 80})
        ok, data, _ = extract_structured(res)
        img_len = len(data.get("image", {}).get("data", "")) if isinstance(data, dict) else 0
        record("take_screenshot (viewport JPEG 80)", res, f"JPEG base64 length: {img_len} bytes")

        res = client.call_tool("evaluate_page", {"expression": "document.querySelectorAll('h2').length"})
        ok, data, _ = extract_structured(res)
        record("evaluate_page (count H2 headings)", res, f"H2 count: {data.get('value')}")

        # -------------------------------------------------------------------------
        # PHASE 2: TARGET 2 - HACKER NEWS (DYNAMIC FEEDS, HISTORY, ATOMIC BATCHING)
        # -------------------------------------------------------------------------
        print("\n[PHASE 2] Target 2: Hacker News - Rapid Navigation, History & Batching")
        url_hn = "https://news.ycombinator.com/"
        res = client.call_tool("navigate_page", {"url": url_hn})
        record("navigate_page (Hacker News)", res)

        res = client.call_tool("snapshot_page", {"anchor": "document"})
        ok, data, _ = extract_structured(res)
        targets = data.get("targets", []) if isinstance(data, dict) else []
        record("snapshot_page (HN document)", res, f"Actionable references: {len(targets)}")

        # Interaction click
        res = client.call_tool("click", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "a.storylink, span.titleline a"}}})
        record("click (story link)", res)

        # History Traversal
        res = client.call_tool("go_back")
        record("go_back (history traversal)", res)

        res = client.call_tool("go_forward")
        record("go_forward (history traversal)", res)

        res = client.call_tool("reload_page")
        record("reload_page (live evidence)", res)

        res = client.call_tool("list_page_assets")
        ok, data, _ = extract_structured(res)
        assets = data.get("assets", []) if isinstance(data, dict) else []
        record("list_page_assets (timing metrics)", res, f"Sanitized asset timing entries: {len(assets)}")

        res = client.call_tool("list_frames")
        ok, data, _ = extract_structured(res)
        frames = data.get("frames", []) if isinstance(data, dict) else []
        record("list_frames (generation frames)", res, f"Frame contexts: {len(frames)}")

        res = client.call_tool("list_page_contexts")
        ok, data, _ = extract_structured(res)
        contexts = data.get("contexts", []) if isinstance(data, dict) else []
        record("list_page_contexts (popup cursor)", res, f"Tracked page contexts: {len(contexts)}")

        # Atomic Batching
        print("\n  Sub-suite: High-Performance Multi-Step Atomic Batching")
        batch_payload = {
            "steps": [
                {"operation": "scroll", "request": {"delta": {"kind": "by_offset", "value": {"dx": 0, "dy": 350}}}},
                {"operation": "evaluate_page", "request": {"expression": "window.scrollY"}},
                {"operation": "scroll", "request": {"delta": {"kind": "by_offset", "value": {"dx": 0, "dy": -350}}}},
                {"operation": "evaluate_page", "request": {"expression": "window.scrollY"}}
            ],
            "timeout": 5000
        }
        res = client.call_tool("batch", batch_payload)
        ok, data, _ = extract_structured(res)
        results = data.get("results", []) if isinstance(data, dict) else []
        record("batch (4 sequential operations)", res, f"Executed batch steps: {len(results)}")

        # -------------------------------------------------------------------------
        # PHASE 3: TARGET 3 - INTERACTIVE FORMS, KEYBOARDS & LIVE OBSERVATION
        # -------------------------------------------------------------------------
        print("\n[PHASE 3] Target 3: Httpbin Forms - Form Controls, Keyboard & Compound Observation")
        url_form = "https://httpbin.org/forms/post"
        res = client.call_tool("navigate_page", {"url": url_form})
        record("navigate_page (httpbin form)", res)

        res = client.call_tool("fill", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "input[name='custname']"}}, "value": "Jane Doe"})
        record("fill (customer name input)", res)

        res = client.call_tool("fill", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "input[name='custtel']"}}, "value": "+1-800-555-0199"})
        record("fill (telephone input)", res)

        res = client.call_tool("fill", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "input[name='custemail']"}}, "value": "jane.doe@krometrail.dev"})
        record("fill (email input)", res)

        res = client.call_tool("click", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "input[value='medium']"}}})
        record("click (radio option medium)", res)

        res = client.call_tool("click", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "input[value='bacon']"}}})
        record("click (checkbox topping bacon)", res)

        res = client.call_tool("click", {"locator": {"kind": "element", "value": {"kind": "css_selector", "value": "input[value='cheese']"}}})
        record("click (checkbox topping cheese)", res)

        res = client.call_tool("press_keys", {"keys": [{"key": "Tab"}]})
        record("press_keys (chord navigation)", res)

        res = client.call_tool("observe_live")
        ok, data, _ = extract_structured(res)
        has_state = "state" in data
        has_snap = "snapshot" in data
        has_scr = "screenshot" in data
        record("observe_live (state + snapshot + screenshot)", res, f"State: {has_state}, Snapshot: {has_snap}, Screenshot: {has_scr}")

        # -------------------------------------------------------------------------
        # PHASE 4: MULTI-TAB SUPERVISION & CONTEXT MANAGEMENT
        # -------------------------------------------------------------------------
        print("\n[PHASE 4] Multi-Tab Lifecycle & Target Supervision")
        res = client.call_tool("create_page", {"url": "https://developer.mozilla.org/en-US/"})
        ok, data, _ = extract_structured(res)
        new_target_id = data.get("state", {}).get("target_id") if isinstance(data, dict) else None
        record("create_page (new tab MDN)", res, f"Spawned target: {new_target_id}")

        res = client.call_tool("list_pages")
        ok, data, _ = extract_structured(res)
        record("list_pages (all supervised tabs)", res, f"Active supervised page count: {len(data) if isinstance(data, list) else 1}")

        if client.selected_target_id:
            res = client.call_tool("select_page", {"target_id": client.selected_target_id})
            record("select_page (switch to primary tab)", res)

            res = client.call_tool("activate_page", {"target_id": client.selected_target_id})
            record("activate_page (bring tab to foreground)", res)

        if new_target_id:
            res = client.call_tool("close_page", {"target": {"selection": "target", "target_id": new_target_id}})
            record("close_page (close spawned tab)", res)

        # -------------------------------------------------------------------------
        # PHASE 5: TEMPORAL VISUAL EVIDENCE ENGINE (CORE DIFFERENTIATOR)
        # -------------------------------------------------------------------------
        print("\n[PHASE 5] Temporal Visual Evidence Engine (Artifacts, Filmstrips, Video, Event Streams)")
        range_req = {
            "anchor": {
                "anchor": "latest_interaction",
                "session_id": client.session_id,
                "target_id": client.selected_target_id,
                "window": {"before_ms": 3000, "after_ms": 3000}
            },
            "retention": "allow_partial",
            "capture_gaps": "include"
        }
        res = client.call_tool("resolve_temporal_range", range_req)
        ok, data, _ = extract_structured(res)
        range_handle = data.get("range_handle") if isinstance(data, dict) else None
        record("resolve_temporal_range (latest_interaction)", res, f"Resolved handle: {range_handle}")

        if range_handle:
            # 1. Source frames
            res = client.call_tool("list_source_frames", {"range_handle": range_handle})
            ok, data, _ = extract_structured(res)
            frames = data.get("frames", []) if isinstance(data, dict) else []
            record("list_source_frames", res, f"Retained source frames in range: {len(frames)}")

            # 2. Query Chronological Browser Events
            res = client.call_tool("query_browser_events", {"range_handle": range_handle})
            ok, data, _ = extract_structured(res)
            events = data.get("events", []) if isinstance(data, dict) else []
            record("query_browser_events", res, f"Captured CDP browser events: {len(events)}")

            # 3. Retention Pinning & Querying
            res = client.call_tool("pin_resolved_range", {"range_handle": range_handle})
            record("pin_resolved_range (protect segments)", res)

            res = client.call_tool("query_pin_state", {"range_handle": range_handle})
            ok, data, _ = extract_structured(res)
            record("query_pin_state", res, f"Pin state: {data.get('pin_status') if isinstance(data, dict) else 'active'}")

            res = client.call_tool("unpin_resolved_range", {"range_handle": range_handle})
            record("unpin_resolved_range (release pin)", res)

            # 4. Generate Multi-Artifact Visuals (Filmstrip & Difference Map)
            res = client.call_tool("generate_artifacts", {
                "range_handle": range_handle,
                "generators": [
                    {"generator": "filmstrip", "tile_limit": 6},
                    {"generator": "difference_map", "sampling": "uniform_bounded"}
                ]
            })
            ok, data, _ = extract_structured(res)
            artifacts = data.get("artifacts", []) if isinstance(data, dict) else []
            record("generate_artifacts (filmstrip + diffmap)", res, f"Generated visual artifacts: {len(artifacts)}")

            # 5. Generate Region Filmstrip
            res = client.call_tool("generate_region_filmstrip", {
                "range_handle": range_handle,
                "region": {
                    "coordinate_space": "fixed_source_image",
                    "rect": {"x": 50, "y": 50, "width": 400, "height": 300}
                },
                "tile_limit": 6
            })
            ok, data, _ = extract_structured(res)
            record("generate_region_filmstrip (fixed ROI)", res)

            # 6. Generate Temporal Video (MP4 / H.264 clip)
            res = client.call_tool("generate_temporal_video", {
                "range_handle": range_handle,
                "policy": "model_optimized",
                "output": {"max_width": 1280, "max_height": 720, "max_encoded_bytes": 10485760}
            })
            ok, data, _ = extract_structured(res)
            has_video = "video" in data or "manifest" in data
            record("generate_temporal_video (MP4 H.264)", res, f"Video generated: {has_video}")

            # 7. Temporal Debug Bundle (All-in-one Diagnostic)
            res = client.call_tool("temporal_debug_bundle", {
                "anchor": {
                    "anchor": "latest_interaction",
                    "session_id": client.session_id,
                    "target_id": client.selected_target_id,
                    "window": {"before_ms": 1500, "after_ms": 1500}
                },
                "retention": "allow_partial",
                "capture_gaps": "include",
                "caller_markers": [],
                "orientation": "include"
            })
            ok, data, _ = extract_structured(res)
            record("temporal_debug_bundle (all-in-one)", res, f"Bundle sections: {list(data.keys()) if isinstance(data, dict) else 'succeeded'}")

        # -------------------------------------------------------------------------
        # PHASE 6: STRESS TESTING & HIGH-THROUGHPUT BURSTS
        # -------------------------------------------------------------------------
        print("\n[PHASE 6] Stress Testing: High-Throughput Rapid Operation Bursts")
        burst_count = 30
        t_burst_start = time.perf_counter()
        burst_errors = 0
        for i in range(burst_count):
            if i % 3 == 0:
                r = client.call_tool("scroll", {"delta": {"kind": "by_offset", "value": {"dx": 0, "dy": 40 if i % 2 == 0 else -40}}})
            elif i % 3 == 1:
                r = client.call_tool("evaluate_page", {"expression": f"window.scrollY + {i}"})
            else:
                r = client.call_tool("take_screenshot", {"target": {"kind": "viewport"}, "format": "jpeg", "jpeg_quality": 40})
            ok, _, _ = extract_structured(r)
            if not ok:
                burst_errors += 1
        t_burst_end = time.perf_counter()
        burst_dt = t_burst_end - t_burst_start
        throughput = burst_count / burst_dt
        record(f"burst_{burst_count}_ops_stress", {"elapsed_s": burst_dt, "result": {"result": {"structuredContent": {"status": "succeeded" if burst_errors == 0 else "failed"}}}}, f"{throughput:.1f} ops/sec, {burst_errors} errors")

        # -------------------------------------------------------------------------
        # PHASE 7: TEARDOWN & RECOVERY
        # -------------------------------------------------------------------------
        print("\n[PHASE 7] Teardown & Clean Process Termination")
        res = client.call_tool("stop_browser")
        record("stop_browser", res, "Graceful browser termination and port cleanup")

    except Exception as e:
        print(f"\n[FATAL UNHANDLED EXCEPTION]: {e}")
        traceback.print_exc()
        metrics["errors"].append(str(e))
    finally:
        client.close()

    print("\n================================================================================")
    print(f"  EXECUTION SUMMARY: {metrics['passed']} PASSED, {metrics['failed']} FAILED")
    print("================================================================================")

    with open("/tmp/krometrail_stress_results.json", "w") as f:
        json.dump(metrics, f, indent=2)

if __name__ == "__main__":
    run_suite()
