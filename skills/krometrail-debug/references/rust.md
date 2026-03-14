# Rust Debugging

## Prerequisites

- `cargo` available in PATH
- CodeLLDB v1.12.1 (auto-downloaded to `~/.krometrail/adapters/codelldb/` on first use)
- Verify: `cargo --version`

## Launch examples

```
# cargo run (auto-builds with debug info)
debug_launch({ command: "cargo run" })

# cargo test
debug_launch({ command: "cargo test" })

# cargo test specific
debug_launch({ command: "cargo test test_discount" })

# Pre-built binary
debug_launch({ command: "./target/debug/myapp" })
```

## Attach to running process

Attach by PID:

```
debug_attach({ language: "rust", pid: 12345 })
```

## Tips

- `cargo run` and `cargo build` auto-build before debugging
- `cargo test` uses `--no-run` to locate the test binary, then launches it
- Binary name is resolved via `cargo metadata` (falls back to package directory name)
- CodeLLDB is platform-aware and auto-downloads the correct variant
- Ensure debug symbols are included (the default `dev` profile includes them)
