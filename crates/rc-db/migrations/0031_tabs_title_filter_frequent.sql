-- A tab's own name and its file filter, and the most visited folders (RC-100-92).
--
-- `tabs.title` is the name a person gave the tab (Total Commander's "Rename tab"), NULL while it
-- shows its folder's name; the engine checks it on the way back (at most 64 characters, no control
-- characters). `tabs.file_filter` is the engine's `FileFilter` as JSON (all files, programs, or a
-- mask; Ctrl+F10, Ctrl+F11, Ctrl+F12), 'null' for a row an older version wrote.
ALTER TABLE tabs ADD COLUMN title TEXT;
ALTER TABLE tabs ADD COLUMN file_filter TEXT NOT NULL DEFAULT 'null';

-- The folders the panels showed, counted, for the lower end of the history list (Alt+Down), as
-- Total Commander 11 shows its frequently used folders. `location` is the engine's `VfsPath` as
-- JSON, one row per folder. `visits` is halved for every 30 days since `counted_since` (unix
-- seconds), so a folder once used often fades. It grows with use, so it is a retained table
-- (`rc_db::RETAINED_TABLES`): the sweep drops folders not visited for long by `last_visit` and,
-- past the row limit, the oldest rows by `id`.
CREATE TABLE dir_frequent (
    id            INTEGER PRIMARY KEY,
    location      TEXT NOT NULL UNIQUE CHECK (length(location) <= 8192),
    visits        INTEGER NOT NULL DEFAULT 1 CHECK (visits >= 0),
    counted_since INTEGER NOT NULL,
    last_visit    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX dir_frequent_last_visit ON dir_frequent (last_visit);
