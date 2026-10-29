-- The error log: `warn` and `error` lines only, masked and capped by the tracing layer
-- (`rc_db::install_diagnostics`), kept within its row and age limit by the retention sweep
-- (`rc_db::RETAINED_TABLES`). `id` is the insertion order the sweep deletes by.
CREATE TABLE error_log (
    id          INTEGER PRIMARY KEY,
    recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    level       TEXT NOT NULL CHECK (level IN ('warn', 'error')),
    target      TEXT NOT NULL,
    message     TEXT NOT NULL,
    code        TEXT
) STRICT;

CREATE INDEX error_log_recorded_at ON error_log (recorded_at);
