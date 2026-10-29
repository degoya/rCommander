-- Folder tabs, their folder history and the directory hotlist (RC-010-03).
--
-- `tabs` holds the tabs of both panels as the window last left them: written whole when the app
-- ends (`rc_db::Db::save_tabs`) and read when it starts. `location` is the engine's `VfsPath` as
-- JSON, so a tab on a server or inside an archive needs no new column; `sort` and `mode` are its
-- listing as JSON too. The engine checks every value when it reads it back.
CREATE TABLE tabs (
    id       INTEGER PRIMARY KEY,
    panel    TEXT NOT NULL CHECK (panel IN ('left', 'right')),
    position INTEGER NOT NULL,
    active   INTEGER NOT NULL DEFAULT 0 CHECK (active IN (0, 1)),
    locked   INTEGER NOT NULL DEFAULT 0 CHECK (locked IN (0, 1)),
    location TEXT NOT NULL,
    sort     TEXT NOT NULL DEFAULT 'null',
    mode     TEXT NOT NULL DEFAULT 'null'
) STRICT;

-- Each tab's folder history around its folder: `step` -1, -2, ... are the folders behind it
-- (Alt+Left), 1, 2, ... those ahead (Alt+Right). It grows with use, so it is a retained table
-- (`rc_db::RETAINED_TABLES`): `id` is the insertion order the sweep deletes by, and a save
-- inserts the farthest steps first, so the sweep takes every tab's oldest folders first.
CREATE TABLE dir_history (
    id       INTEGER PRIMARY KEY,
    tab_id   INTEGER NOT NULL REFERENCES tabs (id) ON DELETE CASCADE,
    step     INTEGER NOT NULL CHECK (step <> 0),
    location TEXT NOT NULL,
    saved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX dir_history_tab ON dir_history (tab_id, step);
CREATE INDEX dir_history_saved_at ON dir_history (saved_at);

-- The directory hotlist (Ctrl+D). A row without a location is a folder (Total Commander's
-- submenu) and holds entries and folders; an entry has a location and may have a target for the
-- other panel, both `VfsPath` JSON. Ids are never reused (AUTOINCREMENT): commands name a row by
-- its id. That a parent is a folder and a folder never moves into itself is checked in
-- `rc_db::Db`'s hotlist methods, inside each write's transaction.
CREATE TABLE hotlist (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id  INTEGER REFERENCES hotlist (id) ON DELETE CASCADE,
    name       TEXT NOT NULL CHECK (length(trim(name)) > 0),
    location   TEXT,
    target     TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE INDEX hotlist_parent ON hotlist (ifnull(parent_id, 0), sort_order);
