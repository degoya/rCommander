-- Own commands and the Start menu (RC-100-86), as Total Commander keeps them in `usercmd.ini`
-- and in `wincmd.ini [user]`.
--
-- `user_commands` is one own command `em_<name>` (lower case, unique without regard to case),
-- run by a key, the palette, the Start menu or another own command. `kind` says what `command`
-- holds: a program (`program`, its path or a name found on the search path), keymap rows
-- separated by commas (`internal`, such as `copy,view_file`) or another own command (`own`, its
-- `em_` name). `params` are the program's parameters with Total Commander's `%` placeholders,
-- `folder` the folder it starts in, `show` its window, `elevated` a start as administrator (`*`
-- in Total Commander). `tooltip` and the symbol (`icon_path`, `icon_index`) are what the Start
-- menu, the palette and a later button bar show.
--
-- `start_menu_items` is the Start menu as a tree: `kind` an entry (`item`), a submenu
-- (`submenu`, the parent of the rows under it) or a separator, `position` its place among its
-- siblings, and an entry's command in the same fields as an own command (`command_kind` for
-- `kind`). A submenu removed takes its rows with it.
--
-- Both are made and removed by a person and kept as they are (`rc_db::DURABLE_TABLES`); the
-- limits (512 rows each, 8 levels, 4096 characters a field) are checked by `rc_db` before it
-- writes, the CHECKs below are the last line.
CREATE TABLE user_commands (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE COLLATE NOCASE
               CHECK (length(name) BETWEEN 4 AND 43 AND substr(name, 1, 3) = 'em_'),
    kind       TEXT NOT NULL CHECK (kind IN ('program', 'internal', 'own')),
    command    TEXT NOT NULL CHECK (length(command) <= 4096),
    params     TEXT NOT NULL DEFAULT '' CHECK (length(params) <= 4096),
    folder     TEXT NOT NULL DEFAULT '' CHECK (length(folder) <= 4096),
    show       TEXT NOT NULL DEFAULT 'normal'
               CHECK (show IN ('normal', 'minimized', 'maximized')),
    elevated   INTEGER NOT NULL DEFAULT 0 CHECK (elevated IN (0, 1)),
    tooltip    TEXT NOT NULL DEFAULT '' CHECK (length(tooltip) <= 4096),
    icon_path  TEXT NOT NULL DEFAULT '' CHECK (length(icon_path) <= 4096),
    icon_index INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

CREATE TABLE start_menu_items (
    id           INTEGER PRIMARY KEY,
    parent_id    INTEGER REFERENCES start_menu_items (id) ON DELETE CASCADE,
    position     INTEGER NOT NULL DEFAULT 0,
    kind         TEXT NOT NULL CHECK (kind IN ('item', 'submenu', 'separator')),
    title        TEXT NOT NULL DEFAULT '' CHECK (length(title) <= 4096),
    command_kind TEXT NOT NULL DEFAULT 'program'
                 CHECK (command_kind IN ('program', 'internal', 'own')),
    command      TEXT NOT NULL DEFAULT '' CHECK (length(command) <= 4096),
    params       TEXT NOT NULL DEFAULT '' CHECK (length(params) <= 4096),
    folder       TEXT NOT NULL DEFAULT '' CHECK (length(folder) <= 4096),
    show         TEXT NOT NULL DEFAULT 'normal'
                 CHECK (show IN ('normal', 'minimized', 'maximized')),
    elevated     INTEGER NOT NULL DEFAULT 0 CHECK (elevated IN (0, 1)),
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

-- The rows of one submenu (or of the top) in their order.
CREATE INDEX start_menu_items_parent ON start_menu_items (parent_id, position);
