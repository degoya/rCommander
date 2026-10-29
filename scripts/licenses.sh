#!/usr/bin/env bash
#
# Writes crates/rcommander/resources/licenses/third-party.json, the crate licences the About page
# of the settings lists (RC-010-19).
#
# Generated, never kept by hand. `cargo tree` names every crate the rcommander binary links on
# each platform it is built for (Windows and Linux now, macOS later), features resolved as a build
# resolves them and `cfg()` followed one target at a time; proc macros, build scripts and tests
# run on the build machine and ship nowhere. `cargo metadata` has the declared licences, and every
# other crate of Cargo.lock is listed by name as not shipped, so the tests can tell a crate new to
# Cargo.lock from one the list forgot. A crate that declares no licence takes its entry from
# crates/rcommander/resources/licenses/overrides.json, which names where the licence was read;
# without one the list is not written. scripts/lib/licenses.py does the work.
#
# The tests in crates/rcommander/src/app/settings/about/licenses_tests.rs hold the list to
# Cargo.lock and name this script. Neither `cargo metadata` nor `cargo tree` compiles anything,
# so no build lock is taken; they may download the manifests of crates for other platforms the
# first time, unless --offline.
#
# Usage:
#   scripts/licenses.sh                      # rewrite the list
#   scripts/licenses.sh --check              # exit 1 if it would change, 2 if a licence is missing
#   scripts/licenses.sh --offline [--check]  # cargo without the network
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# The platforms a package is built for: Windows, Linux on x64 and arm64 (scripts/package-linux.sh
# --arch arm64), and macOS, which keeps compiling.
TARGETS=(
    x86_64-pc-windows-msvc
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-gnu
    x86_64-apple-darwin
    aarch64-apple-darwin
)

cargo_flags=(--locked)
script_flags=()
for argument in "$@"; do
    case "$argument" in
        --offline) cargo_flags+=(--offline) ;;
        --check) script_flags+=(--check) ;;
        *)
            echo "unknown argument: $argument" >&2
            exit 2
            ;;
    esac
done

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
cd "$ROOT"
cargo metadata --format-version 1 "${cargo_flags[@]}" > "$scratch/metadata.json"
for target in "${TARGETS[@]}"; do
    cargo tree "${cargo_flags[@]}" --package rcommander --target "$target" \
        --edges normal,no-proc-macro --prefix none --format '{p}' >> "$scratch/linked.txt"
done
# Not exec: the trap removes the scratch directory afterwards.
python3 -B "$ROOT/scripts/lib/licenses.py" "$ROOT" "$scratch/metadata.json" \
    "$scratch/linked.txt" "${script_flags[@]}"
