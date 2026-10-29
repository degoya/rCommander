-- The secrets of a connection (RC-050-03): its password, its private key (the key file's
-- content: OpenSSH, PEM, PKCS#8 or PuTTY .ppk, taken into the vault so a backup of this file
-- carries it) and the passphrase of that key or of the file at `key_path`. Each value is an
-- rc-vault ciphertext (`v1 | nonce | ciphertext`, ADR 0002) with "connections/<column>/<id>" as
-- associated data, so a value copied into another row or column does not decrypt there. NULL
-- means nothing is stored; the plaintext never reaches this file.
--
-- `rc_db::SECRET_COLUMNS` names the three columns, so a new master password re-encrypts them in
-- one transaction. Deleting a connection deletes its secrets with the row.
--
-- The public half of a stored key is kept in clear for display, set and cleared together with
-- `key_enc`: its type (`ssh-ed25519`), its SHA-256 fingerprint as `ssh-keygen -lf` prints it, its
-- comment and the `authorized_keys` form of the public key.

ALTER TABLE connections ADD COLUMN password_enc BLOB;
ALTER TABLE connections ADD COLUMN key_enc BLOB;
ALTER TABLE connections ADD COLUMN passphrase_enc BLOB;
ALTER TABLE connections ADD COLUMN key_type TEXT;
ALTER TABLE connections ADD COLUMN key_fingerprint TEXT;
ALTER TABLE connections ADD COLUMN key_comment TEXT;
ALTER TABLE connections ADD COLUMN key_public TEXT;
