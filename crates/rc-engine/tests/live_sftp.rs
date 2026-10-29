//! Live tests against real OpenSSH servers in Docker (RC-080-14): the engine without a window,
//! on Linux, doing what the owner's test lists check by hand on Windows - host keys, the SFTP
//! panel, copies in both directions and between servers, resume after a dropped connection and
//! after a restart, the `sudo -i` login script, `ssh.exec`, a locked vault; full-screen programs
//! in the SSH terminal (RC-060-03), keepalive and reconnect (RC-060-01), a folder of 100 000
//! entries (RC-060-02), servers that hang, drop or die mid-work (RC-080-04).
//!
//! Every test is `#[ignore]` and needs the servers: `scripts/live/sftp/run.sh` starts them, runs
//! `cargo nextest run --cargo-profile release-test -p rc-engine --test live_sftp --run-ignored
//! only` and removes them.

mod live;

#[path = "live/faults.rs"]
mod faults;
#[path = "live/listing.rs"]
mod listing;
#[path = "live/panel.rs"]
mod panel;
#[path = "live/reconnect.rs"]
mod reconnect;
#[path = "live/remote.rs"]
mod remote;
#[path = "live/resume.rs"]
mod resume;
#[path = "live/session.rs"]
mod session;
#[path = "live/shell.rs"]
mod shell;
#[path = "live/terminal.rs"]
mod terminal;
#[path = "live/transfer.rs"]
mod transfer;
