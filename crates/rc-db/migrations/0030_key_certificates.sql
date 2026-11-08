-- OpenSSH user certificates (RC-060-09): the certificate a CA signed for a private key stored in
-- the vault, kept beside it in clear like the key's public half (migration 0006) - a certificate
-- is public. `key_certificate` is its `-cert.pub` line as `ssh-keygen` wrote it, what the login
-- sends; `key_certificate_info` is what the window and MCP show of it (key id, serial,
-- principals, validity, the CA's fingerprint) as JSON, read once when it was taken in. Both are
-- NULL without a certificate and are written and cleared with the key. A shared credential gets
-- the same columns (migration 0018), so the two rows keep the same key columns.

ALTER TABLE connections ADD COLUMN key_certificate TEXT;
ALTER TABLE connections ADD COLUMN key_certificate_info TEXT;
ALTER TABLE credentials ADD COLUMN key_certificate TEXT;
ALTER TABLE credentials ADD COLUMN key_certificate_info TEXT;
