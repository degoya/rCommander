#!/usr/bin/env python3
"""Refuses a private name in what goes public, outside the allowlist (RC-100-30).

    public-names.py <tree> --terms <terms> --allow <allowlist> [--exclude <list>] [--wiki] [--unused]

A private name is a private host, a home directory, a private repository or a private address:
whatever scripts/public-names.txt lists, one regular expression per line, matched without regard
to case. That list names the very things it guards, so the export leaves it out; this guard and
its allowlist name none of them and are public.

<tree> is a git checkout (its tracked and new, not ignored files are read) or a plain directory, such as an export's
staging tree (every file is read). Binary files are skipped. --exclude names
scripts/public-exclude.txt: a path it leaves out of the public repository is not read.

What may still match - a placeholder user in a WSL path - is listed in the allowlist
(scripts/public-names-allow.txt), one entry per line:

    <path or glob> | <regular expression> | <reason>

A line passes when no private name is left in it once every entry whose glob matches its file
has cut out what its expression finds.
An entry whose glob starts with `wiki/` applies to the wiki (--wiki: every page is read as
`wiki/<page>`), any other to the repository. With --unused an entry of the mode checked that no
line needs any more is a finding too, so the list cannot outgrow the tree.

Prints one line per finding and exits 1 when there is any, 0 otherwise; 2 on a usage error.
"""

import argparse
import fnmatch
import os
import re
import subprocess
import sys

WIKI = "wiki/"


def read_list(path):
    """[(line number, text)] of the non-empty, non-comment lines of a list file."""
    with open(path, encoding="utf-8") as handle:
        return [
            (number, line.strip())
            for number, line in enumerate(handle, 1)
            if line.strip() and not line.strip().startswith("#")
        ]


def excluded_paths(listing):
    """The entries of the exclude list, the way export-public.sh reads them."""
    entries = []
    with open(listing, encoding="utf-8") as handle:
        for line in handle:
            path = line.split("#", 1)[0].strip().rstrip("/")
            if path:
                entries.append(path)
    return entries


def is_excluded(path, entries):
    return any(path == entry or path.startswith(entry + "/") for entry in entries)


def terms(listing, findings):
    compiled = []
    for number, text in read_list(listing):
        try:
            compiled.append(re.compile(text, re.IGNORECASE))
        except re.error as error:
            findings.append(f"{listing}:{number}: bad expression: {error}")
    return compiled


def allow_entries(listing, findings):
    """[(line number, path glob, compiled expression, reason)]."""
    entries = []
    for number, text in read_list(listing):
        parts = [part.strip() for part in text.split(" | ")]
        if len(parts) != 3 or not all(parts):
            findings.append(f"{listing}:{number}: not '<path> | <expression> | <reason>'")
            continue
        try:
            entries.append((number, parts[0], re.compile(parts[1]), parts[2]))
        except re.error as error:
            findings.append(f"{listing}:{number}: bad expression: {error}")
    return entries


def files(tree):
    """The paths below `tree`, relative and with `/`; of a checkout the tracked and the new ones
    git does not ignore."""
    if os.path.exists(os.path.join(tree, ".git")):
        result = subprocess.run(
            ["git", "-C", tree, "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
            capture_output=True,
            check=False,
        )
        if result.returncode != 0:
            sys.exit(f"git ls-files failed: {result.stderr.decode(errors='replace').strip()}")
        return sorted(p for p in result.stdout.decode().split("\0") if p)
    found = []
    for directory, folders, names in os.walk(tree):
        folders[:] = sorted(f for f in folders if f != ".git")
        for name in sorted(names):
            full = os.path.join(directory, name)
            found.append(os.path.relpath(full, tree).replace(os.sep, "/"))
    return found


def text_lines(full):
    try:
        with open(full, "rb") as handle:
            data = handle.read()
    except OSError:
        return []
    if b"\0" in data[:8192]:
        return []
    return data.decode("utf-8", errors="replace").split("\n")


def main(argv):
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("tree")
    parser.add_argument("--terms", required=True)
    parser.add_argument("--allow", required=True)
    parser.add_argument("--exclude")
    parser.add_argument("--wiki", action="store_true")
    parser.add_argument("--unused", action="store_true")
    try:
        args = parser.parse_args(argv[1:])
    except SystemExit:
        print(__doc__.strip().split("\n")[2].strip(), file=sys.stderr)
        return 2
    findings = []
    names = terms(args.terms, findings)
    entries = [
        entry
        for entry in allow_entries(args.allow, findings)
        if entry[1].startswith(WIKI) == args.wiki
    ]
    excluded = excluded_paths(args.exclude) if args.exclude else []
    used = set()
    for path in files(args.tree):
        if is_excluded(path, excluded):
            continue
        shown = WIKI + path if args.wiki else path
        for number, line in enumerate(text_lines(os.path.join(args.tree, path)), 1):
            if not any(name.search(line) for name in names):
                continue
            # What an entry allows is cut from the line; a private name left over still counts,
            # so an allowed placeholder cannot carry a real name beside it.
            rest = line
            for line_number, glob, expression, _ in entries:
                if fnmatch.fnmatchcase(shown, glob) and expression.search(rest):
                    used.add(line_number)
                    rest = expression.sub(" ", rest)
            if any(name.search(rest) for name in names):
                findings.append(f"{shown}:{number}: names something private: {line.strip()[:160]}")
    if args.unused:
        for line_number, glob, _, reason in entries:
            if line_number not in used:
                findings.append(
                    f"{args.allow}:{line_number}: no line needs this entry any more ({glob}: {reason})"
                )
    for finding in findings:
        print(finding)
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
