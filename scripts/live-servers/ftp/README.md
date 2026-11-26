# Live FTP tests

rCommander's FTP backend against real servers - vsftpd, pure-ftpd and ProFTPD from Debian 13,
in containers - driven through the engine as the window drives it. The tests are
`remote::ftp::live_*` in `crates/rc-engine/src/remote/ftp_live_tests.rs` and
`ftp_live_ops_tests.rs` and `ftp_live_s2s_tests.rs`, all `#[ignore]`d: a normal `cargo nextest
run` never needs Docker. The workflow `.github/workflows/live.yml` runs this suite and the SFTP
one (`../sftp/`) every week on GitHub and on request (RC-110-69); in the development repository
`scripts/live-tests.sh` runs both under one build lock.

```bash
scripts/live-servers/ftp/run.sh                 # all three servers
scripts/live-servers/ftp/run.sh proftpd         # one
```

`run.sh` takes the build lock (never wrap it in `flock`), builds one image per server
(`rc-live-ftp-<server>:local`, Debian pinned by digest, packages by version), starts the
containers `rc-live-ftp-<server>`, runs the tests and removes the containers on every exit.
Logs and a summary go to `/tmp/claude-<uid>/live-ftp/<time>-<pid>/` (`RC_LIVE_LOGS`).

## How a test reaches a server

Nothing is published on the host. cargo's target runner (`in-container.sh`) starts each test
binary in a container that shares the server container's network and PID namespaces; the
servers listen on that namespace's `127.0.0.1`. So passive and active mode both work as on a
LAN - in active mode the server connects back to the address the client names - and the tests
can drop sessions (`drop.sh`) or replace the certificate (`recert.sh`) in the server's
namespace. The runner gets the server's capabilities (`RC_LIVE_FTP_CAPS`): Linux refuses
`/proc/1/root` of a process holding capabilities the reader lacks, and `recert.sh` writes
through it. The checkout is mounted read-only at its own path.

Beside each FTP server a container of the SFTP suite's image (`rc-live-ftp-<server>-sftp`,
user `tester`, password `rc-live-dummy`) runs OpenSSH in the same network namespace on port 22,
for server to server between FTP and SFTP.

## Servers

Ports are inside the container namespaces. User `rcftp`, password `rc-live-pass` (dummy), home
`/home/rcftp` with `docs/readme.txt` and `start/`. FTPS uses a self-signed certificate for
`127.0.0.1` made at container start.

| Server | Plain | Explicit FTPS | Implicit FTPS | Slow (~2 MB/s) | Without REST, slow |
| --- | --- | --- | --- | --- | --- |
| vsftpd 3.0.5 | 2121 (2128 `EPSV` refused) | 2122 | 9900 | 2127 | - |
| pure-ftpd 1.0.50 | 2123 | 2124 | - (not built in) | 2129 | - |
| ProFTPD 1.3.8c | 2125 (2127 `EPSV` denied) | 2126 | 9901 | 2130 | 2128 |

vsftpd also runs on 2128 with `EPSV` refused (`cmds_denied=EPSV`, `550`) while `FEAT` still
lists it, as behind a firewall that only knows `PASV`: the client has to fall back to `PASV`.
ProFTPD runs on 2127 with `EPSV` denied (`<Limit EPSV> DenyAll`), which also takes it out of
`FEAT`, so the client asks `PASV` from the start. The core tests run against both. With the
fallback taken out of the client, the passive run on vsftpd 2128 fails 4 of 4 (the panel shows
"permission denied" - the shape of the "This is not supported here." report).

Passive port ranges are 30000-30059, five ports per instance (`entrypoint.sh`; each server
has its own network namespace, so ranges may repeat between servers). vsftpd and
ProFTPD require TLS session reuse on the data connection by default, which the tests cover.
pure-ftpd needs `SYS_NICE`, `DAC_READ_SEARCH` and `SYS_RESOURCE` to drop its privileges.

## What runs

For every server and encryption: `live_ftp_core_*` in passive and active mode (Test, opening at
the login and at a start folder with the Rights column, upload/list/download, a 48 MiB file
byte for byte both ways); for FTPS `live_ftp_tls_*` (unknown certificate, accept once, trust,
a changed certificate refused); for plain FTP `live_ftp_ops_*` (`.rcpart` and cancel, F6 move,
time stamps, disconnect, the per-server transfer limit, resume after a dropped connection and
a server without `REST`) and `live_ftp_s2s_*` (F5 from one FTP session to another - the slow
instance - then FTP to SFTP and SFTP back to FTP, byte for byte).

## Environment the tests read

`RC_LIVE_FTP_PORT`, `RC_LIVE_FTP_KIND` (`ftp`, `ftps`, `ftps_implicit`), `RC_LIVE_FTP_MODE`
(`passive`, `active`), `RC_LIVE_FTP_USER`, `RC_LIVE_FTP_PASSWORD`, `RC_LIVE_FTP_HOME`,
`RC_LIVE_FTP_SLOW_PORT`, `RC_LIVE_FTP_NOREST_PORT`, `RC_LIVE_FTP_DROP`, `RC_LIVE_FTP_RECERT`,
`RC_LIVE_FTP_SFTP_PORT`, `RC_LIVE_FTP_SFTP_USER`, `RC_LIVE_FTP_SFTP_PASSWORD`; `in-container.sh`
also needs `RC_LIVE_FTP_CONTAINER`, `RC_LIVE_FTP_IMAGE`, `RC_LIVE_FTP_ROOT` and takes
`RC_LIVE_FTP_CAPS`.
