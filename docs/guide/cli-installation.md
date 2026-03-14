---
title: CLI Installation
description: Install the krometrail CLI via npm, bunx, or as a standalone binary.
---

# CLI Installation

The Krometrail CLI exposes the same capabilities as the MCP tools, but as shell commands. Many agent harnesses prefer CLI tools for their transparency and composability — the CLI gives agents explicit, scriptable access to the full debugging toolset. It's also handy for convenience operations like `krometrail doctor`. Sessions persist across commands via a background daemon.

## Options

### bunx / npx (no install)

Run without installing globally. Useful for one-off use or CI:

```bash
bunx krometrail doctor
npx krometrail doctor
```

### npm global install

```bash
npm install -g krometrail
krometrail --version
```

### Standalone binary

Install with a single command — no Node.js or Bun required:

```bash
curl -fsSL https://krometrail.dev/install.sh | sh
```

The installer detects your OS and architecture, downloads the correct binary from GitHub releases, verifies the SHA-256 checksum, and adds it to your PATH.

**Install a specific version:**

```bash
curl -fsSL https://krometrail.dev/install.sh | sh -s -- --version v0.2.0
```

**Custom install directory:**

```bash
KROMETRAIL_INSTALL_DIR=/usr/local/bin curl -fsSL https://krometrail.dev/install.sh | sh
```

**Skip PATH modification:**

```bash
curl -fsSL https://krometrail.dev/install.sh | sh -s -- --no-modify-path
```

Or download a specific binary directly from [GitHub releases](https://github.com/nklisch/krometrail/releases):

```bash
# Linux x64 (example)
curl -L https://github.com/nklisch/krometrail/releases/latest/download/krometrail-linux-x64 \
  -o ~/.local/bin/krometrail
chmod +x ~/.local/bin/krometrail
```

Binaries are available for Linux (x64, arm64), macOS (x64, Apple Silicon), and Windows.

### Build from source

Requires [Bun](https://bun.sh):

```bash
git clone https://github.com/nklisch/krometrail
cd krometrail
bun install
bun run build        # Linux/macOS binary
bun run build:all    # All platforms
```

The compiled binary is at `dist/krometrail`.

## Verify

```bash
krometrail --version
krometrail doctor
```

`doctor` shows which language debuggers are installed. Install missing debuggers for the languages you need:

| Language | Install command |
|----------|----------------|
| Python | `pip install debugpy` |
| Go | `go install github.com/go-delve/delve/cmd/dlv@latest` |
| Java | Download java-debug-adapter |
| C/C++ | `sudo apt install gdb` (GDB 14+) or install lldb-dap |

Node.js and Rust (CodeLLDB) adapters download their debuggers automatically on first use.

## Session Daemon

The CLI manages sessions via a background daemon that starts automatically on `krometrail debug launch` and shuts down after all sessions end. This means consecutive commands share a session:

```bash
krometrail debug launch "python app.py" --break order.py:147
# daemon starts, session created

krometrail debug continue
# same session, no server lifecycle to manage

krometrail debug stop
# session ends, daemon idles then shuts down
```

The daemon socket lives at `$XDG_RUNTIME_DIR/krometrail.sock` or `~/.krometrail/krometrail.sock`.
