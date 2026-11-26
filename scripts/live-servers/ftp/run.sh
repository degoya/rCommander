#!/usr/bin/env bash
#
# The live FTP tests (rc-engine, `remote::ftp::live_*`, all #[ignore]d): builds the server
# images, starts vsftpd, pure-ftpd and ProFTPD in containers named rc-live-ftp-<server>, runs
# the tests against each server, encryption and data mode, and removes the containers again -
# also when a run fails or is interrupted. README.md beside this script has the details.
#
# Usage: scripts/live-servers/ftp/run.sh [server...]     (default: vsftpd pureftpd proftpd)
#
# Environment: RC_LIVE_LOGS (default /tmp/claude-<uid>/live-ftp), RC_JOBS (scripts/lib/jobs.sh).
# Takes the build lock (scripts/lib/lock.sh) - never wrap it in flock.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=../../lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# shellcheck source=../../lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"

HERE="$ROOT/scripts/live-servers/ftp"
LOGS="${RC_LIVE_LOGS:-/tmp/claude-$(id -u)/live-ftp}/$(date +%Y%m%d-%H%M%S)-$$"
mkdir -p "$LOGS"
servers=("$@")
[[ ${#servers[@]} -gt 0 ]] || servers=(vsftpd pureftpd proftpd)

# shellcheck disable=SC2317,SC2329 # called by the trap
cleanup() {
    local server
    for server in "${servers[@]}"; do
        docker rm -f "rc-live-ftp-$server-sftp" "rc-live-ftp-$server" > /dev/null 2>&1 || true
    done
}
trap cleanup EXIT

# Docker Desktop's credential helper is a Windows program; the base image is public.
export DOCKER_CONFIG="$LOGS/docker"
mkdir -p "$DOCKER_CONFIG"
echo '{}' > "$DOCKER_CONFIG/config.json"

# Each server: its instances as "kind port", and its slow and REST-less instances.
declare -A configs=(
    [vsftpd]="ftp:2121 ftps:2122 ftps_implicit:9900 ftp:2128"
    [pureftpd]="ftp:2123 ftps:2124"
    [proftpd]="ftp:2125 ftps:2126 ftps_implicit:9901 ftp:2127"
)
declare -A slow=([vsftpd]=2127 [pureftpd]=2129 [proftpd]=2130)
declare -A norest=([proftpd]=2128)
# pure-ftpd drops privileges with capabilities Docker does not grant by default.
declare -A caps=([pureftpd]="--cap-add SYS_NICE --cap-add DAC_READ_SEARCH --cap-add SYS_RESOURCE")

# The OpenSSH server of the SFTP suite, for server to server between FTP and SFTP: one per FTP
# container, in its network namespace on port 22.
echo "==> image rc-live-sftp-server:local"
docker build --quiet --network host -t rc-live-sftp-server:local "$ROOT/scripts/live-servers/sftp" \
    > "$LOGS/build-sftp.log" 2>&1

for server in "${servers[@]}"; do
    [[ -n "${configs[$server]:-}" ]] || { echo "unknown server: $server" >&2; exit 2; }
    echo "==> image rc-live-ftp-$server:local"
    docker build --quiet --network host --build-arg "SERVER=$server" \
        -t "rc-live-ftp-$server:local" "$HERE" > "$LOGS/build-$server.log" 2>&1
    docker rm -f "rc-live-ftp-$server" > /dev/null 2>&1 || true
    # shellcheck disable=SC2086 # the capability flags are separate words
    docker run -d --name "rc-live-ftp-$server" ${caps[$server]:-} \
        "rc-live-ftp-$server:local" "$server" > /dev/null
    docker rm -f "rc-live-ftp-$server-sftp" > /dev/null 2>&1 || true
    docker run -d --name "rc-live-ftp-$server-sftp" --network "container:rc-live-ftp-$server" \
        rc-live-sftp-server:local > /dev/null
done

wait_for() { # container, port
    local attempt
    for attempt in $(seq 50); do
        if docker exec "$1" bash -c "exec 3<>/dev/tcp/127.0.0.1/$2" 2> /dev/null; then
            return 0
        fi
        sleep 0.2
    done
    echo "!! $1 does not listen on $2 (attempt $attempt)" >&2
    docker logs "$1" >&2
    return 1
}

failed=0
summary="$LOGS/summary"
: > "$summary"

# One nextest run: `run <server> <kind> <port> <mode> <filter>`.
run() {
    local server="$1" kind="$2" port="$3" mode="$4" filter="$5"
    local log="$LOGS/$server-$kind-$mode-$filter.log"
    local status=0
    RC_LIVE_FTP_CONTAINER="rc-live-ftp-$server" RC_LIVE_FTP_IMAGE="rc-live-ftp-$server:local" \
        RC_LIVE_FTP_ROOT="$ROOT" RC_LIVE_FTP_PORT="$port" RC_LIVE_FTP_KIND="$kind" \
        RC_LIVE_FTP_MODE="$mode" RC_LIVE_FTP_USER=rcftp RC_LIVE_FTP_PASSWORD=rc-live-pass \
        RC_LIVE_FTP_HOME=/home/rcftp RC_LIVE_FTP_DROP=/usr/local/bin/drop.sh \
        RC_LIVE_FTP_RECERT=/usr/local/bin/recert.sh RC_LIVE_FTP_LOGS=/proc/1/root/var/log \
        RC_LIVE_FTP_SLOW_PORT="${slow[$server]:-}" RC_LIVE_FTP_NOREST_PORT="${norest[$server]:-}" \
        RC_LIVE_FTP_CAPS="${caps[$server]:-}" RC_LIVE_FTP_SFTP_PORT=22 \
        RC_LIVE_FTP_SFTP_USER=tester RC_LIVE_FTP_SFTP_PASSWORD=rc-live-dummy \
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER="$HERE/in-container.sh" \
        CARGO_BUILD_JOBS="$JOBS" \
        cargo nextest run --locked -p rc-engine --lib --run-ignored only --test-threads 1 \
        --no-fail-fast "$filter" > "$log" 2>&1 || status=$?
    local line
    line="$(grep -E '^ +Summary' "$log" | tail -n 1 || true)"
    printf '%-9s %-14s %-5s %-8s %-15s exit %s %s\n' \
        "$server" "$kind" "$port" "$mode" "$filter" "$status" "${line# }" | tee -a "$summary"
    [[ $status -eq 0 ]] || failed=1
}

for server in "${servers[@]}"; do
    for config in ${configs[$server]}; do
        wait_for "rc-live-ftp-$server" "${config#*:}"
    done
    wait_for "rc-live-ftp-$server" 22
    for config in ${configs[$server]}; do
        kind="${config%%:*}"
        port="${config#*:}"
        for mode in passive active; do
            run "$server" "$kind" "$port" "$mode" live_ftp_core_
        done
        if [[ "$kind" != ftp ]]; then
            run "$server" "$kind" "$port" passive live_ftp_tls_
        elif [[ "$config" == "${configs[$server]%% *}" ]]; then
            run "$server" "$kind" "$port" passive live_ftp_ops_
            run "$server" "$kind" "$port" passive live_ftp_s2s_
        fi
    done
done

echo "==> logs: $LOGS"
cat "$summary"
[[ $failed -eq 0 ]] && echo "==> all live FTP runs passed"
exit "$failed"
