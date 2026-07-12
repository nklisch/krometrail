#!/usr/bin/env bash
# Hermetic installer behavior tests. The fake curl client serves local fixture
# bytes only; these tests never contact the GitHub API or release host.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
INSTALLER="$ROOT/scripts/install.sh"

fail() {
	echo "installer fixture failure: $*" >&2
	exit 1
}

require_text() {
	local file="$1"
	local text="$2"
	grep -Fq -- "$text" "$file" || fail "${file#"$ROOT"} is missing: $text"
}

make_fixture() {
	local dir="$1"
	mkdir -p "$dir/bin" "$dir/home" "$dir/install"
	cat > "$dir/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

url=""
output=""
while [[ $# -gt 0 ]]; do
	case "$1" in
		--output)
			output="$2"
			shift 2
			;;
		*)
			url="$1"
			shift
			;;
	esac
done

printf '%s\n' "$url" >> "$CURL_LOG"
if [[ "$url" == *api.github.com/repos/nklisch/krometrail/releases/latest ]]; then
	[[ -z "$output" ]] || { echo "latest metadata must be fetched as text" >&2; exit 1; }
	printf '{"tag_name":"%s"}\n' "$FAKE_LATEST_TAG"
elif [[ "$url" == */checksums.txt ]]; then
	[[ -n "$output" ]] || { echo "checksums must be downloaded to a file" >&2; exit 1; }
	cp "$FIXTURE_CHECKSUMS" "$output"
elif [[ "$url" == */krometrail-linux-x64 ]]; then
	[[ -n "$output" ]] || { echo "binary must be downloaded to a file" >&2; exit 1; }
	cp "$FIXTURE_ARTIFACT" "$output"
else
	echo "unexpected URL: $url" >&2
	exit 1
fi
EOF
	chmod +x "$dir/bin/curl"
}

write_artifact() {
	local dir="$1"
	local mode="$2"
	printf '%s' "$3" > "$dir/artifact"
	chmod "$mode" "$dir/artifact"
	sha256sum "$dir/artifact" | sed 's#  .*#  krometrail-linux-x64#' > "$dir/checksums.txt"
}

run_installer() {
	local dir="$1"
	shift
	PATH="$dir/bin:$PATH" \
	HOME="$dir/home" \
	CURL_LOG="$dir/curl.log" \
	FAKE_LATEST_TAG="v0.2.20" \
	FIXTURE_ARTIFACT="$dir/artifact" \
	FIXTURE_CHECKSUMS="$dir/checksums.txt" \
	KROMETRAIL_INSTALL_DIR="$dir/install" \
	sh "$INSTALLER" --no-modify-path "$@"
}

# Latest resolution must stop at the immutable legacy boundary before an
# artifact URL is requested.
latest_dir="$(mktemp -d)"
explicit_dir=""
failure_dir=""
success_dir=""
trap 'rm -rf "$latest_dir" "$explicit_dir" "$failure_dir" "$success_dir"' EXIT
make_fixture "$latest_dir"
write_artifact "$latest_dir" 0644 'legacy bytes\n'
: > "$latest_dir/curl.log"
if run_installer "$latest_dir" > "$latest_dir/output" 2>&1; then
	fail "latest legacy release was accepted"
fi
require_text "$latest_dir/output" 'immutable legacy TypeScript/DAP boundary'
require_text "$latest_dir/curl.log" 'https://api.github.com/repos/nklisch/krometrail/releases/latest'
if grep -Fq -- '/releases/download/' "$latest_dir/curl.log"; then
	fail "latest legacy resolution downloaded an artifact"
fi
if [[ -e "$latest_dir/install/krometrail" ]]; then
	fail "latest legacy resolution created an installation"
fi

# Explicit legacy versions must be rejected without making any HTTP request.
explicit_dir="$(mktemp -d)"
make_fixture "$explicit_dir"
write_artifact "$explicit_dir" 0644 'legacy bytes\n'
for legacy_version in v0.2.20 v0.2.19 v0.1.99; do
	: > "$explicit_dir/curl.log"
	if run_installer "$explicit_dir" --version "$legacy_version" > "$explicit_dir/output" 2>&1; then
		fail "explicit legacy release ${legacy_version} was accepted"
	fi
	require_text "$explicit_dir/output" "Release ${legacy_version} is blocked"
	if [[ -s "$explicit_dir/curl.log" ]]; then
		fail "explicit legacy rejection made a network request for ${legacy_version}"
	fi
done

# A checksum-valid artifact that cannot execute must not replace a working
# install. The source fixture is deliberately non-executable; chmod happens
# only on the temporary download path inside the installer.
failure_dir="$(mktemp -d)"
make_fixture "$failure_dir"
write_artifact "$failure_dir" 0644 'not an executable\n'
printf '#!/bin/sh\nprintf previous-install\n' > "$failure_dir/install/krometrail"
chmod +x "$failure_dir/install/krometrail"
cp "$failure_dir/install/krometrail" "$failure_dir/previous"
: > "$failure_dir/curl.log"
if run_installer "$failure_dir" --version v0.2.21 > "$failure_dir/output" 2>&1; then
	fail "non-executable artifact was accepted"
fi
cmp -s "$failure_dir/install/krometrail" "$failure_dir/previous" || fail "failed validation replaced the prior installation"
if grep -Fq -- 'Installed krometrail' "$failure_dir/output"; then
	fail "failed validation printed an install success message"
fi
for leftover in "$failure_dir/install"/krometrail.??????; do
	if [[ -e "$leftover" ]]; then
		fail "failed validation left temporary file $leftover"
	fi
done

# A synthetic post-cutoff Rust release must install and validate successfully,
# while explicit selection avoids the GitHub latest endpoint entirely.
success_dir="$(mktemp -d)"
make_fixture "$success_dir"
write_artifact "$success_dir" 0755 '#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf "krometrail 0.2.21 (rust)\\n"
  exit 0
fi
exit 1
'
: > "$success_dir/curl.log"
if ! run_installer "$success_dir" --version v0.2.21 > "$success_dir/output" 2>&1; then
	cat "$success_dir/output" >&2
	fail "synthetic post-cutoff Rust release was rejected"
fi
[[ -x "$success_dir/install/krometrail" ]] || fail "successful install is not executable"
require_text "$success_dir/output" 'Installed krometrail v0.2.21'
require_text "$success_dir/output" 'Verified: krometrail 0.2.21 (rust)'
if grep -Fq -- 'api.github.com' "$success_dir/curl.log"; then
	fail "explicit post-cutoff install contacted latest-release API"
fi
require_text "$success_dir/curl.log" '/releases/download/v0.2.21/krometrail-linux-x64'
require_text "$success_dir/curl.log" '/releases/download/v0.2.21/checksums.txt'

printf 'installer fixtures: ok\n'
