-- One-time codes for the SSH login (RC-060-08): a connection's TOTP seed, an rc-vault ciphertext
-- like its other secrets (migration 0006) with "connections/totp_seed_enc/<id>" as associated
-- data. The plaintext is what the person pasted - an `otpauth://totp/...` URI or a bare Base32
-- secret - and never reaches this file. NULL means no seed: a server's question for a code goes
-- to the person. A shared credential gets the same column (migration 0018), so the two rows keep
-- the same secret columns; `rc_db::SECRET_COLUMNS` lists both, so a new master password
-- re-encrypts them with every other secret.
--
-- `connection_session_prefs.totp_prompt` is the connection's own pattern for the question that
-- asks for the code; NULL takes the default (`rc_remote::totp::DEFAULT_PROMPT`). It is no secret.

ALTER TABLE connections ADD COLUMN totp_seed_enc BLOB;
ALTER TABLE credentials ADD COLUMN totp_seed_enc BLOB;
ALTER TABLE connection_session_prefs ADD COLUMN totp_prompt TEXT
    CHECK (totp_prompt IS NULL OR length(totp_prompt) BETWEEN 1 AND 512);
