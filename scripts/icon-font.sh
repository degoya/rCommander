#!/usr/bin/env bash
#
# Writes crates/rcommander/resources/icons/rcommander-icons.ttf, the icons on the dialogs'
# buttons (RC-080-07): the glyphs below from Fluent UI System Icons (MIT), at the pinned
# upstream commit, subset by scripts/lib/icon_font.py. The font is committed; run this only to
# add or change a glyph, then give `theme::icon::Icon` the code point it prints - the tests in
# theme/icon_tests.rs hold the enum and the font to each other.
#
# Generated, never kept by hand. It downloads the upstream font and its code-point list (the
# font is checked against its SHA-256) and needs Python's fontTools (`pip install fonttools`).
#
# Usage:
#   scripts/icon-font.sh
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COMMIT=a347af95478f989038cccbf085978865264c048a
SHA256=f977ae5bfb46661e7ba94b098e0d7720e27af888e57b9b14e239933ac4c0e6f4
BASE="https://raw.githubusercontent.com/microsoft/fluentui-system-icons/$COMMIT/fonts"
TARGET="$ROOT/crates/rcommander/resources/icons/rcommander-icons.ttf"
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
)

if [[ $# -gt 0 ]]; then
    echo "usage: scripts/icon-font.sh" >&2
    exit 2
fi
scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT
curl -sSfL -o "$scratch/source.ttf" "$BASE/FluentSystemIcons-Regular.ttf"
curl -sSfL -o "$scratch/source.json" "$BASE/FluentSystemIcons-Regular.json"
echo "$SHA256  $scratch/source.ttf" | sha256sum --check --quiet
# Not exec: the trap removes the scratch directory afterwards.
python3 -B "$ROOT/scripts/lib/icon_font.py" "$scratch/source.ttf" "$scratch/source.json" \
    "$TARGET" "${GLYPHS[@]}"
ls -la "$TARGET"
