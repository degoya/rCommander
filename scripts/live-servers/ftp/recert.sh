#!/usr/bin/env bash
# Replaces the server's certificate with a new self-signed one for 127.0.0.1 and restarts the
# instance on port $1 (entrypoint.sh's `supervise` starts it again), as a server that changed
# its certificate does. Runs in the server's PID namespace and writes through /proc/1/root into
# the server's file system.
set -euo pipefail
# shellcheck source=daemons.sh
source "$(dirname "${BASH_SOURCE[0]}")/daemons.sh"
port="${1:?usage: recert.sh <port>}"
root=/proc/1/root
openssl req -x509 -newkey rsa:2048 -nodes -days 30 -subj "/CN=127.0.0.1" \
    -addext "subjectAltName=IP:127.0.0.1" \
    -keyout "$root/certs/server.key" -out "$root/certs/server.pem" 2> /dev/null
if [[ -f "$root/etc/ssl/private/pure-ftpd.pem" ]]; then
    cat "$root/certs/server.key" "$root/certs/server.pem" > "$root/etc/ssl/private/pure-ftpd.pem"
fi
# ProFTPD and pure-ftpd hide their sockets from another process (not dumpable): then every
# instance restarts, each with the new certificate.
masters_before="$(master_on "$port")"
[[ -n "$masters_before" ]] || masters_before="$(masters)"
[[ -n "$masters_before" ]] || { echo "no FTP daemon runs" >&2; exit 1; }
for master in $masters_before; do
    kill "$master"
done
# ProFTPD keeps listening for a moment after the signal: until the old master is gone (or a
# zombie), the port answers from it and the test would reach the old certificate on its way out.
for master in $masters_before; do
    for _ in $(seq 50); do
        [[ -d "/proc/$master" && "$(awk '{ print $3 }' "/proc/$master/stat" 2> /dev/null)" != Z ]] \
            || break
        sleep 0.1
    done
done
for _ in $(seq 50); do
    if (exec 3<> "/dev/tcp/127.0.0.1/$port") 2> /dev/null; then
        exit 0
    fi
    sleep 0.2
done
echo "the instance on $port did not come back" >&2
exit 1
