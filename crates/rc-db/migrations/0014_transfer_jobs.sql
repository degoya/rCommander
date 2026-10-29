-- The transfer jobs to and from servers that have not ended (RC-060-07): one row per job from
-- the moment it starts until it completes, is cancelled or fails, so a job the app left in the
-- middle is put back into the queue at the next start and can resume. `spec` is the job as
-- submitted (`rc_ops::JobSpec` as JSON: sources, target, options - paths and connection ids,
-- never a secret); `file` is the file it was writing (`rc_ops::FileMark` as JSON: source and
-- target path, the source's size and modification time), NULL before the first one. Kept
-- within its row and age limit by the retention sweep (`rc_db::RETAINED_TABLES`): a job nobody
-- resumed is dropped in the end; `id` is the insertion order the sweep deletes by.
CREATE TABLE transfer_jobs (
    id          INTEGER PRIMARY KEY,
    recorded_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    spec        TEXT NOT NULL CHECK (json_valid(spec)),
    file        TEXT CHECK (file IS NULL OR json_valid(file))
) STRICT;

CREATE INDEX transfer_jobs_recorded_at ON transfer_jobs (recorded_at);
