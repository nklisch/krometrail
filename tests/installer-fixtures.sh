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
	if [[ "${FAKE_DOWNLOAD_MODE:-success}" == fail ]]; then
		printf 'partial download' > "$output"
		exit 22
	fi
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

seed_previous_install() {
	local dir="$1"
	printf '#!/bin/sh\nprintf previous-install\n' > "$dir/install/krometrail"
	chmod +x "$dir/install/krometrail"
	cp "$dir/install/krometrail" "$dir/previous"
}

assert_previous_install_preserved() {
	local dir="$1"
	cmp -s "$dir/install/krometrail" "$dir/previous" || fail "failed install replaced the prior installation"
	for leftover in "$dir/install"/krometrail.??????; do
		if [[ -e "$leftover" ]]; then
			fail "failed install left temporary file $leftover"
		fi
	done
}

run_installer() {
	local dir="$1"
	shift
	local latest_tag="${FAKE_LATEST_TAG:-v0.2.20}"
	local download_mode="${FAKE_DOWNLOAD_MODE:-success}"
	PATH="$dir/bin:$PATH" \
	HOME="$dir/home" \
	CURL_LOG="$dir/curl.log" \
	FAKE_LATEST_TAG="$latest_tag" \
	FAKE_DOWNLOAD_MODE="$download_mode" \
	FIXTURE_ARTIFACT="$dir/artifact" \
	FIXTURE_CHECKSUMS="$dir/checksums.txt" \
	KROMETRAIL_INSTALL_DIR="$dir/install" \
	sh "$INSTALLER" --no-modify-path "$@"
}

tmp_dirs=()
cleanup() {
	((${#tmp_dirs[@]} == 0)) || rm -rf "${tmp_dirs[@]}"
}
trap cleanup EXIT

# Latest resolution must stop at the immutable legacy boundary before an
# artifact URL is requested.
latest_dir="$(mktemp -d)"
tmp_dirs+=("$latest_dir")
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
tmp_dirs+=("$explicit_dir")
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

# A checksum-valid artifact with an empty, wrong-product, or wrong-version
# identity must not replace a working install. Each case also proves that the
# temporary artifact is removed by the installer's EXIT trap.
for identity_case in empty wrong-product wrong-version; do
	identity_dir="$(mktemp -d)"
	tmp_dirs+=("$identity_dir")
	make_fixture "$identity_dir"
	case "$identity_case" in
		empty)
			artifact='#!/bin/sh
exit 0
'
			expected_error='empty --version output'
			;;
		wrong-product)
			artifact='#!/bin/sh
printf "other 0.2.21\\n"
'
			expected_error='expected '\''krometrail 0.2.21'\'''
			;;
		wrong-version)
			artifact='#!/bin/sh
printf "krometrail 0.2.22\\n"
'
			expected_error='expected '\''krometrail 0.2.21'\'''
			;;
	esac
	write_artifact "$identity_dir" 0755 "$artifact"
	seed_previous_install "$identity_dir"
	: > "$identity_dir/curl.log"
	if run_installer "$identity_dir" --version v0.2.21 > "$identity_dir/output" 2>&1; then
		fail "${identity_case} artifact was accepted"
	fi
	require_text "$identity_dir/output" "$expected_error"
	require_text "$identity_dir/output" 'existing installation was preserved'
	assert_previous_install_preserved "$identity_dir"
done

# A checksum-valid artifact that cannot execute must still preserve the old
# installation. The source fixture is deliberately non-executable; chmod
# happens only on the temporary download path inside the installer.
nonexec_dir="$(mktemp -d)"
tmp_dirs+=("$nonexec_dir")
make_fixture "$nonexec_dir"
write_artifact "$nonexec_dir" 0644 'not an executable\n'
seed_previous_install "$nonexec_dir"
: > "$nonexec_dir/curl.log"
if run_installer "$nonexec_dir" --version v0.2.21 > "$nonexec_dir/output" 2>&1; then
	fail "non-executable artifact was accepted"
fi
require_text "$nonexec_dir/output" 'failed --version'
assert_previous_install_preserved "$nonexec_dir"
if grep -Fq -- 'Installed krometrail' "$nonexec_dir/output"; then
	fail "failed validation printed an install success message"
fi

# A failed direct asset download must preserve the old installation and clean
# the partial temporary file before returning the download error.
download_failure_dir="$(mktemp -d)"
tmp_dirs+=("$download_failure_dir")
make_fixture "$download_failure_dir"
write_artifact "$download_failure_dir" 0755 '#!/bin/sh
printf "krometrail 0.2.21\\n"
'
seed_previous_install "$download_failure_dir"
: > "$download_failure_dir/curl.log"
if FAKE_DOWNLOAD_MODE=fail run_installer "$download_failure_dir" --version v0.2.21 > "$download_failure_dir/output" 2>&1; then
	fail "failed asset download was accepted"
fi
require_text "$download_failure_dir/output" 'Download failed'
assert_previous_install_preserved "$download_failure_dir"

# A configurable post-cutoff latest response must install successfully without
# an explicit --version argument. This exercises both latest resolution and
# exact identity validation in isolation from GitHub.
latest_success_dir="$(mktemp -d)"
tmp_dirs+=("$latest_success_dir")
make_fixture "$latest_success_dir"
write_artifact "$latest_success_dir" 0755 '#!/bin/sh
if [ "${1:-}" = "--version" ]; then
  printf "krometrail 0.2.22\\n"
  exit 0
fi
exit 1
'
: > "$latest_success_dir/curl.log"
if ! FAKE_LATEST_TAG=v0.2.22 run_installer "$latest_success_dir" > "$latest_success_dir/output" 2>&1; then
	cat "$latest_success_dir/output" >&2
	fail "synthetic post-cutoff latest Rust release was rejected"
fi
[[ -x "$latest_success_dir/install/krometrail" ]] || fail "successful latest install is not executable"
require_text "$latest_success_dir/output" 'Installed krometrail v0.2.22'
require_text "$latest_success_dir/output" 'Verified: krometrail 0.2.22'
require_text "$latest_success_dir/curl.log" 'https://api.github.com/repos/nklisch/krometrail/releases/latest'
require_text "$latest_success_dir/curl.log" '/releases/download/v0.2.22/krometrail-linux-x64'
require_text "$latest_success_dir/curl.log" '/releases/download/v0.2.22/checksums.txt'

printf 'installer fixtures: ok\n'
