-- The SSH host keys a person trusted (RC-050-07): one row per host name, port and key type,
-- read and written through `rc_db::Db`'s `known_hosts` methods. `public_key` is the key's wire
-- blob in base64 as `known_hosts` spells it, `fingerprint_sha256` the `SHA256:...` form
-- `ssh-keygen -lf` prints. A hashed name imported from `known_hosts` (`|1|salt|hash`) keeps its
-- hash in `host` and port 0: the real port is inside the hash, checked by HMAC-SHA1.
CREATE TABLE known_hosts (
    id                 INTEGER PRIMARY KEY,
    host               TEXT NOT NULL CHECK (host <> ''),
    port               INTEGER NOT NULL CHECK (port BETWEEN 0 AND 65535),
    algorithm          TEXT NOT NULL CHECK (algorithm <> ''),
    public_key         TEXT NOT NULL CHECK (public_key <> ''),
    fingerprint_sha256 TEXT NOT NULL CHECK (fingerprint_sha256 LIKE 'SHA256:%'),
    added_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_seen_at       TEXT,
    source             TEXT NOT NULL CHECK (source IN ('user', 'import')),
    UNIQUE (host, port, algorithm)
) STRICT;
