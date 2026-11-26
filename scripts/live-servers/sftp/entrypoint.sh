#!/bin/sh
# Host keys on the first start only, so `docker restart` keeps them and a new container does not;
# a read-only folder for the test of a failing target; sshd in the foreground, its log on stderr.
# With SFTP_LOG (a level such as DEBUG1) the SFTP server is the external `sftp-server`, which
# logs every read with its offset to syslog, /var/log/messages - the resume tests count them.
set -eu
ssh-keygen -A > /dev/null
mkdir -p /srv/readonly
chmod 0555 /srv/readonly
if [ -n "${SFTP_LOG:-}" ]; then
    syslogd -s 0 -O /var/log/messages
    sed -i "s|^Subsystem sftp .*|Subsystem sftp /usr/lib/ssh/sftp-server -l $SFTP_LOG|" \
        /etc/ssh/sshd_config
fi
exec /usr/sbin/sshd -D -e
