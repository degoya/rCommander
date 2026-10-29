# shellcheck shell=bash
#
# The workspace version: `version` in Cargo.toml's [workspace.package], the one source of the
# version. Anchored to that section, because other sections may have a `version` key.
#
#   source "$ROOT/scripts/lib/workspace-version.sh"
#   rc_workspace_version < Cargo.toml

# The workspace version in the Cargo.toml text on stdin; nothing when there is none.
rc_workspace_version() {
    sed -n '/^\[workspace\.package\]/,/^\[/p' | sed -n 's/^version = "\(.*\)"/\1/p' | head -n 1
}
