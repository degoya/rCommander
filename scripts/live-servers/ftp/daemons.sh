# shellcheck shell=bash
#
# The FTP daemons of the container, for drop.sh and recert.sh: `masters` prints the listening
# processes (a daemon whose parent is not the same daemon), `descendants <pid>` everything
# below one. Reads /proc, so it works from a container that shares the server's PID namespace.

comm_of() {
    cat "/proc/$1/comm" 2> /dev/null || true
}

parent_of() {
    awk '{ print $4 }' "/proc/$1/stat" 2> /dev/null || true
}

masters() {
    local pid comm
    for pid in $(pgrep -x 'vsftpd|pure-ftpd|proftpd' || true); do
        comm="$(comm_of "$pid")"
        [[ "$(comm_of "$(parent_of "$pid")")" == "$comm" ]] || echo "$pid"
    done
}

descendants() {
    local child
    for child in $(pgrep -P "$1" || true); do
        descendants "$child"
        echo "$child"
    done
}

# The master listening on `port` (ProFTPD and pure-ftpd rewrite their command lines, so the
# listening socket decides): its inode in /proc/net/tcp, then the process holding it.
master_on() {
    local hex inode pid
    hex="$(printf '%04X' "$1")"
    inode="$(awk -v port=":$hex" '$2 ~ port"$" && $4 == "0A" { print $10; exit }' /proc/net/tcp)"
    [[ -n "$inode" ]] || return 0
    local fd
    for pid in $(masters); do
        for fd in "/proc/$pid/fd/"*; do
            if [[ "$(readlink "$fd" 2> /dev/null)" == "socket:[$inode]" ]]; then
                echo "$pid"
                break
            fi
        done
    done
}
