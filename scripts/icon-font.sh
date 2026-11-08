#!/usr/bin/env bash
#
# Writes crates/rcommander/resources/icons/rcommander-icons.ttf, the icons on the dialogs'
# buttons (RC-080-07) and on the button bar's buttons of rCommander's commands (RC-100-87): the
# glyphs below from Fluent UI System Icons (MIT), at the pinned upstream commit, subset by
# scripts/lib/icon_font.py. The font is committed; run this only to
# add or change a glyph, then give `theme::icon::Icon` the code point it prints - the tests in
# theme/icon_tests.rs hold the enum and the font to each other.
#
# `--symbols` writes the symbol collection of own commands and bar buttons instead (RC-110-10):
# crates/rcommander/resources/icons/rcommander-symbols.ttf from the list scripts/lib/icon_symbols.txt
# (category, name, keywords), and the table the code reads it by,
# crates/rc-engine/src/user_commands/symbols.tsv (scripts/lib/icon_symbols.py). `--check` holds
# that list, the table and the font's size (at most 400 KiB) together - no network, no fontTools;
# check.sh --preflight runs it, and the Rust tests hold the table and the font's glyphs together.
#
# Generated, never kept by hand. It downloads the upstream font and its code-point list (the
# font is checked against its SHA-256) and needs Python's fontTools (`pip install fonttools`).
#
# Usage:
#   scripts/icon-font.sh [--symbols|--check]
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMMIT=a347af95478f989038cccbf085978865264c048a
SHA256=f977ae5bfb46661e7ba94b098e0d7720e27af888e57b9b14e239933ac4c0e6f4
BASE="https://raw.githubusercontent.com/microsoft/fluentui-system-icons/$COMMIT/fonts"
TARGET="$ROOT/crates/rcommander/resources/icons/rcommander-icons.ttf"
SYMBOL_LIST="$ROOT/scripts/lib/icon_symbols.txt"
SYMBOL_FONT="$ROOT/crates/rcommander/resources/icons/rcommander-symbols.ttf"
SYMBOL_TABLE="$ROOT/crates/rc-engine/src/user_commands/symbols.tsv"
# The glyphs, one per action of the job file, in the order of `Icon`.
GLYPHS=(
    add_20_regular               # New
    checkmark_20_regular         # OK, Yes, Allow
    copy_20_regular              # Copy
    delete_20_regular            # Delete, Remove
    dismiss_20_regular           # Close, Cancel
    folder_add_20_regular        # New folder
    folder_open_20_regular       # Browse, Open
    arrow_import_20_regular      # Import
    list_20_regular              # Queue
    arrow_move_20_regular        # Move
    pause_20_regular             # Pause
    plug_connected_20_regular    # Connect
    plug_disconnected_20_regular # Disconnect
    play_20_regular              # Resume, Run
    rename_20_regular            # Rename
    save_20_regular              # Save
    search_20_regular            # Search
    stop_20_regular              # Stop
    window_console_20_regular    # Terminal
    beaker_20_regular            # Test
    arrow_up_20_regular          # Move up
    arrow_down_20_regular        # Move down
    text_indent_increase_ltr_20_regular # Indent
    text_indent_decrease_ltr_20_regular # Outdent
    star_20_regular              # Star: the drive bar's mark
    # The button bar's internal commands (RC-100-87).
    arrow_clockwise_20_regular   # Reread
    text_column_three_20_regular # Brief view
    text_bullet_list_ltr_20_regular # Full view
    select_all_off_20_regular    # Invert selection
    archive_20_regular           # Pack
    archive_arrow_back_20_regular # Unpack
    document_search_20_regular   # Find files
    eye_20_regular               # Quick View
    folder_sync_20_regular       # Synchronize folders
    document_text_20_regular     # View a file
    document_edit_20_regular     # Edit a file
    branch_compare_20_regular    # Compare by content
    arrow_swap_20_regular        # Swap the panels
    folder_arrow_up_20_regular   # Parent folder
    history_20_regular           # Folder history
    power_20_regular             # Quit
    checkbox_checked_20_regular  # Select all
)

mode="${1:-}"
case "$#:$mode" in
    0: | 1:--symbols) ;;
    1:--check)
        exec python3 -B "$ROOT/scripts/lib/icon_symbols.py" --check "$SYMBOL_LIST" \
            "$SYMBOL_TABLE" "$SYMBOL_FONT"
        ;;
    *)
        echo "usage: scripts/icon-font.sh [--symbols|--check]" >&2
        exit 2
        ;;
esac
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
curl -sSfL -o "$scratch/source.ttf" "$BASE/FluentSystemIcons-Regular.ttf"
curl -sSfL -o "$scratch/source.json" "$BASE/FluentSystemIcons-Regular.json"
echo "$SHA256  $scratch/source.ttf" | sha256sum --check --quiet
# Not exec: the trap removes the scratch directory afterwards.
if [[ "$mode" == --symbols ]]; then
    python3 -B "$ROOT/scripts/lib/icon_symbols.py" --write "$scratch/source.ttf" \
        "$scratch/source.json" "$SYMBOL_LIST" "$SYMBOL_FONT" "$SYMBOL_TABLE"
    exit 0
fi
python3 -B "$ROOT/scripts/lib/icon_font.py" "$scratch/source.ttf" "$scratch/source.json" \
    "$TARGET" "${GLYPHS[@]}"
ls -la "$TARGET"
