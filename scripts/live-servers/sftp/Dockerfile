# An OpenSSH server with SFTP and sudo for rCommander's live tests (RC-080-14), with vim and htop
# for the full-screen programs of the SSH terminal (RC-060-03). The user, its password and the
# sudo rule are dummy values for a container bound to 127.0.0.1 only. Every container makes its
# own host keys when it starts the first time (entrypoint.sh): a container made again on the same
# port presents another key, which is what the changed-key test needs.
FROM alpine:3.22.2@sha256:4b7ce07002c69e8f3d704a9c5d6fd3053be500b7f1c69fc0d80990c2ad8dd412

RUN apk add --no-cache openssh-server openssh-sftp-server sudo vim htop \
    && adduser -D -s /bin/sh tester \
    && echo 'tester:rc-live-dummy' | chpasswd \
    && echo 'tester ALL=(ALL) ALL' > /etc/sudoers.d/tester \
    && chmod 0440 /etc/sudoers.d/tester \
    && rm -f /etc/ssh/ssh_host_*

COPY sshd_config /etc/ssh/sshd_config
COPY entrypoint.sh /entrypoint.sh

EXPOSE 22
ENTRYPOINT ["/bin/sh", "/entrypoint.sh"]
