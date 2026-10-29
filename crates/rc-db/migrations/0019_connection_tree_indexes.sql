-- Plain indexes on the connection tree's link columns (RC-080-54). The unique name indexes of
-- migrations 0003 and 0018 index `ifnull(folder_id, 0)` and `ifnull(parent_id, 0)`, an
-- expression no `WHERE folder_id = ?` and no foreign-key cascade can use: deleting a folder, or
-- listing what is in one, read every row of the table. `connections.login_script_id` had no
-- index at all, so the trigger of migration 0009 scanned `connections` for every deleted script.

CREATE INDEX IF NOT EXISTS connection_folders_parent ON connection_folders (parent_id);

CREATE INDEX IF NOT EXISTS connections_folder ON connections (folder_id);

CREATE INDEX IF NOT EXISTS connections_login_script ON connections (login_script_id);

CREATE INDEX IF NOT EXISTS credentials_folder ON credentials (folder_id);
