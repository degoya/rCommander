"""The deb's and the rpm's /usr/share/doc/rcommander/copyright, machine-readable (DEP-5), from the
package's THIRD-PARTY-LICENSES (scripts/lib/license_texts.py) - one source for both (RC-110-72).

One `Files` paragraph for rCommander itself and one per licence text: the crates it covers as
`<crate>-<version>/*`, the path of their source in cargo's registry; the copyright lines the text
holds; the licence by a short name the text's own words give, with the full text.

    python3 scripts/lib/license_dep5.py <THIRD-PARTY-LICENSES> <copyright to write>

Exit 0 when written; 2 when the input does not read or holds no licence text.
"""

import re
import sys
from pathlib import Path

from license_texts import parse

FORMAT = "https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/"
COPYRIGHT_LINE = re.compile(r"^\s*(?:Copyright\b|\(c\)\s|©)")
# Placeholders of a licence's template, never a holder.
PLACEHOLDER = re.compile(r"\[yyyy\]|\[name of copyright owner\]|<year>|<copyright holders?>|\{yyyy\}", re.IGNORECASE)
# The short name of a text, by words only that licence uses; the first that matches wins.
KINDS = [
    ("Apache-2.0 WITH LLVM-exception", ("Apache License", "LLVM Exceptions")),
    ("Apache-2.0", ("Apache License", "Version 2.0")),
    ("MPL-2.0", ("Mozilla Public License Version 2.0",)),
    ("GPL-2.0", ("GNU GENERAL PUBLIC LICENSE", "Version 2")),
    ("BSL-1.0", ("Boost Software License",)),
    ("CC0-1.0", ("CC0 1.0 Universal",)),
    ("Unicode-3.0", ("UNICODE LICENSE V3",)),
    ("Unlicense", ("This is free and unencumbered software",)),
    ("MIT", ("Permission is hereby granted, free of charge",)),
    ("BSD-3-Clause", ("Redistribution and use in source and binary forms", "Neither the name")),
    ("BSD-2-Clause", ("Redistribution and use in source and binary forms",)),
    ("ISC", ("Permission to use, copy, modify, and/or distribute",)),
    ("Zlib", ("This software is provided 'as-is'",)),
]


def kind(text: str, fallback: str) -> str:
    """A short name for the licence a text is."""
    for name, words in KINDS:
        if all(word in text for word in words):
            return name
    return fallback


def holders(text: str) -> list[str]:
    """The copyright lines of a text, each once, in order."""
    found: list[str] = []
    for line in text.split("\n"):
        line = line.strip()
        if COPYRIGHT_LINE.match(line) and not PLACEHOLDER.search(line) and line not in found:
            found.append(line)
    return found


def field(text: str) -> str:
    """A multi-line value: each line indented by one blank, an empty one as ` .`."""
    return "\n".join(f" {line}" if line.strip() else " ." for line in text.split("\n"))


def render(blocks: list[tuple[list[str], str]]) -> str:
    paragraphs = [
        f"Format: {FORMAT}\n"
        "Upstream-Name: rCommander\n"
        "Source: https://github.com/degoya/rCommander\n"
        "Comment: The Rust crates rCommander and its bundled plugins contain, each named as its\n"
        " source in cargo's registry (<crate>-<version>/*), with the licence texts their archives\n"
        " carry. /usr/share/doc/rcommander/THIRD-PARTY-LICENSES has the same texts with the licence\n"
        " each crate declares; /usr/share/doc/rcommander/NOTICE names the further third-party\n"
        " material (icons, colour themes, fonts). A crate offered under a choice of licences is\n"
        " named in the paragraph of each text its archive carries.",
        "Files: *\n"
        "Copyright: Alexander Herling / DEGOYA.digital\n"
        "License: GPL-3.0-or-later\n"
        " The full text is /usr/share/doc/rcommander/LICENSE.",
    ]
    for number, (headers, text) in enumerate(blocks, start=1):
        crates = sorted({"-".join(header.split()[:2]) + "/*" for header in headers})
        copyright_lines = holders(text) or ["the authors of the crates named in Files"]
        paragraphs.append(
            "Files: " + "\n ".join(crates) + "\n"
            "Copyright: " + "\n ".join(copyright_lines) + "\n"
            f"License: {kind(text, f'LicenseRef-text-{number}')}\n" + field(text)
        )
    return "\n\n".join(paragraphs) + "\n"


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print("usage: license_dep5.py <THIRD-PARTY-LICENSES> <copyright to write>", file=sys.stderr)
        return 2
    try:
        blocks = parse(Path(argv[1]).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if not blocks:
        print(f"error: {argv[1]} holds no licence text", file=sys.stderr)
        return 2
    Path(argv[2]).write_text(render(blocks), encoding="utf-8")
    print(f"==> wrote {argv[2]}: {len(blocks)} licence texts")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
