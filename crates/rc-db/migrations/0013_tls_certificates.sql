-- The FTPS server certificates a person trusted (RC-070-02): at most one row per host name and
-- port, read and written through `rc_db::Db`'s `tls_certificates` methods. `fingerprint_sha256`
-- is the SHA-256 of the end-entity certificate (DER) as `AB:CD:...` in upper-case hex; `subject`,
-- `issuer` and `not_after` (Unix seconds) are what the certificate said when it was trusted. A
-- server that presents another certificate is stopped (`certificate_changed`); a person removes
-- the row before another one is trusted, a stored row is never overwritten.
CREATE TABLE tls_certificates (
    id                 INTEGER PRIMARY KEY,
    host               TEXT NOT NULL CHECK (host <> ''),
    port               INTEGER NOT NULL CHECK (port BETWEEN 1 AND 65535),
    fingerprint_sha256 TEXT NOT NULL CHECK (length(fingerprint_sha256) = 95),
    subject            TEXT NOT NULL,
    issuer             TEXT NOT NULL,
    not_after          INTEGER NOT NULL,
    added_at           TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    last_seen_at       TEXT,
    UNIQUE (host, port)
) STRICT;
