"""The buttons' icon font, crates/rcommander/resources/icons/rcommander-icons.ttf (RC-080-07).

A subset of Fluent UI System Icons' regular font (MIT): only the glyphs the buttons draw, at
their upstream code points, without the ligature table, under the family name "rCommander Icons"
so an installed copy of the full font never stands in for it. The timestamp of the source is
kept, so the same source and the same glyphs give the same bytes.

    python3 scripts/lib/icon_font.py <FluentSystemIcons-Regular.ttf> <its .json> <out.ttf> <name>...

A name is the glyph's upstream name without its prefix, `save_20_regular`. Needs fontTools.
Exit 0 when written, 2 when a name is not in the source or an input does not read.
"""

import json
import sys

from fontTools import subset
from fontTools.ttLib import TTFont

FAMILY = "rCommander Icons"
POSTSCRIPT = "rCommanderIcons-Regular"
COPYRIGHT = (
    "Copyright (c) 2020 Microsoft Corporation. Fluent UI System Icons, MIT License; "
    "subset for rCommander"
)


def main(argv: list[str]) -> int:
    if len(argv) < 5:
        print(__doc__, file=sys.stderr)
        return 2
    source, mapping, target, names = argv[1], argv[2], argv[3], argv[4:]
    try:
        with open(mapping, encoding="utf-8") as file:
            points = json.load(file)
        font = TTFont(source, recalcTimestamp=False)
    except (OSError, ValueError) as error:
        print(f"!! {error}", file=sys.stderr)
        return 2
    missing = [name for name in names if f"ic_fluent_{name}" not in points]
    if missing:
        print(f"!! not in the source: {', '.join(missing)}", file=sys.stderr)
        return 2
    unicodes = [points[f"ic_fluent_{name}"] for name in names]
    options = subset.Options()
    options.layout_features = []
    options.name_IDs = []
    options.notdef_outline = False
    options.recalc_timestamp = False
    subsetter = subset.Subsetter(options)
    subsetter.populate(unicodes=unicodes)
    subsetter.subset(font)
    table = font["name"]
    for name_id, value in (
        (0, COPYRIGHT),
        (1, FAMILY),
        (2, "Regular"),
        (3, POSTSCRIPT),
        (4, FAMILY),
        (5, "Version 1.0"),
        (6, POSTSCRIPT),
    ):
        table.setName(value, name_id, 3, 1, 0x409)
    font.save(target)
    for name, point in zip(names, unicodes):
        print(f"U+{point:04X} {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
