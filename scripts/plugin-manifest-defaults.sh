#!/usr/bin/env bash
#
# Removes from the bundled plugin manifests what they need not state (RC-100-12): a shared field
# with the value plugins/metadata.toml holds, and a [limits] field at the host's default. The
# rules are in scripts/lib/plugin-manifest-defaults.py. Rerunnable: a converted manifest stays as
# it is, and a manifest a merge brought back in its long form is converted by running it again.
# Copied from rDownloader's scripts/plugin-manifest-defaults.sh at commit 26c8592a6.
#
# Usage:
#   scripts/plugin-manifest-defaults.sh                # convert every manifest, name each one
#   scripts/plugin-manifest-defaults.sh json-viewer    # only these
#   scripts/plugin-manifest-defaults.sh --check        # write nothing; exit 1 on a stated default
#
# Removing lines from a manifest changes the packaged manifest: a converted plugin needs a version
# raise before its next signed build.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/scripts/lib/plugin-manifest-defaults.py" "$ROOT" "$@"
