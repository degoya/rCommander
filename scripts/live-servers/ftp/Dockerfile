# Real FTP servers for rCommander's opt-in live tests (scripts/live-servers/ftp/README.md).
# The base is pinned by digest and the packages by version, so a run is repeatable.
FROM debian@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f

# The three servers conflict as packages (each provides ftp-server): one image per server.
ARG SERVER
RUN case "$SERVER" in \
        vsftpd) packages="vsftpd=3.0.5-0.2" ;; \
        pureftpd) packages="pure-ftpd=1.0.50-2.2+b2" ;; \
        proftpd) packages="proftpd-core=1.3.8.c+dfsg-4+deb13u5 proftpd-mod-crypto=1.3.8.c+dfsg-4+deb13u5" ;; \
        *) echo "SERVER must be vsftpd, pureftpd or proftpd" >&2; exit 2 ;; \
    esac \
    && apt-get update \
    && apt-get install -y --no-install-recommends $packages openssl procps \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -d /home/rcftp -s /bin/bash rcftp \
    && echo 'rcftp:rc-live-pass' | chpasswd \
    && mkdir -p /var/run/vsftpd/empty /certs

COPY entrypoint.sh daemons.sh drop.sh recert.sh /usr/local/bin/
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
