#!/usr/bin/env bash
# Hermetic managed plugin bootstrap and update fixtures. No network, Chrome, or model calls.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SOURCE_PLUGIN="$ROOT/plugin"
STATE="$(mktemp -d)"
cleanup() { rm -rf "$STATE"; }
trap cleanup EXIT

fail() {
  echo "plugin bootstrap fixture failure: $*" >&2
  exit 1
}

make_release() {
  local version="$1"
  local dir="$STATE/releases/$version"
  mkdir -p "$dir"
  cat >"$dir/krometrail-linux-x64" <<EOF
#!/bin/sh
if [ "\${1:-}" = "--version" ]; then
  printf 'krometrail $version\\n'
  exit 0
fi
printf 'managed-$version:%s\\n' "\$*"
EOF
  chmod 700 "$dir/krometrail-linux-x64"
  sha256sum "$dir/krometrail-linux-x64" | sed 's#  .*#  krometrail-linux-x64#' >"$dir/checksums.txt"
}

mkdir -p "$STATE/fake-bin" "$STATE/releases"
make_release 1.0.0
make_release 1.0.1

cat >"$STATE/fake-bin/uname" <<'EOF'
#!/bin/sh
case "${1:-}" in
  -s|'') printf 'Linux\n' ;;
  -m) printf 'x86_64\n' ;;
  *) exit 1 ;;
esac
EOF
chmod +x "$STATE/fake-bin/uname"

cat >"$STATE/fake-bin/curl" <<'EOF'
#!/bin/sh
set -eu
output=""
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --output) output="$2"; shift 2 ;;
    --write-out) shift 2 ;;
    --connect-timeout|--max-time|--max-filesize|--user-agent|--proto|--proto-redir) shift 2 ;;
    --silent|--show-error|--max-redirs) 
      if [ "$1" = "--max-redirs" ]; then shift 2; else shift; fi
      ;;
    *) url="$1"; shift ;;
  esac
done
printf '%s\n' "$url" >>"$NETWORK_LOG"
if [ "${FAKE_CURL_MODE:-success}" = "fail" ]; then
  exit 22
fi
if [ "${FAKE_CURL_MODE:-success}" = "untrusted-redirect" ]; then
  printf '302\nhttps://evil.example.invalid/payload\n'
  exit 0
fi
version=$(printf '%s' "$url" | sed -n 's#.*releases/download/v\([0-9][0-9.]*\)/.*#\1#p')
[ -n "$version" ] || exit 23
case "$url" in
  */checksums.txt) source="$FIXTURE_RELEASES/$version/checksums.txt" ;;
  */krometrail-linux-x64) source="$FIXTURE_RELEASES/$version/krometrail-linux-x64" ;;
  *) exit 24 ;;
esac
[ -f "$source" ] || exit 25
cp "$source" "$output"
if [ "${FAKE_CURL_MODE:-success}" = "bad-checksum" ] && [ "${url##*/}" = "checksums.txt" ]; then
  sed 's/^[0-9a-f]*/ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff/' "$output" >"$output.bad"
  mv "$output.bad" "$output"
fi
printf '200\n\n'
EOF
chmod +x "$STATE/fake-bin/curl"

copy_plugin() {
  local version="$1"
  local destination="$2"
  cp -R "$SOURCE_PLUGIN" "$destination"
  printf '%s\n' "$version" >"$destination/version"
  chmod +x "$destination/bin/krometrail" "$destination/scripts/install-managed.sh"
}

run_launcher() {
  local plugin="$1"
  local managed="$2"
  shift 2
  PATH="$STATE/fake-bin:$PATH" \
    NETWORK_LOG="$STATE/network.log" \
    FIXTURE_RELEASES="$STATE/releases" \
    KROMETRAIL_MANAGED_ROOT="$managed" \
    "$plugin/bin/krometrail" "$@"
}

: >"$STATE/network.log"
copy_plugin 1.0.0 "$STATE/plugin-100"
managed="$STATE/managed"
cold_stdout=$(run_launcher "$STATE/plugin-100" "$managed" mcp 2>"$STATE/cold.stderr")
[[ "$cold_stdout" == 'managed-1.0.0:mcp' ]] || fail "cold start leaked bootstrap output or ran the wrong binary"
grep -Fq 'installing managed release v1.0.0' "$STATE/cold.stderr" || fail "cold start did not report bootstrap on stderr"
[[ "$(wc -l <"$STATE/network.log" | tr -d ' ')" -eq 2 ]] || fail "cold start must fetch one asset and one checksum file"
[[ "$("$managed/versions/1.0.0/krometrail" --version)" == 'krometrail 1.0.0' ]] || fail "cold start did not publish v1.0.0"
[[ "$(stat -c '%a' "$managed/versions/1.0.0/krometrail")" == '700' ]] || fail "managed binary is not private"

# A matching release must start offline with no downloader invocation.
cp "$STATE/network.log" "$STATE/network.before"
FAKE_CURL_MODE=fail warm_stdout=$(run_launcher "$STATE/plugin-100" "$managed" probe 2>"$STATE/warm.stderr")
[[ "$warm_stdout" == 'managed-1.0.0:probe' ]] || fail "warm offline start failed"
cmp -s "$STATE/network.log" "$STATE/network.before" || fail "warm start performed network work"
[[ ! -s "$STATE/warm.stderr" ]] || fail "warm start emitted bootstrap diagnostics"

# A new plugin version installs alongside the old release and selects itself.
copy_plugin 1.0.1 "$STATE/plugin-101"
update_stdout=$(run_launcher "$STATE/plugin-101" "$managed" mcp 2>"$STATE/update.stderr")
[[ "$update_stdout" == 'managed-1.0.1:mcp' ]] || fail "plugin update did not select v1.0.1"
[[ -x "$managed/versions/1.0.0/krometrail" && -x "$managed/versions/1.0.1/krometrail" ]] || fail "plugin update replaced an existing release"

# Concurrent cold starts can only converge on the same verified artifact.
concurrent="$STATE/concurrent-managed"
run_launcher "$STATE/plugin-101" "$concurrent" one >"$STATE/concurrent.one" 2>"$STATE/concurrent.one.err" &
pid_one=$!
run_launcher "$STATE/plugin-101" "$concurrent" two >"$STATE/concurrent.two" 2>"$STATE/concurrent.two.err" &
pid_two=$!
wait "$pid_one"
wait "$pid_two"
[[ "$(cat "$STATE/concurrent.one")" == 'managed-1.0.1:one' ]] || fail "first concurrent launcher failed"
[[ "$(cat "$STATE/concurrent.two")" == 'managed-1.0.1:two' ]] || fail "second concurrent launcher failed"
[[ "$("$concurrent/versions/1.0.1/krometrail" --version)" == 'krometrail 1.0.1' ]] || fail "concurrent publication produced the wrong identity"

# A failed update preserves the already installed version and publishes no candidate.
failed="$STATE/failed-managed"
run_launcher "$STATE/plugin-100" "$failed" seed >/dev/null 2>"$STATE/seed.stderr"
if FAKE_CURL_MODE=bad-checksum run_launcher "$STATE/plugin-101" "$failed" mcp >"$STATE/fail.stdout" 2>"$STATE/fail.stderr"; then
  fail "bad checksum update succeeded"
fi
[[ "$("$failed/versions/1.0.0/krometrail" --version)" == 'krometrail 1.0.0' ]] || fail "failed update damaged the prior release"
[[ ! -e "$failed/versions/1.0.1/krometrail" ]] || fail "failed update published a candidate"
[[ ! -s "$STATE/fail.stdout" ]] || fail "failed bootstrap wrote to stdout"
grep -Fq 'checksum verification failed' "$STATE/fail.stderr" || fail "failed update did not explain checksum rejection"

# Unsafe destinations and redirects fail before publication.
mkdir "$STATE/real-root"
ln -s "$STATE/real-root" "$STATE/symlink-root"
if run_launcher "$STATE/plugin-100" "$STATE/symlink-root" mcp >/dev/null 2>"$STATE/symlink.stderr"; then
  fail "symlinked managed root was accepted"
fi
grep -Fq 'contains a symlink' "$STATE/symlink.stderr" || fail "symlink rejection was not explicit"

if FAKE_CURL_MODE=untrusted-redirect run_launcher "$STATE/plugin-100" "$STATE/redirect-root" mcp >/dev/null 2>"$STATE/redirect.stderr"; then
  fail "untrusted redirect was accepted"
fi
grep -Fq 'untrusted host' "$STATE/redirect.stderr" || fail "redirect rejection was not explicit"

printf 'plugin bootstrap fixtures: ok\n'
