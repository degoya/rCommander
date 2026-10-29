-- The FTP options of a connection (RC-070-01): how data connections are opened and how names
-- are encoded on a server that does not offer UTF-8. Both are NULL for an SSH connection, and
-- NULL on an FTP connection means the default: passive (`EPSV` before `PASV`) and Latin-1.
--
-- `ftp_data_mode` is 'passive' or 'active', `ftp_encoding` 'latin1' or 'utf8'. No CHECK, like
-- `kind` in 0003: the repository validates every write.

ALTER TABLE connections ADD COLUMN ftp_data_mode TEXT;
ALTER TABLE connections ADD COLUMN ftp_encoding TEXT;
