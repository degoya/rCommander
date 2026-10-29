-- Where a connection's secret lives when it is not in the vault (RC-100-34): a secret source - an
-- installed secrets plugin (`plugin:<id>`, RC-100-33) or, from RC-100-37, the Windows credential
-- manager (`wincred`) - and the entry in it. Only the reference is stored, never a value: the
-- engine asks the source at every connect. At most one reference per connection and kind; one
-- set takes precedence over a value stored in the vault for the same kind.
--
-- `entry_id` is the id the source's search handed out; `entry_label` is what the form shows
-- (title and user), neither a secret. Deleting a connection deletes its references. Kept
-- (`rc_db::DURABLE_TABLES`).
CREATE TABLE secret_sources (
    connection_id INTEGER NOT NULL REFERENCES connections (id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('password', 'key', 'passphrase')),
    source        TEXT NOT NULL
                  CHECK (source = 'wincred' OR (source GLOB 'plugin:*' AND length(source) = 43)),
    entry_id      TEXT NOT NULL CHECK (length(entry_id) BETWEEN 1 AND 256),
    entry_label   TEXT NOT NULL CHECK (length(entry_label) <= 512),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    PRIMARY KEY (connection_id, kind)
) STRICT;
