-- Saved transfer lists (RC-010-15): a list of the operations queue a person saved under a name,
-- to load it again later - "Backup NAS", "Upload web server". `saved_queue_entries` holds its
-- jobs in their order; `spec` is a job as it would be submitted (`rc_ops::JobSpec` as JSON:
-- sources, target, kind, options - paths and connection ids, never a secret). The lists are made
-- and removed by a person and kept as they are (`rc_db::DURABLE_TABLES`); finished jobs are never
-- recorded here. `restore_on_start` loads the list again, stopped, when the app starts.
CREATE TABLE saved_queues (
    id               INTEGER PRIMARY KEY,
    name             TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(name)) > 0),
    restore_on_start INTEGER NOT NULL DEFAULT 0 CHECK (restore_on_start IN (0, 1)),
    saved_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE saved_queue_entries (
    queue_id INTEGER NOT NULL REFERENCES saved_queues (id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK (position >= 0),
    spec     TEXT NOT NULL CHECK (json_valid(spec)),
    PRIMARY KEY (queue_id, position)
) STRICT;
