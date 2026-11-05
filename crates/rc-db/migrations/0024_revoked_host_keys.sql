-- The SSH host keys an OpenSSH `known_hosts` file marks `@revoked` (RC-080-67): a connection
-- whose server presents one of them stops hard, before any trust question, as OpenSSH does. One
-- row per key, whatever the host: `hosts` is the line's name field as written, shown to a
-- person and never matched. `public_key` is the key's wire blob in base64, `fingerprint_sha256`
-- the `SHA256:...` form `ssh-keygen -lf` prints, `source` the file the line came from. Only a
-- person removes a row, in the settings' Host keys page.
CREATE TABLE revoked_host_keys (
    id                 INTEGER PRIMARY KEY,
    algorithm          TEXT NOT NULL CHECK (algorithm <> ''),
    public_key         TEXT NOT NULL CHECK (public_key <> ''),
    fingerprint_sha256 TEXT NOT NULL UNIQUE CHECK (fingerprint_sha256 LIKE 'SHA256:%'),
    hosts              TEXT NOT NULL,
    source             TEXT NOT NULL,
    added_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
