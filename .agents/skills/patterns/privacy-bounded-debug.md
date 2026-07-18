# Privacy-Bounded Debug Projections

Secret-bearing types implement explicit `Debug` projections containing safe identities, counts, states, or digests instead of raw user or process content.

## Rationale

Rust diagnostics, assertion failures, and tracing frequently format values with `Debug`. Clipboard text, filenames, URLs, resource URIs, and subprocess output must remain private even when a caller accidentally logs the containing type.

## Examples

### Clipboard writes expose byte count, not text

**File**: `crates/krometrail-core/src/browser/local_io.rs:45`

```rust
f.debug_struct("WriteClipboardRequest")
    .field("target", &self.target)
    .field("utf8_bytes", &self.text.len())
    .finish()
```

### Downloads omit filename, URL, and resource URI

**File**: `crates/krometrail-core/src/browser/local_io.rs:123`

```rust
f.debug_struct("ManagedDownload")
    .field("id", &self.id)
    .field("state", &self.state)
    .field("received_bytes", &self.received_bytes)
    .field("has_resource", &self.resource_uri.is_some())
    .finish()
```

### Process diagnostics expose lengths and a digest

**File**: `crates/krometrail-ffmpeg/src/process.rs:40`

```rust
formatter.debug_struct("SanitizedProcessOutcome")
    .field("stdout_bytes", &self.stdout.len())
    .field("stderr_bytes", &self.stderr.len())
    .field("diagnostic_sha256", &HexDigest(self.diagnostic_sha256))
    .finish()
```

## When to Use

- Types containing clipboard/page text, filenames, URLs, paths, resource URIs, credentials, or raw process output.
- Values likely to appear in tracing, errors, assertions, or test failures.
- Diagnostics where counts, booleans, identities, or digests answer operational questions safely.

## When NOT to Use

- Types composed entirely of public, sanitized values.
- Cases where suppressing a field hides a security-relevant state; expose a safe derived indicator instead.
- As a substitute for sanitizing explicit log fields and errors.

## Common Violations

- Deriving `Debug` on a type that owns sensitive strings or bytes.
- Redacting `Display` while leaving `Debug` raw.
- Logging a sensitive field separately after defining a safe projection.
- Omitting all diagnostic state instead of safe counts or flags.
- Testing one nested type while containers still derive raw output.
