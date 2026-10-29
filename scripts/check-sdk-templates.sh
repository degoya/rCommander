#!/usr/bin/env bash
#
# Every world of the plugin contract has an SDK template, and every template belongs to a world
# (RC-100-12). Copied and adapted from rDownloader's scripts/check-sdk-templates.sh at commit
# 26c8592a6 (RD-160-04). For each `world <name>-plugin` in crates/rc-plugin-api/wit/rcommander.wit,
# sdk/templates/<name>/ has to exist and carry:
#
#   - src/guest.rs generating that very world (`world: "<name>-plugin",` in its
#     `wit_bindgen::generate!`),
#   - manifest.toml with that `plugin_type`,
#   - wit/rcommander.wit, byte for byte the contract,
#   - README.md and CHANGES.md (a `## <version>` section per version),
#   - at least one `#[test]` under src/, so `cargo test` in a fresh scaffold proves something,
#   - an entry in crates/rcommander/src/plugin_cli/scaffold.rs, which compiles the template into
#     the executable (`include_str!` of each file; a test there holds the files one by one).
#
# And the versions a template states have to fit together:
#
#   - `wit-bindgen` is the workspace's own requirement, the one the bundled plugins build with.
#   - `api_version` is one this rCommander accepts (`SUPPORTED_API_VERSIONS`, rc-plugin-host:
#     `rc_plugin_api::API_VERSION`).
#   - `min_app_version` is the workspace version, the release the SDK is exported with
#     (scripts/set-version.sh writes it); once the release that first accepts `api_version` is
#     tagged (`v<release>`), it may not be older than that one (`api_first_release`).
#   - The SDK workflow sdk/ci/plugin.yml pins `RCOMMANDER_VERSION` to the workspace version too.
#
# A template folder no world names is refused too: `plugin new --type` would offer a scaffold
# for a world the contract does not have. Whether the template *compiles* is the scaffold test's
# question (crates/rcommander/src/plugin_cli/scaffold_tests.rs for the packaging, a wasm32 build
# for the guest).
#
# Pure bash, no build. Exit 1 on the findings, each named.
#
#   scripts/check-sdk-templates.sh [repo]
#
set -euo pipefail

ROOT="${1:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
WIT="$ROOT/crates/rc-plugin-api/wit/rcommander.wit"
TEMPLATES="$ROOT/sdk/templates"
SCAFFOLD="$ROOT/crates/rcommander/src/plugin_cli/scaffold.rs"

findings=0
finding() { echo "!! $1" >&2; findings=$((findings + 1)); }

# shellcheck source=lib/workspace-version.sh
source "$(dirname "${BASH_SOURCE[0]}")/lib/workspace-version.sh"

# The first release that accepts a plugin `api_version`. A contract step adds its row.
api_first_release() {
    case "$1" in
        0.1.0) echo 0.1.0 ;;
        0.2.0) echo 0.2.0 ;;
        0.3.0) echo 0.3.1 ;;
    esac
}

# Whether X.Y.Z $1 is not newer than version $2. A pre-release of $1's own version is older
# than $1, as semantic versioning and the host's `min_app_version` check have it.
version_at_most() {
    local core="${2%%[-+]*}"
    [[ "$1" == "$core" && "$2" != "$core" ]] && return 1
    [[ "$(printf '%s\n%s\n' "$1" "$core" | sort -V | head -n 1)" == "$1" ]]
}

# Whether release $1 is tagged in this repository.
released() { git -C "$ROOT" rev-parse -q --verify "refs/tags/v$1" > /dev/null 2>&1; }

# The value of `key = "..."` in TOML file $2, the first one.
toml_value() { sed -n "s/^$1 = \"\(.*\)\"\$/\1/p" "$2" 2> /dev/null | head -n 1; }

workspace_version="$(rc_workspace_version < "$ROOT/Cargo.toml")"
bindgen="$(sed -n '/^\[workspace\.dependencies\]/,/^\[/p' "$ROOT/Cargo.toml" | sed -n 's/^wit-bindgen = "\(.*\)"$/\1/p')"
[[ -n "$workspace_version" && -n "$bindgen" ]] \
    || { echo "!! no workspace version or wit-bindgen requirement in $ROOT/Cargo.toml" >&2; exit 1; }
supported="$(sed -n 's/^pub const API_VERSION: &str = "\(.*\)";$/\1/p' \
    "$ROOT/crates/rc-plugin-api/src/lib.rs")"
[[ -n "$supported" ]] || { echo "!! no API_VERSION in crates/rc-plugin-api/src/lib.rs" >&2; exit 1; }

worlds=()
while read -r world; do
    worlds+=("$world")
done < <(sed -n 's/^world \([a-z0-9-]*\)-plugin {$/\1/p' "$WIT")
[[ "${#worlds[@]}" -gt 0 ]] || { echo "!! no world found in $WIT" >&2; exit 1; }

for world in "${worlds[@]}"; do
    dir="$TEMPLATES/$world"
    if [[ ! -d "$dir" ]]; then
        finding "world ${world}-plugin has no template at sdk/templates/${world}"
        continue
    fi
    grep -qE "^[[:space:]]*world: \"${world}-plugin\",\$" "$dir/src/guest.rs" 2> /dev/null \
        || finding "sdk/templates/${world}/src/guest.rs does not generate world ${world}-plugin"
    [[ "$(toml_value plugin_type "$dir/manifest.toml")" == "$world" ]] \
        || finding "sdk/templates/${world}/manifest.toml does not declare plugin_type \"${world}\""
    cmp -s "$WIT" "$dir/wit/rcommander.wit" \
        || finding "sdk/templates/${world}/wit/rcommander.wit is not the current contract"
    [[ -f "$dir/README.md" ]] || finding "sdk/templates/${world} has no README.md"
    [[ -f "$dir/CHANGES.md" ]] || finding "sdk/templates/${world} has no CHANGES.md"
    grep -rqF '#[test]' "$dir/src" 2> /dev/null \
        || finding "sdk/templates/${world} has no unit test under src/"
    while IFS= read -r -d '' file; do
        relative="${file#"$dir/"}"
        grep -qF "sdk/templates/${world}/${relative}\")" "$SCAFFOLD" \
            || finding "sdk/templates/${world}/${relative} is not compiled in by crates/rcommander/src/plugin_cli/scaffold.rs"
    done < <(find "$dir" -type f -print0)

    found="$(toml_value wit-bindgen "$dir/Cargo.toml")"
    [[ "$found" == "$bindgen" ]] \
        || finding "sdk/templates/${world}/Cargo.toml asks for wit-bindgen ${found:-nothing}, the workspace for $bindgen"
    api="$(toml_value api_version "$dir/manifest.toml")"
    [[ " $supported " == *" $api "* ]] \
        || finding "sdk/templates/${world}/manifest.toml declares api_version ${api:-nothing}, this rCommander accepts $supported"
    first="$(api_first_release "$api")"
    minimum="$(toml_value min_app_version "$dir/manifest.toml")"
    if [[ -z "$first" ]]; then
        finding "api_version ${api:-nothing} has no row in api_first_release (scripts/check-sdk-templates.sh): add the release that first accepts it"
    elif [[ "$minimum" != "$workspace_version" ]]; then
        finding "sdk/templates/${world}/manifest.toml says min_app_version ${minimum:-nothing}, the workspace is $workspace_version (scripts/set-version.sh $workspace_version)"
    elif released "$first" && ! version_at_most "$first" "$minimum"; then
        finding "sdk/templates/${world}/manifest.toml says min_app_version $minimum, older than $first, the release that first shipped api_version $api"
    fi
done

for workflow in "$ROOT"/sdk/ci/*.yml; do
    [[ -e "$workflow" ]] || continue
    name="sdk/ci/$(basename "$workflow")"
    pin="$(sed -n 's/^  RCOMMANDER_VERSION: //p' "$workflow")"
    if [[ "$pin" != "$workspace_version" ]]; then
        finding "$name pins rCommander ${pin:-nothing}, the workspace is $workspace_version (scripts/set-version.sh $workspace_version)"
    fi
done

for dir in "$TEMPLATES"/*/; do
    [[ -d "$dir" ]] || continue
    name="$(basename "$dir")"
    known=0
    for world in "${worlds[@]}"; do
        [[ "$world" == "$name" ]] && known=1
    done
    [[ "$known" -eq 1 ]] || finding "sdk/templates/${name} belongs to no world of the contract"
done

if [[ "$findings" -gt 0 ]]; then
    echo "!! ${findings} finding(s) in the SDK templates" >&2
    exit 1
fi
echo "==> ${#worlds[@]} world(s), ${#worlds[@]} template(s), each complete"
