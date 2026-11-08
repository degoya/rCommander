"""The buttons' icon font, crates/rcommander/resources/icons/rcommander-icons.ttf (RC-080-07).

A subset of Fluent UI System Icons' regular font (MIT): only the glyphs the buttons draw, at
their upstream code points, without the ligature table, under the family name "rCommander Icons"
so an installed copy of the full font never stands in for it. The timestamp of the source is
kept, so the same source and the same glyphs give the same bytes.

    python3 scripts/lib/icon_font.py <FluentSystemIcons-Regular.ttf> <its .json> <out.ttf> <name>...

A name is the glyph's upstream name without its prefix, `save_20_regular`. Needs fontTools.
Exit 0 when written, 2 when a name is not in the source or an input does not read. The symbol
collection's font (RC-110-10) is icon_symbols.py's, which subsets with `subset_to` here.
"""

import json
import sys

FAMILY = "rCommander Icons"
POSTSCRIPT = "rCommanderIcons-Regular"
COPYRIGHT = (
    "Copyright (c) 2020 Microsoft Corporation. Fluent UI System Icons, MIT License; "
    "subset for rCommander"
)


def subset_to(source: str, unicodes: list[int], target: str, family: str, postscript: str) -> None:
    """Writes the glyphs of `unicodes` from the font `source` to `target`, named `family`."""
    from fontTools import subset
    from fontTools.ttLib import TTFont

    font = TTFont(source, recalcTimestamp=False)
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
        (1, family),
        (2, "Regular"),
        (3, postscript),
        (4, family),
        (5, "Version 1.0"),
        (6, postscript),
    ):
        table.setName(value, name_id, 3, 1, 0x409)
    font.save(target)


def main(argv: list[str]) -> int:
    if len(argv) < 5:
        print(__doc__, file=sys.stderr)
        return 2
    source, mapping, target, names = argv[1], argv[2], argv[3], argv[4:]
    try:
        with open(mapping, encoding="utf-8") as file:
            points = json.load(file)
    except (OSError, ValueError) as error:
        print(f"!! {error}", file=sys.stderr)
        return 2
    missing = [name for name in names if f"ic_fluent_{name}" not in points]
    if missing:
        print(f"!! not in the source: {', '.join(missing)}", file=sys.stderr)
        return 2
    unicodes = [points[f"ic_fluent_{name}"] for name in names]
    try:
        subset_to(source, unicodes, target, FAMILY, POSTSCRIPT)
    except (OSError, ValueError) as error:
        print(f"!! {error}", file=sys.stderr)
        return 2
    for name, point in zip(names, unicodes):
        print(f"U+{point:04X} {name}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
