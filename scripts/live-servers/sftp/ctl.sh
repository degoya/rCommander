#!/usr/bin/env bash
#
# The containers of the live SFTP tests (RC-080-14): OpenSSH servers in Docker, each on its own
# port of 127.0.0.1, with dummy credentials. run.sh starts and removes them; the tests call this
# script for what only Docker can do - a new host key, a dropped session, a stopped server.
#
# Usage:
#   scripts/live-servers/sftp/ctl.sh up                  # build the image, start every server, wait
#   scripts/live-servers/sftp/ctl.sh down                # remove every container and the network
#   scripts/live-servers/sftp/ctl.sh rekey <role>        # the server made again: same port, new host key
#   scripts/live-servers/sftp/ctl.sh kill-sessions <role># end every SSH session (the connection drops)
#   scripts/live-servers/sftp/ctl.sh stop|start <role>   # stop or start the server (start waits)
#   scripts/live-servers/sftp/ctl.sh pause|unpause <role># freeze it: the connection hangs, nothing closes
#   scripts/live-servers/sftp/ctl.sh kill <role>         # SIGKILL the container (start brings it back)
#   scripts/live-servers/sftp/ctl.sh fingerprints <role> # the SHA256 fingerprints of its host keys
#   scripts/live-servers/sftp/ctl.sh sha256 <role> <path># the SHA-256 of a file on the server
#   scripts/live-servers/sftp/ctl.sh exec <role> <cmd>   # a shell command in the server as root
#   scripts/live-servers/sftp/ctl.sh logs <role>         # the server's sshd log
#   scripts/live-servers/sftp/ctl.sh sftp-log <role>     # its SFTP log (the resume servers only)
#   scripts/live-servers/sftp/ctl.sh port <role>         # its port on 127.0.0.1
#
# The roles and their ports (RC_LIVE_SFTP_PORT_BASE, default 2201, and the twelve after it); the
# tests read the same table (crates/rc-engine/tests/live/mod.rs):
#   a 0, b 1 (the second server of server to server), keys 2, drop 3, spent 4, restart 5, hang 6,
#   alive 7 (keepalive and reconnect), faults-list 8, faults-copy 9, faults-term 10 (one fault
#   case each), many 11 (a folder of 100 000 entries, also on faults-list), lost 12 (dropped
#   sessions, RC-060-01)
#
# Environment:
#   RC_LIVE_SFTP_PORT_BASE  the first port (default 2201)
#   RC_LIVE_SFTP_BIG_MB     the size of the random file each server holds (default 300)
#
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="rc-live-sftp"
IMAGE="$PREFIX-server:local"
NETWORK="$PREFIX-net"
ROLES=(a b keys drop spent restart hang alive faults-list faults-copy faults-term many lost)
BASE="${RC_LIVE_SFTP_PORT_BASE:-2201}"
BIG_MB="${RC_LIVE_SFTP_BIG_MB:-300}"

die() {
    echo "ctl.sh: $*" >&2
    exit 2
}

[[ "$BASE" =~ ^[1-9][0-9]{3,4}$ ]] || die "RC_LIVE_SFTP_PORT_BASE must be a port, not '$BASE'"
[[ "$BIG_MB" =~ ^[1-9][0-9]*$ ]] || die "RC_LIVE_SFTP_BIG_MB must be a whole number, not '$BIG_MB'"

# A Docker without a usable credential helper (WSL with Docker Desktop's config) cannot pull even
# a public image; an empty config of our own needs none.
docker_cmd() {
    if [[ -z "${DOCKER_CONFIG:-}" ]]; then
        local config="${TMPDIR:-/tmp}/$PREFIX-docker-config"
        mkdir -p "$config"
        [[ -f "$config/config.json" ]] || echo '{}' > "$config/config.json"
        DOCKER_CONFIG="$config" docker "$@"
    else
        docker "$@"
    fi
}

port_of() {
    local index
    for index in "${!ROLES[@]}"; do
        if [[ "${ROLES[$index]}" == "$1" ]]; then
            echo $((BASE + index))
            return 0
        fi
    done
    die "no role '$1' (one of: ${ROLES[*]})"
}

name_of() {
    port_of "$1" > /dev/null
    echo "$PREFIX-$1"
}

# Waits until the server on `port` sends its SSH banner, 30 s at most.
wait_ready() {
    local port="$1" tries=0 banner
    while (( tries < 60 )); do
        banner="$(timeout 2 bash -c "exec 3<>/dev/tcp/127.0.0.1/$port && head -c 4 <&3" 2> /dev/null || true)"
        [[ "$banner" == SSH- ]] && return 0
        sleep 0.5
        tries=$((tries + 1))
    done
    die "the server on port $port did not answer"
}

# Starts the container of `role`, with a sub folder in the user's home and, on the servers whose
# tests read it, the random file; the servers of the resume tests log every SFTP read; `many` and
# `faults-list` hold /home/tester/many with 100 000 empty files.
create() {
    local role="$1" name port
    name="$(name_of "$role")"
    port="$(port_of "$role")"
    local log="" big="$BIG_MB" many=0
    case "$role" in
        drop | spent | restart | hang) log=DEBUG1 ;;
        alive | faults-copy | faults-term | lost) big=0 ;;
        faults-list | many) big=0 many=100000 ;;
    esac
    docker_cmd run -d --name "$name" --network "$NETWORK" --hostname "$role" \
        -e "SFTP_LOG=$log" -p "127.0.0.1:$port:22" "$IMAGE" > /dev/null
    wait_ready "$port"
    docker_cmd exec "$name" sh -c "if [ $big -gt 0 ]; then \
            dd if=/dev/urandom of=/home/tester/big.bin bs=1M count=$big 2> /dev/null; fi \
        && mkdir -p /home/tester/docs && echo hello > /home/tester/docs/readme.txt \
        && if [ $many -gt 0 ]; then mkdir /home/tester/many && cd /home/tester/many \
            && seq 1 $many | sed 's/^/entry-/' | xargs touch; fi \
        && chown -R tester:tester /home/tester"
}

up() {
    docker_cmd build -q -t "$IMAGE" "$HERE" > /dev/null
    docker_cmd network inspect "$NETWORK" > /dev/null 2>&1 \
        || docker_cmd network create "$NETWORK" > /dev/null
    local role
    for role in "${ROLES[@]}"; do
        create "$role"
    done
}

down() {
    local ids
    ids="$(docker_cmd ps -aq --filter "name=^$PREFIX-")"
    if [[ -n "$ids" ]]; then
        # shellcheck disable=SC2086  # one id per word
        docker_cmd rm -f $ids > /dev/null
    fi
    docker_cmd network rm "$NETWORK" > /dev/null 2>&1 || true
}

[[ $# -ge 1 ]] || die "a command is needed (see the head of this script)"
command="$1"
shift
case "$command" in
    up) up ;;
    down) down ;;
    rekey)
        name="$(name_of "${1:?a role}")"
        docker_cmd rm -f "$name" > /dev/null
        create "$1"
        ;;
    kill-sessions)
        name="$(name_of "${1:?a role}")"
        # Every child of the listener is one connection (its title is rewritten, so no name
        # matches it reliably); syslogd, also a child of PID 1, stays. Names what it killed.
        # shellcheck disable=SC2016  # expanded by the container's shell
        docker_cmd exec "$name" sh -c 'for pid in $(pgrep -P 1); do
            title="$(tr "\0" " " < "/proc/$pid/cmdline")"
            case "$title" in syslogd*) ;; *) kill "$pid" && echo "killed $pid: $title" ;; esac
        done'
        ;;
    stop) docker_cmd stop -t 1 "$(name_of "${1:?a role}")" > /dev/null ;;
    start)
        docker_cmd start "$(name_of "${1:?a role}")" > /dev/null
        wait_ready "$(port_of "$1")"
        ;;
    pause) docker_cmd pause "$(name_of "${1:?a role}")" > /dev/null ;;
    unpause) docker_cmd unpause "$(name_of "${1:?a role}")" > /dev/null ;;
    kill) docker_cmd kill "$(name_of "${1:?a role}")" > /dev/null ;;
    fingerprints)
        # shellcheck disable=SC2016  # expanded by the container's shell
        docker_cmd exec "$(name_of "${1:?a role}")" sh -c \
            'for key in /etc/ssh/ssh_host_*_key.pub; do ssh-keygen -lf "$key"; done' \
            | awk '{ print $2 }'
        ;;
    sha256)
        docker_cmd exec "$(name_of "${1:?a role}")" sha256sum "${2:?a path}" | awk '{ print $1 }'
        ;;
    exec)
        name="$(name_of "${1:?a role}")"
        shift
        docker_cmd exec "$name" sh -c "$*"
        ;;
    logs) docker_cmd logs "$(name_of "${1:?a role}")" 2>&1 ;;
    sftp-log) docker_cmd exec "$(name_of "${1:?a role}")" cat /var/log/messages ;;
    port) port_of "${1:?a role}" ;;
    *) die "unknown command '$command'" ;;
esac
