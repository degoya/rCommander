//! The coverage rows of the `queue` group (`rc-engine`'s `registry/queue.rs`), sorted by id.

use super::Coverage;

pub(super) const ROWS: &[(&str, Coverage)] = &[
    ("queue.assign", Coverage::Covered("queue_assign")),
    ("queue.assign_many", Coverage::Covered("queue_assign_many")),
    ("queue.clear", Coverage::Covered("queue_clear")),
    (
        "queue.clear_finished",
        Coverage::Covered("queue_clear_finished"),
    ),
    ("queue.create", Coverage::Covered("queue_create")),
    ("queue.delete", Coverage::Covered("queue_delete")),
    (
        "queue.delete_saved",
        Coverage::Covered("queue_delete_saved"),
    ),
    ("queue.discard", Coverage::Covered("queue_discard")),
    ("queue.list", Coverage::Covered("queue_list")),
    ("queue.lists", Coverage::Covered("queue_lists")),
    ("queue.load", Coverage::Covered("queue_load")),
    ("queue.move", Coverage::Covered("queue_move")),
    ("queue.move_many", Coverage::Covered("queue_move_many")),
    ("queue.pause", Coverage::Covered("queue_pause")),
    ("queue.pause_all", Coverage::Covered("queue_pause_all")),
    ("queue.remove", Coverage::Covered("queue_remove")),
    ("queue.rename", Coverage::Covered("queue_rename")),
    ("queue.resume_all", Coverage::Covered("queue_resume_all")),
    ("queue.save", Coverage::Covered("queue_save")),
    ("queue.start", Coverage::Covered("queue_start")),
    (
        "queue.window",
        Coverage::Omitted("opens a window; the queue_ tools do what it offers"),
    ),
];
