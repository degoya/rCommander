#!/usr/bin/env bash
#
# Builds, checks, packages, signs and conformance-checks the bundled plugins (RC-100-12):
#   artifacts/plugins/<name>-<version>.rcplug
# one per folder under plugins/ that has a manifest.toml; scripts/package-windows.sh puts them
# into plugins/ of the zip, and rCommander installs them at its first start.
#
# Copied and adapted from rDownloader's scripts/build-plugins.sh and scripts/lib/plugin-build.sh
# at commit 26c8592a6, without what serves rDownloader's 75 plugins and its CI (stamps, the
# same-version comparison, examples, shared plugin libraries). The steps per plugin:
#
#   1. `cargo rustc --crate-type cdylib` links the core module for wasm32-unknown-unknown; its
#      `wit_bindgen::generate!` (src/guest.rs) embeds the world. The crates are `rlib` only, so no
#      native build of the workspace links a module nobody loads. The checkout, cargo's and
#      rustup's homes are mapped to fixed names, so a panic location carries no local path.
#   2. `wasm-tools component new` (the version scripts/lib/tool-versions.sh pins) makes the
#      component - no WASI adapter, nothing is imported but the contract - and
#      scripts/check-plugin-imports.sh reads its import section.
#   3. `rcommander plugin package --defaults plugins/metadata.toml` writes the shared fields into
#      the manifest it packages and signs, with the development key `rcommander-dev-v1` until the
#      owner names the release key (docs/plugins.md, decision 7).
#   4. `rcommander plugin conformance` verifies the package against the compiled-in roots and
#      asks the component its kind's question.
#
# The signing key is read from RC_PLUGIN_KEY, by default the release key's private half at
# ~/.config/rcommander/rcommander-plugin.key (rcommander-plugin-v1) - outside the repository, where it
# stays: this script never makes a key, and a missing one stops it.
#
# Usage:
#   scripts/build-plugins.sh                 # every bundled plugin, signed
#   scripts/build-plugins.sh json-viewer     # only these
#   scripts/build-plugins.sh --development   # unsigned, into artifacts/dev-plugins
#   scripts/build-plugins.sh --list          # names the bundled plugins, builds nothing
#
# Takes the build lock (scripts/lib/lock.sh) - never wrap it in flock. Parallelism:
# scripts/lib/jobs.sh (RC_JOBS).
set -euo pipefail

KEY="${RC_PLUGIN_KEY:-$HOME/.config/rcommander/rcommander-plugin.key}"
TARGET="wasm32-unknown-unknown"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# shellcheck source=lib/tool-versions.sh
source "$ROOT/scripts/lib/tool-versions.sh"
# Sourced before the `cd`, because the lock library resolves this script's own path from $0;
# taken below, after --list has had its say.
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
cd "$ROOT"

usage() {
    echo "usage: scripts/build-plugins.sh [--development] [--list] [<plugin>...]" >&2
    exit 2
}

development=0
list_only=0
selected=()
for argument in "$@"; do
    case "$argument" in
        --development) development=1 ;;
        --list) list_only=1 ;;
        -h | --help) sed -n '2,/^set -euo/p' "$ROOT/scripts/build-plugins.sh" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
        -*) usage ;;
        *) selected+=("$argument") ;;
    esac
done

# Only folders with a manifest are plugins.
if [[ ${#selected[@]} -eq 0 ]]; then
    for manifest in plugins/*/manifest.toml; do
        [[ -e "$manifest" ]] || continue
        selected+=("$(basename "$(dirname "$manifest")")")
    done
fi
for name in "${selected[@]}"; do
    [[ -f "plugins/$name/manifest.toml" ]] || { echo "!! no plugin named $name (plugins/$name/manifest.toml)" >&2; exit 2; }
done
if [[ "$list_only" -eq 1 ]]; then
    printf '%s\n' "${selected[@]}"
    exit 0
fi
[[ ${#selected[@]} -gt 0 ]] || { echo "!! no plugin under plugins/" >&2; exit 1; }

if [[ "$development" -eq 0 && ! -f "$KEY" ]]; then
    echo "!! the signing key is not at $KEY" >&2
    echo "   The development key's private half is kept outside the repository" >&2
    echo "   (docs/development.md, \"Plugins\"); set RC_PLUGIN_KEY to where yours is, or pass" >&2
    echo "   --development for unsigned packages. Never make or copy a key into the repository." >&2
    exit 1
fi

# Everything above this line reads files. Everything below it compiles.
rc_take_lock "$@"

# The component bytes depend on the wasm-tools that encodes them, so a build here uses the
# pinned version or none.
installed_wasm_tools="$(wasm-tools --version 2> /dev/null | awk '{ print $2 }' || true)"
if [[ "$installed_wasm_tools" != "$RC_WASM_TOOLS_VERSION" ]]; then
    echo "!! wasm-tools $RC_WASM_TOOLS_VERSION is required, found ${installed_wasm_tools:-none}" >&2
    echo "   cargo install wasm-tools --version $RC_WASM_TOOLS_VERSION --locked" >&2
    exit 1
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
COMPONENTS="$TARGET_DIR/plugin-components"
OUT="$ROOT/artifacts/plugins"
[[ "$development" -eq 1 ]] && OUT="$ROOT/artifacts/dev-plugins"
mkdir -p "$COMPONENTS" "$OUT"

# The packager is the native rcommander in the dev profile, which the tests build anyway; the
# plugin host's Wasmtime and Cranelift are optimised in it (Cargo.toml, [profile.dev.package.*]).
echo "==> building the packager (rcommander, dev profile, jobs: $JOBS)"
CARGO_BUILD_JOBS="$JOBS" cargo build --locked --quiet -j "$JOBS" -p rcommander
PACKAGER="$TARGET_DIR/debug/rcommander"
[[ -x "$PACKAGER" ]] || { echo "!! $PACKAGER was not produced" >&2; exit 1; }

# The path map of every wasm32 build here; flags the caller set for wasm32 come first and stay.
wasm_flags="${CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS:-}"
wasm_flags+="${wasm_flags:+ }--remap-path-prefix=$ROOT=/rcommander"
wasm_flags+=" --remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo"
wasm_flags+=" --remap-path-prefix=${RUSTUP_HOME:-$HOME/.rustup}=/rustup"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="$wasm_flags"

# The value of `key = "..."` in TOML file $2, the first one.
toml_value() { sed -n "s/^$1 = \"\(.*\)\"\$/\1/p" "$2" | head -n 1; }

echo "==> building ${#selected[@]} plugin(s) for $TARGET"
for name in "${selected[@]}"; do
    directory="plugins/$name"
    crate="$(toml_value name "$directory/Cargo.toml")"
    version="$(toml_value version "$directory/manifest.toml")"
    [[ -n "$crate" && -n "$version" ]] || { echo "!! $directory names no crate or no version" >&2; exit 1; }

    echo "--> $crate $version"
    CARGO_BUILD_JOBS="$JOBS" cargo rustc --locked --release --target "$TARGET" -j "$JOBS" \
        -p "$crate" --lib --crate-type cdylib \
        || { echo "!! $crate does not build for $TARGET" >&2; exit 1; }
    module="$TARGET_DIR/$TARGET/release/${crate//-/_}.wasm"
    [[ -f "$module" ]] || { echo "!! the build left no $module" >&2; exit 1; }
    component="$COMPONENTS/$name.wasm"
    wasm-tools component new "$module" -o "$component" \
        || { echo "!! wasm-tools could not make a component of $module" >&2; exit 1; }
    "$ROOT/scripts/check-plugin-imports.sh" "$component"

    output="$OUT/$name-$version.rcplug"
    arguments=(plugin package --manifest "$directory/manifest.toml" --component "$component"
        --output "$output")
    [[ -f plugins/metadata.toml ]] && arguments+=(--defaults plugins/metadata.toml)
    [[ -d "$directory/locales" ]] && arguments+=(--locales "$directory/locales")
    checks=(plugin conformance "$output")
    if [[ "$development" -eq 1 ]]; then
        arguments+=(--development)
        checks+=(--development-mode)
    else
        arguments+=(--key "$KEY")
    fi
    "$PACKAGER" "${arguments[@]}"
    "$PACKAGER" "${checks[@]}"

    # One package per plugin: a version raise would otherwise leave the old one beside the new,
    # and both would be bundled. Anchored on `<name>-<digit>`.
    for existing in "$OUT/$name"-*.rcplug; do
        [[ -f "$existing" && "$existing" != "$output" ]] || continue
        remainder="${existing##*/}"
        remainder="${remainder#"$name-"}"
        [[ "$remainder" == [0-9]* ]] || continue
        rm -f "$existing"
        echo "    removed the superseded ${existing##*/}"
    done
done

echo "==> done - $(find "$OUT" -maxdepth 1 -name '*.rcplug' | wc -l) package(s) in $OUT"
