#!/usr/bin/env bash
set -euo pipefail

# The stable_registry! macro is the sanctioned declaration for bare wire enums.
# Explicitly tagged enums use their serde representation as a separate contract;
# this guard targets a divergent bare derive that can publish Rust identifiers.
scan_sources() {
    if [[ -n "${KROMETRAIL_WIRE_ENUM_SCHEMA_ROOTS:-}" ]]; then
        find "$KROMETRAIL_WIRE_ENUM_SCHEMA_ROOTS" -type f -name '*.rs' -print0
    else
        rg --files crates src -g '*.rs' -g '!**/tests/**' -g '!**/tests.rs' -g '!**/*_test.rs' -g '!**/*_tests.rs' -g '!**/test_*.rs' -0
    fi
}

scan_file() {
    awk '
        function reset_pending() {
            pending_derive = 0
            pending_container_naming = 0
            in_container_serde = 0
        }
        function brace_delta(line, opens, closes) {
            opens = line
            closes = line
            gsub(/[^\{]/, "", opens)
            gsub(/[^\}]/, "", closes)
            return length(opens) - length(closes)
        }
        function naming_attribute(line) {
            return line ~ /rename_all[[:space:]]*=/ || line ~ /(^|[^_])rename[[:space:]]*=/
        }
        function start_enum(line) {
            return line ~ /(^|[[:space:]])enum[[:space:]]+[A-Za-z_][A-Za-z0-9_]*/ && line !~ /\$/
        }
        function start_variant(line) {
            return line ~ /^[[:space:]]*[A-Za-z_][A-Za-z0-9_]*[[:space:]]*(\{|\(|=|,|$)/
        }
        function begin_candidate(line) {
            derive_line = enum_line
            enum_braces = brace_delta(line)
            enum_container_naming = pending_container_naming
            variant_count = 0
            renamed_variant_count = 0
            pending_variant_rename = 0
            in_enum = 1
        }
        function finish_candidate() {
            if (!enum_container_naming && renamed_variant_count < variant_count) {
                printf "%s:%d: bare enum JsonSchema derive; declare wire enums through stable_registry!\n", FILENAME, derive_line
                failed = 1
            }
            in_enum = 0
            reset_pending()
        }
        {
            lines[NR] = $0
        }
        END {
            reset_pending()
            in_enum = 0
            for (line_number = 1; line_number <= NR; line_number++) {
                line = lines[line_number]
                if (in_enum) {
                    if (enum_braces == 1) {
                        if (line ~ /^[[:space:]]*#\[serde\(/ && naming_attribute(line)) {
                            pending_variant_rename = 1
                        }
                        if (line ~ /^[[:space:]]*rename_all[[:space:]]*=/ || line ~ /^[[:space:]]*rename[[:space:]]*=/) {
                            pending_variant_rename = 1
                        }
                        if (start_variant(line)) {
                            variant_count++
                            if (pending_variant_rename) {
                                renamed_variant_count++
                            }
                            pending_variant_rename = 0
                        }
                    }
                    enum_braces += brace_delta(line)
                    if (enum_braces <= 0) {
                        finish_candidate()
                    }
                    continue
                }

                if (line ~ /^[[:space:]]*#\[/) {
                    if (line ~ /#\[derive\(/ && line ~ /JsonSchema/) {
                        pending_derive = 1
                        enum_line = line_number
                    }
                    if (line ~ /#\[serde\(/) {
                        in_container_serde = (line !~ /\][[:space:]]*$/)
                        if (naming_attribute(line)) {
                            pending_container_naming = 1
                        }
                    } else if (in_container_serde && naming_attribute(line)) {
                        pending_container_naming = 1
                        if (line ~ /\][[:space:]]*$/) {
                            in_container_serde = 0
                        }
                    }
                    continue
                }

                if (in_container_serde) {
                    if (naming_attribute(line)) {
                        pending_container_naming = 1
                    }
                    if (line ~ /\][[:space:]]*$/) {
                        in_container_serde = 0
                    }
                    continue
                }

                if (pending_derive && start_enum(line)) {
                    begin_candidate(line)
                    continue
                }

                # Keep attributes adjacent to an enum together, including a
                # rename_all attribute placed before the derive attribute.
                if (line ~ /^[[:space:]]*$/ || line ~ /^[[:space:]]*\/\//) {
                    continue
                }
                reset_pending()
            }
            exit failed
        }
    ' "$1"
}

failed=0
while IFS= read -r -d '' file; do
    if ! scan_file "$file"; then
        failed=1
    fi
done < <(scan_sources)

if ((failed)); then
    exit 1
fi

# Keep the guard's acceptance criteria executable: plain derives, irrelevant
# container serde attributes, and partially renamed variants must fail; a
# naming-relevant container attribute before derive and fully renamed variants
# must pass. This runs for local and CI use.
if [[ "${KROMETRAIL_WIRE_ENUM_SCHEMA_SELF_TEST:-1}" == 1 ]]; then
    fixture_root=$(mktemp -d)
    trap 'rm -rf "$fixture_root"' EXIT
    printf '%s\n' \
        '#[derive(schemars::JsonSchema)]' \
        'enum Plain {' \
        '    Value,' \
        '}' > "$fixture_root/plain.rs"
    printf '%s\n' \
        '#[derive(schemars::JsonSchema)]' \
        '#[serde(deny_unknown_fields)]' \
        'enum IrrelevantContainerSerde {' \
        '    StillPascalCase,' \
        '}' > "$fixture_root/irrelevant_container.rs"
    printf '%s\n' \
        '#[derive(schemars::JsonSchema)]' \
        'enum PartialVariantRenames {' \
        '    #[serde(rename = "renamed_variant")]' \
        '    RenamedVariant,' \
        '    StillPascalCase,' \
        '}' > "$fixture_root/partial_variant.rs"
    printf '%s\n' \
        '#[serde(rename_all = "snake_case")]' \
        '#[derive(schemars::JsonSchema)]' \
        'enum NamingBeforeDerive {' \
        '    StillPascalCase,' \
        '}' > "$fixture_root/naming_before_derive.rs"
    printf '%s\n' \
        '#[derive(schemars::JsonSchema)]' \
        'enum FullyRenamedVariants {' \
        '    #[serde(rename = "first_variant")]' \
        '    FirstVariant,' \
        '    #[serde(rename = "second_variant")]' \
        '    SecondVariant,' \
        '}' > "$fixture_root/fully_renamed.rs"

    if self_test_output=$(KROMETRAIL_WIRE_ENUM_SCHEMA_ROOTS="$fixture_root" \
        KROMETRAIL_WIRE_ENUM_SCHEMA_SELF_TEST=0 "$0" 2>&1); then
        echo "wire enum schema guard self-test did not reject invalid fixtures" >&2
        exit 1
    fi
    for fixture in plain.rs irrelevant_container.rs partial_variant.rs; do
        if [[ "$self_test_output" != *"$fixture:"* ]]; then
            echo "wire enum schema guard self-test missed $fixture" >&2
            exit 1
        fi
    done
    for accepted in naming_before_derive.rs fully_renamed.rs; do
        if [[ "$self_test_output" == *"$accepted:"* ]]; then
            echo "wire enum schema guard self-test rejected $accepted" >&2
            exit 1
        fi
    done
fi
