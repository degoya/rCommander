-- Column sets (RC-100-90): the person's own columns of the file list, as Total Commander's custom
-- columns view. `spec` is the engine's versioned JSON (`rc_engine::columns::ColumnSetSpec`: the
-- columns with field, title, width and alignment, and the folder masks of the automatic switch),
-- checked by the engine on the way in and on the way back, at most 16 KiB. `position` is the
-- person's order in the list, which also decides which set's mask wins. The sets are made and
-- removed by a person and kept as they are (`rc_db::DURABLE_TABLES`).
CREATE TABLE column_sets (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE COLLATE NOCASE
                   CHECK (length(trim(name)) > 0 AND length(name) <= 64),
    position   INTEGER NOT NULL DEFAULT 0,
    spec       TEXT NOT NULL CHECK (json_valid(spec) AND length(spec) <= 16384),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;
