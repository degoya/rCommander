#!/usr/bin/env bash
# Drops every FTP session of the server - all processes below the listening daemons, so no
# control or data connection survives - and leaves the daemons listening: a broken link, as a
# test needs one. Runs in the server's PID namespace (scripts/live-servers/ftp/in-container.sh).
set -euo pipefail
# shellcheck source=daemons.sh
source "$(dirname "${BASH_SOURCE[0]}")/daemons.sh"

for master in $(masters); do
    for session in $(descendants "$master"); do
        kill -9 "$session" 2> /dev/null || true
    done
done
