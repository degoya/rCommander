-- Workspaces (RC-100-68): what both panels show, saved under a name to call up at once - the tabs
-- of each panel with their folders, the file list or a terminal, the active panel, the divider -
-- and a colour theme for each mode while it is in force. `spec` is the engine's versioned JSON
-- (`rc_engine::workspaces::WorkspaceSpec`: folders, connection ids, shell names, never a secret);
-- the engine checks it on the way in and on the way back. `sort_order` is the person's order in
-- the list. The workspaces are made and removed by a person and kept as they are
-- (`rc_db::DURABLE_TABLES`).
CREATE TABLE workspaces (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(name)) > 0),
    sort_order  INTEGER NOT NULL DEFAULT 0,
    spec        TEXT NOT NULL CHECK (json_valid(spec)),
    theme_light TEXT,
    theme_dark  TEXT,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
