-- The button bars (RC-100-87), as Total Commander keeps them in its `.bar` files.
--
-- One row is one bar: its `name` (unique without regard to case) and its buttons as versioned
-- JSON in `spec` (`rc_engine::button_bars::BarSpec`): each a button - a command (a program, the
-- keymap's rows or an own command `em_`, as the own commands keep it) or a sub-bar it opens, by
-- the sub-bar's id - with its symbol and tooltip, or a separator. The settings
-- `buttonbar.horizontal` and `buttonbar.vertical` name the bar each place shows.
--
-- The bars are made and removed by a person and kept as they are (`rc_db::DURABLE_TABLES`); the
-- limits (64 bars, 128 buttons a bar, sub-bars 4 deep, no cycle) are checked by `rc_engine` and
-- `rc_db` before they write, the CHECKs below are the last line.
--
-- The first start shows the bar "Default" (owner 2026-11-07): it is made here, once.
CREATE TABLE button_bars (
    id         INTEGER PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE COLLATE NOCASE
               CHECK (length(name) BETWEEN 1 AND 64),
    spec       TEXT NOT NULL CHECK (length(spec) <= 65536),
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
) STRICT;

INSERT INTO button_bars (name, spec) VALUES (
    'Default',
    '{"version":1,"items":['
    || '{"type":"button","action":{"command":{"kind":"internal","command":"panel.refresh"}}},'
    || '{"type":"separator"},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"view_brief"}}},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"view_full"}}},'
    || '{"type":"separator"},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"panel.invert_selection"}}},'
    || '{"type":"separator"},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"archive.pack"}}},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"archive.extract"}}},'
    || '{"type":"separator"},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"files.find_dialog"}}},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"panel.toggle_quick_view"}}},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"multi_rename.open_dialog"}}},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"sync.open_dialog"}}},'
    || '{"type":"separator"},'
    || '{"type":"button","action":{"command":{"kind":"internal","command":"panel.toggle_terminal"}}}'
    || ']}'
);
