#!/usr/bin/env python3
"""The line and function coverage of each crate, from cargo-llvm-cov's JSON summary.

    coverage.py <summary.json> <root>     # the table on stdout

<summary.json> is `cargo llvm-cov report --json --summary-only` (llvm-cov's export format);
<root> the checkout its file names start with. A file below `crates/<name>/` or
`plugins/<name>/` counts for that crate; anything else (the registry, the toolchain) is left
out. Prints a Markdown table, a row per crate in name order and the total last, for
docs/development.md ("Fuzzing and coverage"). Exit 0, or 2 for a file that cannot be read or is
no llvm-cov export.
"""

import json
import os
import sys


def crate_of(filename, root):
    """`crates/<name>` or `plugins/<name>` for a file of the checkout, else None."""
    relative = os.path.relpath(filename, root).replace(os.sep, "/")
    parts = relative.split("/")
    if len(parts) >= 3 and parts[0] in ("crates", "plugins"):
        return parts[0] + "/" + parts[1]
    return None


def percent(covered, count):
    return "-" if count == 0 else f"{100.0 * covered / count:.1f} %"


def table(export, root):
    totals = {}
    for data in export["data"]:
        for item in data["files"]:
            crate = crate_of(item["filename"], root)
            if crate is None:
                continue
            row = totals.setdefault(crate, [0, 0, 0, 0])
            summary = item["summary"]
            row[0] += summary["lines"]["covered"]
            row[1] += summary["lines"]["count"]
            row[2] += summary["functions"]["covered"]
            row[3] += summary["functions"]["count"]
    lines = [
        "| Crate | Lines | Line coverage | Functions | Function coverage |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    whole = [0, 0, 0, 0]
    for crate in sorted(totals):
        row = totals[crate]
        whole = [a + b for a, b in zip(whole, row)]
        lines.append(
            f"| `{crate.split('/')[1]}` | {row[1]} | {percent(row[0], row[1])} "
            f"| {row[3]} | {percent(row[2], row[3])} |"
        )
    lines.append(
        f"| **all** | {whole[1]} | {percent(whole[0], whole[1])} "
        f"| {whole[3]} | {percent(whole[2], whole[3])} |"
    )
    return "\n".join(lines)


def main(argv):
    if len(argv) != 3:
        print("usage: coverage.py <summary.json> <root>", file=sys.stderr)
        return 2
    try:
        with open(argv[1], encoding="utf-8") as handle:
            export = json.load(handle)
        if not str(export.get("type", "")).startswith("llvm.coverage.json.export"):
            raise ValueError("not an llvm-cov export")
        print(table(export, os.path.abspath(argv[2])))
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"!! {argv[1]}: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
