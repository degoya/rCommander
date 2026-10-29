-- The connection tree (RC-050-02): folders and the connections inside them, read and written
-- through `rc_db::Db`'s connection methods, which validate before every write.
--
-- No secret lives in these tables. RC-050-03 adds encrypted BLOB columns to `connections` with
-- `ALTER TABLE ... ADD COLUMN`; the repository names its columns explicitly, so a later column
-- never reaches an answer. Ids are AUTOINCREMENT and never reused, because the vault binds a
-- secret to its row id (associated data, ADR 0002).
--
-- `kind`, `auth_method` and `default_view` carry no CHECK: FTP/FTPS (RC-070-01) and later auth
-- methods arrive in the same columns without rebuilding the table; the code validates them.
-- `login_script_id` has no foreign key yet: `login_scripts` arrives with RC-050-08.

CREATE TABLE connection_folders (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_id  INTEGER REFERENCES connection_folders (id) ON DELETE CASCADE,
    name       TEXT NOT NULL CHECK (length(trim(name)) > 0),
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A name once per parent, ignoring ASCII case; the root is parent 0.
CREATE UNIQUE INDEX connection_folders_name
    ON connection_folders (ifnull(parent_id, 0), name COLLATE NOCASE);

CREATE TABLE connections (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    folder_id       INTEGER REFERENCES connection_folders (id) ON DELETE CASCADE,
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    kind            TEXT NOT NULL,
    host            TEXT NOT NULL CHECK (length(trim(host)) > 0),
    port            INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    username        TEXT,
    auth_method     TEXT NOT NULL,
    key_path        TEXT,
    login_script_id INTEGER,
    default_view    TEXT NOT NULL DEFAULT 'files',
    start_directory TEXT,
    notes           TEXT,
    sort_order      INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A name once per folder, ignoring ASCII case; the root is folder 0.
CREATE UNIQUE INDEX connections_name
    ON connections (ifnull(folder_id, 0), name COLLATE NOCASE);
