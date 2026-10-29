-- Plugins (RC-100-09, docs/plugins.md "Installation and lifecycle"). The packages themselves lie
-- in `<data dir>/plugins/<id>/<version>/`; these tables keep what a person decided about them.
--
-- `plugins`: one row per installed plugin id - switched on or off, the version a person chose
-- (`active_version`, a rollback), one under test (`staged_version`), an update installed beside a
-- running version that takes over at the next start (`pending_version`), and whether it came
-- with rCommander. Kept (`rc_db::DURABLE_TABLES`); a removed plugin's row goes with it.
CREATE TABLE plugins (
    id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
    enabled         INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    active_version  TEXT,
    staged_version  TEXT,
    pending_version TEXT,
    bundled         INTEGER NOT NULL DEFAULT 0 CHECK (bundled IN (0, 1)),
    installed_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- `plugin_keys`: the signing keys a person confirmed on first use, each `key_id` pinned to its
-- key, and the keys a person revoked (`revoked_at`). A public key, never a secret. Kept.
CREATE TABLE plugin_keys (
    key_id      TEXT PRIMARY KEY NOT NULL CHECK (length(trim(key_id)) > 0),
    public_key  TEXT NOT NULL,
    fingerprint TEXT NOT NULL UNIQUE CHECK (length(fingerprint) = 64),
    trusted_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    revoked_at  TEXT
) STRICT;

-- `plugin_digest_revocations`: single package versions withdrawn by their content digest, whoever
-- signed them. Kept.
CREATE TABLE plugin_digest_revocations (
    digest     TEXT PRIMARY KEY NOT NULL CHECK (length(digest) = 64),
    reason     TEXT,
    revoked_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- `plugin_executions`: every plugin call - plugin, version, kind, export, outcome, the failure's
-- code, how long - never a message a plugin wrote. Grows with use, so it is in
-- `rc_db::RETAINED_TABLES`, cut by the shared sweep by rows and age; `id` is the insertion order
-- the sweep deletes by.
CREATE TABLE plugin_executions (
    id             INTEGER PRIMARY KEY,
    recorded_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    plugin_id      TEXT NOT NULL,
    plugin_version TEXT NOT NULL,
    plugin_type    TEXT NOT NULL,
    operation      TEXT NOT NULL,
    outcome        TEXT NOT NULL,
    code           TEXT,
    duration_ms    INTEGER NOT NULL CHECK (duration_ms >= 0)
) STRICT;

CREATE INDEX plugin_executions_recorded_at ON plugin_executions (recorded_at);
CREATE INDEX plugin_executions_plugin ON plugin_executions (plugin_id, id);
