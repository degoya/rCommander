-- Server inventories (RC-100-39): an inventory plugin (`plugin_type = "inventory"`) lists the
-- hosts of a system such as NetBox or Proxmox, and rCommander keeps one folder of connections in
-- step with them.
--
-- `inventory_sources` is one such system: the plugin that reads it, the address the person
-- entered and confirmed (the only one the plugin's `http` reaches), the folder its connections
-- go into (NULL until the first sync creates one at the root, or after the person deleted it)
-- and how the last sync ended (`ok` or a failure code, never a message from the server).
--
-- `inventory_source_secrets` holds the values of the plugin's `secrets` slots, such as its API
-- token: each `value_enc` an rc-vault ciphertext with "inventory_source_secrets/value_enc/<id>"
-- as associated data, listed in `rc_db::SECRET_COLUMNS`. The plugin never reads one; the host
-- puts it into a request to the entered address in place of a `{{secret:<slot>}}` marker.
--
-- A connection of a source is marked by `import_source = 'inventory:<source id>'` and
-- `import_key = <the inventory's id of the host>` (migration 0007), so a host is matched again
-- however it was renamed. `inventory_missing_since` is when a sync first missed it: a host that
-- left the inventory is marked, never deleted. Deleting a source keeps its connections as
-- ordinary ones.
CREATE TABLE inventory_sources (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    name            TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 255),
    plugin_id       TEXT NOT NULL CHECK (length(plugin_id) = 36),
    address         TEXT NOT NULL CHECK (length(address) BETWEEN 1 AND 2048),
    folder_id       INTEGER REFERENCES connection_folders (id) ON DELETE SET NULL,
    last_sync_at    TEXT,
    last_outcome    TEXT CHECK (last_outcome IS NULL OR length(last_outcome) <= 64),
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A name once, ignoring ASCII case.
CREATE UNIQUE INDEX inventory_sources_name ON inventory_sources (name COLLATE NOCASE);

CREATE TABLE inventory_source_secrets (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id  INTEGER NOT NULL REFERENCES inventory_sources (id) ON DELETE CASCADE,
    slot       TEXT NOT NULL CHECK (length(slot) BETWEEN 1 AND 32),
    value_enc  BLOB NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- A slot once per source.
CREATE UNIQUE INDEX inventory_source_secrets_slot ON inventory_source_secrets (source_id, slot);

ALTER TABLE connections ADD COLUMN inventory_missing_since TEXT;

CREATE TRIGGER inventory_sources_unlink AFTER DELETE ON inventory_sources
BEGIN
    UPDATE connections
       SET import_source = NULL, import_key = NULL, inventory_missing_since = NULL
     WHERE import_source = 'inventory:' || OLD.id;
END;
