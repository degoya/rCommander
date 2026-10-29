-- Where an imported connection came from (RC-050-06): the source it was read from and its key
-- there, so that importing the same source again updates the connection instead of adding a
-- second one. Both stay NULL for a connection made by hand.
--
-- The ssh_config import writes `ssh_config:<file>` and the host alias in ASCII lower case. The
-- partial unique index lets a source name each key once, even when two processes import at once.

ALTER TABLE connections ADD COLUMN import_source TEXT;
ALTER TABLE connections ADD COLUMN import_key TEXT;

CREATE UNIQUE INDEX connections_import
    ON connections (import_source, import_key) WHERE import_source IS NOT NULL;
