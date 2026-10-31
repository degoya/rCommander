-- Own colour themes (RC-100-50): a set of colours a person made, duplicated from a built-in theme
-- or imported from a `.rctheme` file. The built-in themes are code, never rows. `id` is the
-- engine's (`own-<hex>`) and stays when the theme is renamed or edited, so the settings
-- `appearance.theme_light` and `appearance.theme_dark` keep naming it; `dark` is the mode the
-- theme is for; `accent` is empty for the system's accent or `#rrggbb`; `tokens` is a JSON object
-- `token -> "#rrggbb"`, a token it lacks taking the fixed set of its mode. A person makes and
-- removes them (`rc_db::DURABLE_TABLES`).
CREATE TABLE themes (
    id       TEXT PRIMARY KEY NOT NULL CHECK (length(id) BETWEEN 1 AND 64),
    name     TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(trim(name)) > 0),
    dark     INTEGER NOT NULL CHECK (dark IN (0, 1)),
    accent   TEXT NOT NULL DEFAULT '' CHECK (accent = '' OR length(accent) = 7),
    tokens   TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(tokens)),
    saved_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
