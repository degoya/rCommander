-- The vault (rc-vault, ADR 0002): how the key is derived from the master password and the value
-- that tells a right password from a wrong one. No row means no vault, and then the master
-- password is never asked. At most one row (`id = 1`).
--
-- `verifier` is the AEAD ciphertext of a fixed plaintext under the derived key; the KDF
-- parameters are stored, not assumed, so a later build can raise them and still open this vault.
CREATE TABLE vault_meta (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    version     INTEGER NOT NULL,
    kdf         TEXT NOT NULL,
    m_cost_kib  INTEGER NOT NULL,
    t_cost      INTEGER NOT NULL,
    p_cost      INTEGER NOT NULL,
    salt        BLOB NOT NULL,
    verifier    BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
