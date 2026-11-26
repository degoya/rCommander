#!/usr/bin/env bash
#
# cargo's target runner for the live FTP tests (scripts/live-servers/ftp/run.sh sets it): runs one test
# binary inside the network and PID namespaces of the server container RC_LIVE_FTP_CONTAINER,
# so the servers listen on that namespace's 127.0.0.1 only, nothing is published on the host,
# and active mode works - the server connects back to the address the client names. The
# checkout is mounted read-only at its own path; drop.sh and recert.sh are in the image.
set -euo pipefail

container="${RC_LIVE_FTP_CONTAINER:?run through scripts/live-servers/ftp/run.sh}"
image="${RC_LIVE_FTP_IMAGE:?run through scripts/live-servers/ftp/run.sh}"
root="${RC_LIVE_FTP_ROOT:?run through scripts/live-servers/ftp/run.sh}"

# The server's capabilities too: reading /proc/1/root of a process that holds capabilities the
# reader lacks is refused (ptrace access mode), and recert.sh writes the certificate through it.
read -ra caps <<< "${RC_LIVE_FTP_CAPS:-}"

env_args=()
while IFS= read -r name; do
    env_args+=(-e "$name")
done < <(compgen -e | grep -E '^(RC_LIVE_FTP_|NEXTEST|CARGO|RUST_)' || true)

exec docker run --rm "${caps[@]}" --network "container:$container" --pid "container:$container" \
    -v "$root:$root:ro" -w "$PWD" "${env_args[@]}" --entrypoint "$1" "$image" "${@:2}"
