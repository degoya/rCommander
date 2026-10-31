#!/usr/bin/env bash
#
# One check of package-channels.yml (RC-100-52, RC-100-53), after a package manager installed or
# upgraded rCommander: the `rcommander` on PATH runs and is the expected version, and the update
# check takes it for the manager's installation - `rcommander install-kind` names the kind and
# the manager's command, where the release has the subcommand (it came with RC-100-52; an older
# release answers with clap's usage error and only the version is checked). CI only; bash on every
# runner, Git Bash on Windows, where rcommander is a GUI program whose output a pipe still
# carries.
#
#   scripts/ci-channel-check.sh <homebrew|scoop|winget> <tag or version>
set -euo pipefail

[[ $# -eq 2 ]] || { echo "usage: scripts/ci-channel-check.sh <kind> <version>" >&2; exit 2; }
kind="$1"
version="${2#v}"

command -v rcommander
said="$(rcommander --version)"
echo "rcommander --version: ${said}"
if [[ "${said}" != *"${version}"* ]]; then
    echo "::error::rcommander --version is not ${version}"
    exit 1
fi
if said="$(rcommander install-kind 2>&1)"; then
    echo "rcommander install-kind: ${said//$'\n'/ | }"
    if [[ "$(head -n 1 <<< "${said}")" != "${kind}" ]]; then
        echo "::error::the update check takes this installation for '$(head -n 1 <<< "${said}")', not ${kind}"
        exit 1
    fi
    if [[ "$(sed -n 2p <<< "${said}")" != *upgrade* && "$(sed -n 2p <<< "${said}")" != *update* ]]; then
        echo "::error::the update check names no command of ${kind}"
        exit 1
    fi
else
    echo "::notice::rcommander ${version} has no install-kind subcommand yet; the version alone is checked"
fi
