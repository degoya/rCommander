# shellcheck shell=bash
#
# The development tools' versions, one source each. Rust is rust-toolchain.toml's `channel`; the
# rest are pinned here and named in docs/development.md ("Prerequisites"). A GitHub workflow
# cannot source this file where an action takes the version as a literal (`tool: nextest@...`);
# scripts/tests/tool-versions.sh holds every such literal, and the documentation's table, to the
# values here (RC-110-33).
#
#   source "$ROOT/scripts/lib/tool-versions.sh"
#   echo "$RC_NEXTEST_VERSION"
#   rc_rust_version "$ROOT"          # 1.99.1
#   rc_user_tools_path               # ~/.local/bin in front of PATH, unless it leads it

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
# komac: the winget submission of release.yml (scripts/release-channels.sh winget, RC-100-53), the
# Linux x86_64 archive with its SHA-256. nfpm: the deb and rpm of a release
# (scripts/release-channels.sh install-nfpm, RC-100-54), likewise. CI downloads only.
# shellcheck disable=SC2034
RC_KOMAC_VERSION="2.17.2"
# shellcheck disable=SC2034
RC_KOMAC_SHA256="b03b020eec98acfb0b53e284a6e21815b884be33de6785b97e7a120f0e748966"
# shellcheck disable=SC2034
RC_NFPM_VERSION="2.47.0"
# shellcheck disable=SC2034
RC_NFPM_SHA256="0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783"
# Fuzzing and coverage (RC-080-72; docs/development.md, "Fuzzing and coverage"): cargo-fuzz needs
# a nightly for libFuzzer's sanitizer flags, pinned by date so a finding reproduces; the main
# workspace stays on rust-toolchain.toml's stable. cargo-llvm-cov runs on that stable with its
# llvm-tools component.
# nightly-2026-11-01 stops on russh/ssh-key with E0391 (a query cycle on async drop), 2026-11-05.
# shellcheck disable=SC2034
RC_FUZZ_NIGHTLY="nightly-2026-10-01"
# shellcheck disable=SC2034
RC_CARGO_FUZZ_VERSION="0.13.2"
# shellcheck disable=SC2034
RC_LLVM_COV_VERSION="0.9.1"

# The Rust toolchain checkout $1 pins: rust-toolchain.toml's `channel`.
rc_rust_version() {
    sed -n 's/^channel = "\(.*\)"$/\1/p' "$1/rust-toolchain.toml" | head -n 1
}

# Puts ~/.local/bin in front of PATH unless it is on it already: the pinned tools a user installs
# (docs/development.md: shellcheck through uv) sit there, and a shell started without the profile
# lacks it - check.sh then finds an older system shellcheck and fails `shellcheck-version`
# (0.10.0, 2026-10-30). The one place for it: check.sh and release-start.sh both call this.
rc_user_tools_path() {
    [[ -n "${HOME:-}" ]] || return 0
    # In front even when it is on PATH already: behind /usr/bin the system's tool still wins.
    case "$PATH" in
        "$HOME/.local/bin:"*) ;;
        *) export PATH="$HOME/.local/bin:$PATH" ;;
    esac
}
