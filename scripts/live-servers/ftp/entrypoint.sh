#!/usr/bin/env bash
# Starts one server software's instances on 127.0.0.1 (host network): `vsftpd`, `pureftpd` or
# `proftpd`. Ports and passive ranges are in scripts/live-servers/ftp/README.md. A self-signed
# certificate for 127.0.0.1 is made at start unless /certs holds one, so replacing it and
# restarting an instance presents a changed certificate (recert.sh). Every instance runs under
# `supervise`, which starts it again when it ends.
set -euo pipefail

supervise() {
    while true; do
        "$@" || true
        sleep 0.2
    done
}

server="${1:?usage: entrypoint.sh vsftpd|pureftpd|proftpd}"
home=/home/rcftp
mkdir -p "$home/start" "$home/docs"
echo "hello from the live server" > "$home/docs/readme.txt"
chown -R rcftp:rcftp "$home"

if [[ ! -f /certs/server.pem ]]; then
    openssl req -x509 -newkey rsa:2048 -nodes -days 30 -subj "/CN=127.0.0.1" \
        -addext "subjectAltName=IP:127.0.0.1" \
        -keyout /certs/server.key -out /certs/server.pem 2> /dev/null
fi

vsftpd_instance() { # port, pasv min, pasv max, extra lines
    local file="/etc/vsftpd-$1.conf"
    cat > "$file" << CONF
listen=YES
listen_ipv6=NO
listen_address=127.0.0.1
listen_port=$1
background=NO
anonymous_enable=NO
local_enable=YES
write_enable=YES
local_umask=022
pasv_enable=YES
pasv_min_port=$2
pasv_max_port=$3
pasv_address=127.0.0.1
port_enable=YES
connect_from_port_20=NO
seccomp_sandbox=NO
secure_chroot_dir=/var/run/vsftpd/empty
pam_service_name=vsftpd
utf8_filesystem=YES
log_ftp_protocol=YES
vsftpd_log_file=/var/log/vsftpd-$1.log
CONF
    printf '%s\n' "${@:4}" >> "$file"
    supervise vsftpd "$file" &
}

proftpd_instance() { # port, pasv min, pasv max, extra lines
    local file="/etc/proftpd-$1.conf"
    cat > "$file" << CONF
ModulePath /usr/lib/proftpd
LoadModule mod_xfer.c
LoadModule mod_ls.c
LoadModule mod_tls.c
ServerName "rc-live-ftp"
ServerType standalone
DefaultServer on
DefaultAddress 127.0.0.1
SocketBindTight on
UseIPv6 off
Port $1
PassivePorts $2 $3
User nobody
Group nogroup
AllowOverwrite on
AllowStoreRestart on
AllowRetrieveRestart on
RequireValidShell off
UseReverseDNS off
PidFile /run/proftpd-$1.pid
ScoreboardFile /run/proftpd-$1.scoreboard
WtmpLog off
SystemLog /var/log/proftpd-$1.log
TransferLog /var/log/proftpd-$1.xfer
CONF
    printf '%s\n' "${@:4}" >> "$file"
    supervise proftpd --nodaemon --config "$file" &
}

tls_vsftpd=(ssl_enable=YES rsa_cert_file=/certs/server.pem rsa_private_key_file=/certs/server.key
    force_local_logins_ssl=YES force_local_data_ssl=YES)
tls_proftpd=("TLSEngine on" "TLSRequired on" "TLSRSACertificateFile /certs/server.pem"
    "TLSRSACertificateKeyFile /certs/server.key" "TLSProtocol TLSv1.2 TLSv1.3"
    "TLSVerifyClient off" "TLSLog /var/log/proftpd-tls.log")

slow_proftpd="TransferRate RETR,STOR,APPE 2000"

case "$server" in
vsftpd)
    vsftpd_instance 2121 30000 30004
    vsftpd_instance 2122 30005 30009 "${tls_vsftpd[@]}"
    vsftpd_instance 9900 30010 30014 "${tls_vsftpd[@]}" implicit_ssl=YES
    # About 2 MB/s, for the dropped-connection, cancel and move tests.
    vsftpd_instance 2127 30015 30019 local_max_rate=2000000
    # EPSV refused while FEAT still lists it, as behind a firewall that only knows PASV: the
    # client has to fall back (ProFTPD's denied EPSV below leaves FEAT and is never tried).
    vsftpd_instance 2128 30020 30024 cmds_denied=EPSV
    ;;
pureftpd)
    cat /certs/server.key /certs/server.pem > /etc/ssl/private/pure-ftpd.pem
    chmod 600 /etc/ssl/private/pure-ftpd.pem
    pure_ftpd=(pure-ftpd -P 127.0.0.1 -l unix -E)
    supervise "${pure_ftpd[@]}" -S 127.0.0.1,2123 -p 30020:30024 -Y 0 &
    supervise "${pure_ftpd[@]}" -S 127.0.0.1,2124 -p 30025:30029 -Y 2 \
        -2 /etc/ssl/private/pure-ftpd.pem &
    supervise "${pure_ftpd[@]}" -S 127.0.0.1,2129 -p 30030:30034 -Y 0 -T 2000:2000 &
    ;;
proftpd)
    proftpd_instance 2125 30035 30039
    proftpd_instance 2126 30040 30044 "${tls_proftpd[@]}"
    proftpd_instance 9901 30045 30049 "${tls_proftpd[@]}" "TLSOptions UseImplicitSSL"
    # Slow, and slow without REST: a resume there has to start over.
    proftpd_instance 2130 30055 30059 "$slow_proftpd"
    proftpd_instance 2128 30050 30054 "$slow_proftpd" "AllowRetrieveRestart off" \
        "AllowStoreRestart off"
    # EPSV denied, as a firewall or an administrator does: what the owner's report looked like.
    proftpd_instance 2127 30015 30019 "<Limit EPSV>" "DenyAll" "</Limit>"
    ;;
*)
    echo "unknown server: $server" >&2
    exit 2
    ;;
esac
wait
