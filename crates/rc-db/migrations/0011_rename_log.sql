-- The multi-rename undo log (RC-020-01): one row per step of a run, in the order the steps are
-- carried out, written before the first of them and marked `done` once each one is. `steps`
-- repeats the run's length on every row, so a run the sweep has cut is recognised and never
-- undone in part. `dir` is the directory as the engine names it (a `VfsPath` as JSON). Kept
-- within its row and age limit by the retention sweep (`rc_db::RETAINED_TABLES`); `id` is the
-- insertion order the sweep deletes by.
CREATE TABLE rename_log (
    id          INTEGER PRIMARY KEY,
    recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    run         TEXT NOT NULL,
    dir         TEXT NOT NULL,
    seq         INTEGER NOT NULL CHECK (seq >= 0),
    steps       INTEGER NOT NULL CHECK (steps > 0),
    from_name   TEXT NOT NULL,
    to_name     TEXT NOT NULL,
    done        INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1)),
    UNIQUE (run, seq)
) STRICT;

CREATE INDEX rename_log_recorded_at ON rename_log (recorded_at);
