# Live SFTP tests (RC-080-14)

The engine without a window, on Linux, against real OpenSSH servers in Docker: host keys, the
SFTP panel, copies in both directions and between servers, resume after a dropped connection
and after a restart, the `sudo -i` login script, `ssh.exec`, a locked vault; vim and htop in
the SSH terminal through `terminal.send_input`/`read_screen` (RC-060-03), keepalive and
reconnect after a hang and a restart (RC-060-01), a folder of 100 000 entries (RC-060-02; its times and memory for `docs/performance.md`), and
servers paused, killed or stopped mid-listing, mid-upload and mid-terminal (RC-080-04). Beside
them `crates/rcommander/tests/live_mcp.rs` starts the binary as `rcommander mcp`, headless, with a
data folder of its own: the connection tree, a listing of the server, a copy up and one down byte
for byte, `ssh_exec` refused without a person and run with a confirmation code, a server whose
key nobody trusted refused. Opt-in: the tests are `#[ignore]` and skip themselves without the
servers, and `scripts/check.sh` never runs them. The workflow `.github/workflows/live.yml` runs
this suite and the FTP one (`../ftp/`) every week on GitHub and on request (RC-110-69); in the
development repository `scripts/live-tests.sh` runs both under one build lock.

```bash
scripts/live-servers/sftp/run.sh             # start the servers, run every live test, remove the servers
scripts/live-servers/sftp/run.sh resume      # only the tests whose name contains "resume"
```

`run.sh` takes the build lock like every heavy script, builds the image, starts the servers,
runs `cargo nextest run --locked --cargo-profile release-test -p rc-engine -p rcommander --test live_sftp
--test live_mcp --run-ignored only` and removes every container and the network on its way out, after a failure
or Ctrl+C too (`RC_LIVE_SFTP_KEEP=1` leaves them running). Optimised, because the tests print
the times a person gets.

| File | What it is |
| --- | --- |
| `Dockerfile` | OpenSSH, `sftp`, `sudo`, `vim` and `htop` on `alpine:3.22.2` (pinned by digest); user `tester`, dummy password `rc-live-dummy`, `sudo` with that password |
| `sshd_config` | Password login, `internal-sftp`, `LogLevel DEBUG1` (every `userauth-request` is logged) |
| `entrypoint.sh` | Host keys on the first start, `/srv/readonly` (0555) for a failing target, the SFTP log level of `SFTP_LOG` |
| `ctl.sh` | `up`, `down`, and what the tests need from Docker: `rekey`, `kill-sessions`, `stop`/`start`, `kill`, `pause`/`unpause`, `sha256`, `exec`, `logs`, `fingerprints` |
| `run.sh` | The run, as above |

The servers, one container each (`rc-live-sftp-<role>`, network `rc-live-sftp-net`), on
127.0.0.1 only, from `RC_LIVE_SFTP_PORT_BASE` (2201): `a` 2201 and `b` 2202 (panel, transfers,
server to server, shell, vault), `keys` 2203 (gets a new host key mid-test), `drop` 2204,
`spent` 2205, `restart` 2206, `hang` 2207 (one resume test each; their SFTP log names the offset
of every read), `alive` 2208 (keepalive and reconnect), `faults-list` 2209, `faults-copy` 2210,
`faults-term` 2211 (one fault case each), `many` 2212 (the 100 000 entries, untouched), `lost`
2213 (dropped sessions). The first seven hold `/home/tester/big.bin`, `RC_LIVE_SFTP_BIG_MB` (300)
MiB of random bytes; `many` and `faults-list` hold `/home/tester/many`, 100 000 empty files.

On a CI runner, which runs one job, `RC_NO_LOCK=1` and `RC_MIN_FREE_MB=0` skip the build lock
and its memory gate (`scripts/lib/lock.sh`).

Needs Docker; a Docker Desktop config whose credential helper does not run under WSL is worked
around with an empty `DOCKER_CONFIG` of the script's own.
