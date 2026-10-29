//! The page Transfers: the copy buffer, how many transfers run at once, how a broken connection
//! is retried (RC-060-05, RC-060-07), and the queue (RC-010-14, owner 2026-10-18).
//!
//! One job per item: on, a copy, move or delete of several sources, from the window or over MCP,
//! becomes one job per source in the queue, in their order, each paused, moved and cancelled
//! on its own while an answer "for all" holds for all of them; off, it is one job as before. The
//! queue in a window of its own: on, Ctrl+Shift+Q opens the queue in a native window beside the
//! main window, which stays usable; off, the queue lies over the main window as a dialog. Neither
//! is protected: either way the same files are worked on, and nothing is shown that the queue
//! does not show anyway. The jobs run at once in the queue window: on, the jobs F5, F6 and F8
//! confirmed with OK run at once are listed there as a group of their own (RC-010-25); off, the
//! window lists the queue's lists alone, while the background bar still shows every job. The
//! speed together: on, the queue window shows the sum of its running jobs' speeds once more than
//! one runs (RC-010-26); off, only the selected job's. One lane: on, the queue window leaves out
//! its lane column while every job it lists is in the same lane (all local, say); off, the column
//! always shows. Tools at once: on, packing, unpacking, checksums, synchronizing and
//! multi-rename run at once, outside the queue's lists, as F5, F6 and F8 confirmed with OK do
//! (RC-010-28); off, they go into the default list and wait in their lane as before.

use super::{SettingKind, SettingPage, SettingSpec};

/// Bytes per read and write of a copy, in KiB.
pub const TRANSFER_BUFFER_KEY: &str = "transfers.buffer_kib";
/// Copies and moves that run at once on one server's connection.
pub const TRANSFER_PER_CONNECTION_KEY: &str = "transfers.per_connection";
/// Copies and moves that run at once in all.
pub const TRANSFER_TOTAL_KEY: &str = "transfers.max_running";
/// How often a transfer whose connection broke is retried before it pauses (RC-060-07).
pub const TRANSFER_RETRY_COUNT_KEY: &str = "transfers.retry_count";
/// The wait before the first retry, in seconds; each further one doubles it.
pub const TRANSFER_RETRY_DELAY_KEY: &str = "transfers.retry_delay_secs";
/// Whether a copy, move or delete of several sources becomes one job per source.
pub const ONE_JOB_PER_ITEM_KEY: &str = "queue.one_job_per_item";
/// Whether the queue opens in a window of its own, the main window usable beside it.
pub const QUEUE_WINDOW_OWN_KEY: &str = "queue.window_own";
/// How many jobs the background bar under the panels lists; the queue window has them all
/// (owner, 2026-10-19).
pub const STRIP_ROWS_KEY: &str = "queue.strip_rows";
/// Whether the queue window lists the jobs run at once beside the queued ones (RC-010-25).
pub const SHOW_AT_ONCE_KEY: &str = "queue.show_at_once";
/// Whether the queue window shows the speed of all its running jobs together (RC-010-26).
pub const TOTAL_SPEED_KEY: &str = "queue.total_speed";
/// Whether the queue window leaves out its lane column while every job is in the same lane
/// (RC-010-26).
pub const HIDE_SINGLE_LANE_KEY: &str = "queue.hide_single_lane";
/// Whether packing, unpacking, checksums, synchronizing and multi-rename run at once, never in a
/// lane behind a copy (RC-010-28).
pub const TOOLS_AT_ONCE_KEY: &str = "queue.tools_at_once";

pub(super) const BUFFER: SettingSpec = SettingSpec {
    key: TRANSFER_BUFFER_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 16,
        max: 16384,
        default: (rc_ops::DEFAULT_BUFFER_SIZE / 1024) as i64,
    },
    label: "setting.transfers.buffer_kib",
    help: "setting.transfers.buffer_kib.help",
    protected: false,
    restart: false,
};

pub(super) const PER_CONNECTION: SettingSpec = SettingSpec {
    key: TRANSFER_PER_CONNECTION_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 1,
        max: 8,
        default: rc_ops::DEFAULT_PER_CONNECTION as i64,
    },
    label: "setting.transfers.per_connection",
    help: "setting.transfers.per_connection.help",
    protected: false,
    restart: false,
};

pub(super) const TOTAL: SettingSpec = SettingSpec {
    key: TRANSFER_TOTAL_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 1,
        max: 16,
        default: rc_ops::DEFAULT_MAX_RUNNING as i64,
    },
    label: "setting.transfers.max_running",
    help: "setting.transfers.max_running.help",
    protected: false,
    restart: false,
};

pub(super) const RETRY_COUNT: SettingSpec = SettingSpec {
    key: TRANSFER_RETRY_COUNT_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 0,
        max: 10,
        default: rc_ops::DEFAULT_RETRY_ATTEMPTS as i64,
    },
    label: "setting.transfers.retry_count",
    help: "setting.transfers.retry_count.help",
    protected: false,
    restart: false,
};

pub(super) const RETRY_DELAY: SettingSpec = SettingSpec {
    key: TRANSFER_RETRY_DELAY_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 1,
        max: 300,
        default: rc_ops::DEFAULT_RETRY_DELAY.as_secs() as i64,
    },
    label: "setting.transfers.retry_delay_secs",
    help: "setting.transfers.retry_delay_secs.help",
    protected: false,
    restart: false,
};

pub(super) const ONE_JOB_PER_ITEM: SettingSpec = SettingSpec {
    key: ONE_JOB_PER_ITEM_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.queue.one_job_per_item",
    help: "setting.queue.one_job_per_item.help",
    protected: false,
    restart: false,
};

pub(super) const QUEUE_WINDOW_OWN: SettingSpec = SettingSpec {
    key: QUEUE_WINDOW_OWN_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.queue.window_own",
    help: "setting.queue.window_own.help",
    protected: false,
    restart: false,
};

pub(super) const STRIP_ROWS: SettingSpec = SettingSpec {
    key: STRIP_ROWS_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Int {
        min: 1,
        max: 50,
        default: 5,
    },
    label: "setting.queue.strip_rows",
    help: "setting.queue.strip_rows.help",
    protected: false,
    restart: false,
};

pub(super) const SHOW_AT_ONCE: SettingSpec = SettingSpec {
    key: SHOW_AT_ONCE_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.queue.show_at_once",
    help: "setting.queue.show_at_once.help",
    protected: false,
    restart: false,
};

pub(super) const TOTAL_SPEED: SettingSpec = SettingSpec {
    key: TOTAL_SPEED_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.queue.total_speed",
    help: "setting.queue.total_speed.help",
    protected: false,
    restart: false,
};

pub(super) const HIDE_SINGLE_LANE: SettingSpec = SettingSpec {
    key: HIDE_SINGLE_LANE_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.queue.hide_single_lane",
    help: "setting.queue.hide_single_lane.help",
    protected: false,
    restart: false,
};

pub(super) const TOOLS_AT_ONCE: SettingSpec = SettingSpec {
    key: TOOLS_AT_ONCE_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.queue.tools_at_once",
    help: "setting.queue.tools_at_once.help",
    protected: false,
    restart: false,
};
