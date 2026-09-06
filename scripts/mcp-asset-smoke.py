#!/usr/bin/env python3
"""Probe an explicit staged/released command, never a PATH-selected substitute.
Usage: python3 scripts/mcp-asset-smoke.py --version 1.7.0 -- /exact/krometrail mcp
"""
import argparse
import json
import os
import queue
import subprocess
import tempfile
import threading


def probe(command, version, protocol):
    with tempfile.TemporaryDirectory(prefix="krometrail-asset-") as root:
        env = dict(os.environ, KROMETRAIL_DATA_DIR=root, KROMETRAIL_FFMPEG_PATH=os.path.join(root, "missing-ffmpeg"))
        with tempfile.TemporaryFile() as errors:
            child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=errors, env=env)
            replies = queue.Queue()

            def read():
                try:
                    for line in child.stdout:
                        replies.put(json.loads(line))
                except Exception as error:
                    replies.put(error)
                finally:
                    replies.put(EOFError("MCP stdout closed"))

            reader = threading.Thread(target=read, daemon=True)
            reader.start()
            request_id = 0

            def request(method, params):
                nonlocal request_id
                request_id += 1
                if protocol == "2026-07-28":
                    params["_meta"] = {"io.modelcontextprotocol/protocolVersion": protocol, "io.modelcontextprotocol/clientCapabilities": {}}
                child.stdin.write((json.dumps({"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}) + "\n").encode())
                child.stdin.flush()
                reply = replies.get(timeout=30)
                if isinstance(reply, Exception):
                    raise reply
                assert reply["id"] == request_id and "result" in reply, reply
                return reply["result"]

            try:
                if protocol == "2026-07-28":
                    info = request("server/discover", {})
                    assert info["supportedVersions"] == ["2026-07-28", "2025-11-25", "2025-06-18"]
                    assert info["ttlMs"] == 0 and info["cacheScope"] == "private"
                    identity = info["_meta"]["io.modelcontextprotocol/serverInfo"]
                else:
                    info = request("initialize", {"protocolVersion": protocol, "capabilities": {}, "clientInfo": {"name": "asset-qualification", "version": "1"}})
                    assert info["protocolVersion"] == protocol
                    identity = info["serverInfo"]
                    child.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
                    child.stdin.flush()
                assert identity["name"] == "krometrail" and identity["version"] == version, identity
                names, pages, maximum = [], 0, 0
                params = {}
                while True:
                    page = request("tools/list", params)
                    size = len(json.dumps(page, separators=(",", ":"), ensure_ascii=False).encode())
                    maximum = max(maximum, size)
                    if protocol == "2026-07-28":
                        assert size <= 192 * 1024 and len(page["tools"]) <= 8
                        assert page["resultType"] == "complete" and page["ttlMs"] == 60000 and page["cacheScope"] == "private"
                    else:
                        assert "resultType" not in page and "ttlMs" not in page and "cacheScope" not in page
                        assert "nextCursor" not in page and pages == 0
                    names.extend(tool["name"] for tool in page["tools"])
                    pages += 1
                    assert pages < 1000, "catalogue cursor did not terminate"
                    if "nextCursor" not in page:
                        break
                    params = {"cursor": page["nextCursor"]}
                assert names and names == sorted(set(names))
                assert "start_browser" in names and "temporal_debug_bundle" in names
                child.stdin.close()
                assert child.wait(timeout=12) == 0
                reader.join(timeout=2)
                assert not reader.is_alive()
                trailing = replies.get(timeout=1)
                assert isinstance(trailing, EOFError), "unexpected trailing stdout"
                return {"protocol": protocol, "version": version, "tool_count": len(names), "pages": pages, "max_page_bytes": maximum}
            finally:
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=5)
                if child.stdin and not child.stdin.closed:
                    child.stdin.close()
                child.stdout.close()
                reader.join(timeout=2)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--version", required=True)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command[1:] if args.command[:1] == ["--"] else args.command
    if not command:
        parser.error("an explicit executable command is required")
    for protocol in ["2026-07-28", "2025-11-25", "2025-06-18"]:
        print(json.dumps(probe(command, args.version, protocol)), flush=True)


if __name__ == "__main__":
    main()
