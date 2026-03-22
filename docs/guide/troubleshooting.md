---
title: "Troubleshooting"
description: "Common issues and solutions for Krometrail debugging and browser observation."
---

# Troubleshooting Krometrail

Start with `krometrail doctor` to check your setup before diving into specific issues.

## Debugger Not Found

### Python: debugpy not installed

**Symptom**: `AdapterPrerequisiteError: python: missing debugpy`

**Fix**:
```bash
pip install debugpy
# or
pip3 install debugpy
```

### Node.js: js-debug adapter download failed

**Symptom**: `LaunchError: Failed to download js-debug adapter` or missing adapter at `~/.krometrail/adapters/js-debug/`

**Fix**:
```bash
# Clear the cache and retry
rm -rf ~/.krometrail/adapters/js-debug/
krometrail debug launch "node app.js"  # triggers re-download
```

If the download fails due to network restrictions, download manually from the [js-debug releases](https://github.com/microsoft/vscode-js-debug/releases) and extract to `~/.krometrail/adapters/js-debug/`.

### Go: dlv not found

**Symptom**: `AdapterPrerequisiteError: go: missing dlv`

**Fix**:
```bash
go install github.com/go-delve/delve/cmd/dlv@latest
```

### Rust: CodeLLDB download failed

**Symptom**: `LaunchError: Failed to download CodeLLDB`

**Fix**:
```bash
rm -rf ~/.krometrail/adapters/codelldb/
krometrail debug launch "cargo run"  # triggers re-download
```

Also ensure `cargo` is installed: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Java: JDK not found or version < 17

**Symptom**: `AdapterPrerequisiteError: java: missing javac 17+`

**Fix**:
```bash
# Fedora/RHEL
sudo dnf install java-21-openjdk-devel

# Ubuntu/Debian
sudo apt-get install openjdk-21-jdk

# Arch
sudo pacman -S jdk21-openjdk

# macOS
brew install openjdk@21
```
Verify: `javac -version` should output `javac 17.x.x` or higher

### C/C++: GDB version < 14

**Symptom**: `AdapterPrerequisiteError: cpp: GDB X is too old`

**Fix**:
```bash
# Fedora/RHEL
sudo dnf install gdb

# Ubuntu/Debian
sudo apt-get install gdb

# Arch
sudo pacman -S gdb

# macOS (via Homebrew)
brew install gdb

# Alternative: install lldb-dap (macOS)
xcode-select --install
```

GDB must be version 14+ for DAP support. Check with: `gdb --version`

---

## Connection Issues

### Port conflicts

**Symptom**: `DAPConnectionError: ECONNREFUSED` or `LaunchError: Failed to connect to debugger on port XXXX`

**Fix**:
- Krometrail allocates random ports in the 4000–5000 range. If all are in use, wait and retry.
- Check for zombie debugger processes: `pkill -f debugpy` / `pkill -f dlv`

### Timeout on launch

**Symptom**: `DAPTimeoutError: DAP request timed out`

**Fix**:
- Increase timeout with `--timeout` flag (in ms): `krometrail debug continue --timeout 30000`
- Check if the debugger process started: `krometrail debug status`
- For slow machines or large programs, the debugger may need more time to initialize

### Debugger process crashed

**Symptom**: `LaunchError: gdb exited with code 1. stderr: ...`

**Fix**:
- Check the stderr output in the error message for the root cause
- Ensure the binary has debug symbols (compiled with `-g` for C/C++, not stripped)
- Run `krometrail doctor` to verify the debugger version

---

## Breakpoint Issues

### Breakpoint not hit (wrong file path)

**Symptom**: Session continues without stopping, or breakpoint shows as unverified

**Fix**:
- Use absolute paths for breakpoints: `krometrail debug break /full/path/to/file.py:25`
- Ensure the file path matches exactly — relative paths are resolved from the working directory
- For pytest: the breakpoint file should be the test file, not the module being tested

### Breakpoint set on non-executable line

**Symptom**: Breakpoint is adjusted to a different line, or never hit

**Fix**:
- Set breakpoints on lines with actual code, not:
  - Comment lines (`#`, `//`)
  - Blank lines
  - Decorator lines (`@pytest.mark.parametrize(...)`)
  - Function/class declaration lines (`def foo():`, `class Bar:`)
- The debugger adjusts breakpoints to the nearest executable line automatically

### Conditional breakpoint syntax errors per language

- **Python**: Use Python expressions: `discount < 0`, `len(items) > 10`
- **Node.js/JavaScript**: Use JS expressions: `x < 0`, `arr.length > 10`
- **Go**: Use Go expressions: `x < 0`, `len(items) > 10`
- **C/C++**: Use C expressions: `x < 0`, `n > 10`

---

## Session Issues

### Session timeout (5-minute default)

**Symptom**: Session becomes unresponsive after inactivity

**Fix**: Sessions automatically expire after 5 minutes of inactivity. This is a safety measure to prevent orphaned debugger processes. Restart the session.

### Action limit reached

**Symptom**: `SessionLimitError: maxActionsPerSession`

**Fix**: This prevents runaway sessions. Stop the current session and start a new one. If you regularly need more actions, the limit can be adjusted in session manager configuration.

### Multiple sessions — using --session flag

**Symptom**: `Error: Multiple active sessions. Use --session to specify one`

**Fix**:
```bash
# List active sessions
krometrail debug status

# Target a specific session
krometrail debug eval "x" --session abc123
```

---

## Framework Detection Issues

### Framework not auto-detected

**Symptom**: Running `pytest tests/` doesn't configure the pytest adapter

**Fix**: Override with `--framework`:
```bash
krometrail debug launch "pytest tests/" --framework pytest
```

### Framework detection causes launch failure

**Symptom**: Launch fails with framework-related errors

**Fix**: Disable framework detection:
```bash
krometrail debug launch "python app.py" --framework none
```

Or for MCP: `debug_launch` with `framework: "none"`

---

## Performance Issues

### Slow launch times

- Python with debugpy: ~1-2s normal
- Node.js with js-debug: ~2-3s (first launch downloads adapter)
- Go with dlv: ~2-5s (includes compilation)
- Subsequent launches are faster after caching

### Large viewport output

The viewport is designed to be compact (~400 tokens). If it's growing:
- Use `--scope local` with vars to limit to local variables
- Set `stack_depth` in viewport config to limit stack frames shown

### Context window exhaustion — use progressive compression

Krometrail compresses older history automatically as sessions grow. The most recent stop always gets full detail. Older stops show summary information.

---

## Common Error Messages

### "Adapter not found for extension .xyz"

No adapter is registered for this file extension. Check `krometrail doctor` for supported languages. Use `--language` to force a specific adapter.

### "Session is in 'running' state, expected 'stopped'"

Most debugging commands (eval, vars, step) require the program to be paused. Use `krometrail debug continue` first to run to a breakpoint.

### "Failed to connect to debugger on port XXXX"

The debugger process started but isn't listening yet. This usually means:
1. The process crashed immediately — check `krometrail debug status` for stderr output
2. The program requires input before the debugger port opens
3. The port was blocked by a firewall

---

## Getting Help

1. Run `krometrail doctor` and include the output
2. Check daemon logs: `cat ~/.krometrail/daemon.log`
3. File an issue: https://github.com/nklisch/krometrail/issues
