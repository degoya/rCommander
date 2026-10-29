-- Shared credentials (RC-050-12): a user and the secrets that go with it - password, private key,
-- passphrase - kept once in the connection tree, and any number of connections pointing at it.
-- Changing the password of the entry changes it for every connection that uses it.
--
-- The secrets are rc-vault ciphertexts as a connection's are (migration 0006), with
-- "credentials/<column>/<id>" as associated data, so a value copied from a connection's row or
-- into another entry's does not decrypt there; `rc_db::SECRET_COLUMNS` lists the three columns,
-- so a new master password re-encrypts them in the same transaction as every other secret. The
-- public half of a stored key is kept in clear for display, as for a connection. Ids are
-- AUTOINCREMENT and never reused, because the vault binds a secret to its row id.
--
-- An entry sits in a folder of the tree (the root for NULL) and goes with it when the folder is
-- deleted. `connections.credential_id` names the entry a connection takes its user and secrets
-- from; deleting the entry clears it, and the connection falls back to its own. Kept
-- (`rc_db::DURABLE_TABLES`).

CREATE TABLE credentials (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    folder_id       INTEGER REFERENCES connection_folders (id) ON DELETE CASCADE,
    name            TEXT NOT NULL CHECK (length(trim(name)) > 0),
    username        TEXT,
    password_enc    BLOB,
    key_enc         BLOB,
    passphrase_enc  BLOB,
    key_type        TEXT,
    key_fingerprint TEXT,
    key_comment     TEXT,
    key_public      TEXT,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A name once per folder, ignoring ASCII case; the root is folder 0.
CREATE UNIQUE INDEX credentials_name ON credentials (ifnull(folder_id, 0), name COLLATE NOCASE);

ALTER TABLE connections ADD COLUMN credential_id INTEGER
    REFERENCES credentials (id) ON DELETE SET NULL;

CREATE INDEX connections_credential ON connections (credential_id);
