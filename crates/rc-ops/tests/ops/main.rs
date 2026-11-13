//! The queue's integration tests, one binary for all of them so a run links it once (RC-080-49):
//! each module was a test binary of its own, `support` is what they share. A test's name is its
//! module and its function, e.g. `cargo nextest run -p rc-ops --test ops transfer::`.
//!
//! Tests here share one process under `cargo test` (nextest runs each in a process of its own):
//! none sets an environment variable or the current directory, every test server binds port 0,
//! every file is in memory or in a temporary directory of its own, and the copies within one FTP
//! server take turns on a lock of the process (`rc-ops` `stream_pair.rs`) whichever test runs them.
//! `memory_profile.rs` stays a binary of its own: it measures the peak resident size of the
//! whole process, which any other test would raise.

mod support;

mod archive_test;
mod archive_update;
mod batch;
mod case_rename;
mod checksum;
mod conflicts;
mod connection_faults;
mod control;
mod crash_restart;
mod device_transfer;
mod errors;
mod external_tools;
mod extract;
mod links;
mod no_replace;
mod overwrite_replace;
mod pack;
mod queue_at_once;
mod queue_bulk;
mod queue_lists;
mod queue_order;
mod queue_running;
mod read_guard;
mod remote_lanes;
mod remote_remote;
mod remote_remote_failures;
mod remote_throughput;
mod remote_transfer;
mod rename;
mod rename_rollback;
mod restore;
mod resume;
mod sftp_case;
mod sftp_transfer;
mod sync_before_remove;
mod transfer;
mod transfer_as;
