#!/usr/bin/env bash
#
# Writes the crate licences of the About page and the packages (RC-010-19, RC-110-72): under
# crates/rcommander/resources/licenses/, third-party.json (name, version and licence per crate)
# and THIRD-PARTY-LICENSES (their licence texts, each once, headed by the crates it covers; every
# package carries it, and the deb and rpm turn it into their DEP-5 copyright file).
#
# Generated, never kept by hand. `cargo tree` names every crate the rcommander binary links on
# each platform it is built for (Windows, Linux and macOS) and every crate a bundled plugin
# (plugins/*/manifest.toml) compiles in on wasm32, features resolved as a build resolves them and
# `cfg()` followed one target at a time; proc macros, build scripts and tests run on the build
# machine and ship nowhere. `cargo metadata` has the declared licences and where cargo unpacked
# each crate, whose licence files are the texts; every other crate of Cargo.lock is listed by
# name as not shipped, so the tests can tell a crate new to Cargo.lock from one the list forgot.
# A crate that declares no licence, or whose archive carries no licence file, takes its entry
# from crates/rcommander/resources/licenses/overrides.json, which names where the licence was
# read; without one nothing is written. scripts/lib/licenses.py and license_texts.py do the work.
#
# The tests in crates/rcommander/src/app/settings/about/licenses_tests.rs and
# license_texts_tests.rs hold the files to Cargo.lock and name this script. Neither `cargo
# metadata` nor `cargo tree` compiles anything, so no build lock is taken; they may download the
# manifests of crates for other platforms the first time, unless --offline.
#
# Usage:
#   scripts/licenses.sh                      # rewrite both files
#   scripts/licenses.sh --check              # exit 1 if one would change, 2 if a licence is missing
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
# The bundled plugins: every folder of plugins/ with a manifest (scripts/build-plugins.sh), built
# for wasm32-unknown-unknown.
for manifest in plugins/*/manifest.toml; do
    [[ -e "$manifest" ]] || continue
    cargo tree "${cargo_flags[@]}" --manifest-path "$(dirname "$manifest")/Cargo.toml" \
        --target wasm32-unknown-unknown --edges normal,no-proc-macro --prefix none --format '{p}' \
        >> "$scratch/linked.txt"
done
# Not exec: the trap removes the scratch directory afterwards.
python3 -B "$ROOT/scripts/lib/licenses.py" "$ROOT" "$scratch/metadata.json" \
    "$scratch/linked.txt" "${script_flags[@]}"
