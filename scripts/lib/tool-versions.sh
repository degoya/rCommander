# shellcheck shell=bash
#
# The development tools' versions, one source each. Rust is rust-toolchain.toml's `channel`; the
# rest are pinned here and named in docs/development.md ("Prerequisites").
#
#   source "$ROOT/scripts/lib/tool-versions.sh"
#   echo "$RC_NEXTEST_VERSION"
#   rc_rust_version "$ROOT"          # 1.99.0

# shellcheck disable=SC2034  # read by the scripts that source this file
RC_NEXTEST_VERSION="0.9.146"
# shellcheck disable=SC2034
RC_CARGO_XWIN_VERSION="0.23.1"
# shellcheck disable=SC2034
RC_SHELLCHECK_VERSION="0.11.0"
# shellcheck disable=SC2034
RC_SHELLCHECK_SHA256="8c3be12b05d5c177a04c29e3c78ce89ac86f1595681cab149b65b97c4e227198"
# gitleaks: the secret scan of `check.sh --preflight`; pinned for the CI download
# (scripts/ci-tools.sh, RC-100-31), rDownloader's pin. A local install may be any recent one.
# shellcheck disable=SC2034
RC_GITLEAKS_VERSION="8.30.1"
# shellcheck disable=SC2034
RC_GITLEAKS_SHA256="551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb"
# wasm-tools: componentising a plugin and checking its imports (RC-100-08; docs/plugins.md, "New
# dependencies"), rDownloader's pin.
# shellcheck disable=SC2034
RC_WASM_TOOLS_VERSION="1.261.0"

# The Rust toolchain checkout $1 pins: rust-toolchain.toml's `channel`.
rc_rust_version() {
    sed -n 's/^channel = "\(.*\)"$/\1/p' "$1/rust-toolchain.toml" | head -n 1
}
