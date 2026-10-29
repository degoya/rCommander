-- Login scripts (RC-050-08): what a session types after it logged in, such as `sudo -i` and the
-- password it asks for. `steps` is the JSON array of rc-remote's `login_script::Step` under
-- `schema_version`; a step that sends a secret names where it is stored, never its value.
--
-- A script's own secrets live in `login_script_secrets`: each `value_enc` is an rc-vault
-- ciphertext with "login_script_secrets/value_enc/<id>" as associated data, listed in
-- `rc_db::SECRET_COLUMNS` so a new master password re-encrypts it. Deleting a script deletes its
-- secrets with it.
--
-- `connections.login_script_id` (migration 0003) names the script a connection runs; a column
-- added without a foreign key cannot get one later in SQLite, so a trigger clears it when its
-- script is deleted.

CREATE TABLE login_scripts (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    name             TEXT NOT NULL CHECK (length(trim(name)) > 0),
    schema_version   INTEGER NOT NULL,
    steps            TEXT NOT NULL CHECK (json_valid(steps)),
    step_timeout_ms  INTEGER NOT NULL CHECK (step_timeout_ms > 0),
    total_timeout_ms INTEGER NOT NULL CHECK (total_timeout_ms > 0),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A name once, ignoring ASCII case.
CREATE UNIQUE INDEX login_scripts_name ON login_scripts (name COLLATE NOCASE);

CREATE TABLE login_script_secrets (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id  INTEGER NOT NULL REFERENCES login_scripts (id) ON DELETE CASCADE,
    label      TEXT NOT NULL CHECK (length(trim(label)) > 0),
    value_enc  BLOB NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A label once per script, ignoring ASCII case.
CREATE UNIQUE INDEX login_script_secrets_label
    ON login_script_secrets (script_id, label COLLATE NOCASE);

CREATE TRIGGER login_scripts_unlink AFTER DELETE ON login_scripts
BEGIN
    UPDATE connections SET login_script_id = NULL WHERE login_script_id = OLD.id;
END;
