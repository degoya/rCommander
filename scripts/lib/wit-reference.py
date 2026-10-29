#!/usr/bin/env python3
"""Generates the plugin contract reference from the WIT (RC-100-12).

    wit-reference.py <repo> [--print] [--check] [--wiki DIR]

Copied and adapted from rDownloader's `scripts/lib/wit-reference.py` at commit 26c8592a6
(RD-160-04). The contract, crates/rc-plugin-api/wit/rcommander.wit, is the one source of what a
plugin can call and must export. The wiki's page for plugin authors
(`develop/plugin-development.md`) carries a generated part between two markers, and this script
writes it:

    <!-- BEGIN wit-reference -->
    ...
    <!-- END wit-reference -->

Every world with its imports and exports; every interface with its type aliases, functions,
records, variants, enums and flags, each with the comment the WIT gives it. Nothing in the
generated part is edited by hand: a contract change is followed by a run of this script, in the
same change.

  --print        the Markdown to stdout
  --wiki DIR     write it into DIR/develop/plugin-development.md between the markers
  --wiki DIR --check
                 write nothing; exit 1 when the page's generated part differs from the WIT
  --check        (without --wiki) read the contract strictly and write nothing: exit 0 when
                 every statement is understood. The preflight runs this, since the wiki is a
                 repository of its own; a construct the reader does not know (a `resource`, an
                 inline interface in a world) is exit 2, so the reference cannot silently leave
                 part of the contract out.

Exit 2 also when the page has no markers, or they are out of order: the run refuses rather than
guess where the part belongs.

Adapted: rCommander's WIT writes its comments with `//` after the code on the same line, and
records and enums on one line, so the reader works on statements rather than lines. A comment
belongs to the statement on its line, or - on lines of its own directly above - to the statement
below it; the comments above `package` are the file's preamble and not part of the reference.
The wiki names no job ids and no ADRs: a parenthesis holding only those, `(RC-100-08)` or
`(RC-100-08, ADR 0003)`, is left out of the page, and a job id anywhere else is exit 2.
"""

import bisect
import os
import re
import sys

WIT = "crates/rc-plugin-api/wit/rcommander.wit"
PAGE = "develop/plugin-development.md"
BEGIN = "<!-- BEGIN wit-reference -->"
END = "<!-- END wit-reference -->"

TRACE_ID = r"(?:RC-\d{3}-\d{2}|ADR \d{4})"
TRACE = re.compile(rf"[ \t]*\({TRACE_ID}(?:,\s*{TRACE_ID})*\)")
JOB_ID = re.compile(r"\bRC-\d{3}-\d{2}\b")
NAME = r"%?[a-z][a-z0-9-]*"


class Refusal(Exception):
    """A contract or a page this script will not guess about."""


# --- reading ------------------------------------------------------------------------------


class Source:
    """The WIT with its comments blanked out, the comments kept per line."""

    def __init__(self, text):
        lines, self.comments = [], {}
        for number, line in enumerate(text.split("\n"), 1):
            at = line.find("//")
            if at >= 0:
                comment = line[at + 2 :].lstrip("/").strip()
                self.comments[number] = (comment, line[:at].strip() == "")
                line = line[:at] + " " * (len(line) - at)
            lines.append(line)
        self.code = "\n".join(lines)
        self.starts = [0]
        for line in lines:
            self.starts.append(self.starts[-1] + len(line) + 1)

    def line_of(self, position):
        return bisect.bisect_right(self.starts, position)

    def docs(self, first, last):
        """The comment lines directly above line `first`, then the comments after code on the
        lines `first` to `last`."""
        above, number = [], first - 1
        while number in self.comments and self.comments[number][1]:
            above.insert(0, self.comments[number][0])
            number -= 1
        after = [
            self.comments[line][0]
            for line in range(first, last + 1)
            if line in self.comments and not self.comments[line][1]
        ]
        return [line for line in above + after if line]


def statements(source, start, end):
    """The statements of `source.code[start:end]`: (text, first line, last line, body), where
    `body` is the (start, end) of the braces a statement opens, else None."""
    code, position = source.code, start
    while True:
        while position < end and code[position].isspace():
            position += 1
        if position >= end:
            return
        first = position
        while position < end and code[position] not in ";{}":
            position += 1
        if position >= end or code[position] == "}":
            raise Refusal(f"{WIT}:{source.line_of(first)}: a statement without its end: "
                          f"{code[first:position].strip()}")
        head = " ".join(code[first:position].split())
        if code[position] == "{" and head.startswith("use "):
            # `use types.{a, b};`: braces inside a statement, which ends at its `;`.
            closing = code.find("}", position, end)
            stop = code.find(";", max(closing, position), end)
            if closing < 0 or stop < 0:
                raise Refusal(f"{WIT}:{source.line_of(first)}: `{head}` is never closed")
            head = " ".join(code[first:stop].split())
            yield head, source.line_of(first), source.line_of(stop), None
            position = stop + 1
            continue
        if code[position] == ";":
            yield head, source.line_of(first), source.line_of(position), None
            position += 1
            continue
        opened, depth = position + 1, 1
        position += 1
        while position < end and depth:
            depth += {"{": 1, "}": -1}.get(code[position], 0)
            position += 1
        if depth:
            raise Refusal(f"{WIT}:{source.line_of(first)}: `{head}` is never closed")
        yield head, source.line_of(first), source.line_of(position - 1), (opened, position - 1)


def split_members(text):
    """The comma-separated members of a type's braces, commas inside `<>` and `()` kept."""
    members, depth, current = [], 0, ""
    for character in text:
        if character in "<(":
            depth += 1
        elif character in ">)":
            depth -= 1
        if character == "," and depth == 0:
            members.append(current.strip())
            current = ""
        else:
            current += character
    if current.strip():
        members.append(current.strip())
    return [" ".join(member.split()) for member in members]


MEMBER = {
    "record": rf"({NAME}): (.+)",
    "variant": rf"({NAME})(?:\((.+)\))?",
    "enum": rf"({NAME})()",
    "flags": rf"({NAME})()",
}


def parse(text):
    """The contract as plain data: package, interfaces and worlds, in file order."""
    source = Source(text)
    contract = {"package": None, "interfaces": [], "worlds": []}
    for head, first, last, body in statements(source, 0, len(source.code)):
        match = re.fullmatch(r"package ([a-z0-9:-]+@[0-9.]+)", head)
        if match and body is None:
            contract["package"] = match.group(1)
            continue
        match = re.fullmatch(r"(interface|world) ([a-z0-9-]+)", head)
        if not match or body is None:
            raise Refusal(f"{WIT}:{first}: not understood at the top level: {head}")
        kind, name = match.groups()
        item = {"name": name, "docs": source.docs(first, first)}
        if kind == "interface":
            item.update({"uses": [], "types": [], "functions": []})
            parse_interface(source, body, item)
            contract["interfaces"].append(item)
        else:
            item.update({"imports": [], "exports": []})
            parse_world(source, body, item)
            contract["worlds"].append(item)
    if contract["package"] is None:
        raise Refusal(f"{WIT}: no package statement")
    return contract


def parse_interface(source, body, interface):
    for head, first, last, inner in statements(source, *body):
        docs = source.docs(first, last)
        if inner is not None:
            match = re.fullmatch(rf"(record|variant|enum|flags) ({NAME})", head)
            if not match:
                raise Refusal(f"{WIT}:{first}: not understood in interface "
                              f"`{interface['name']}`: {head}")
            kind, name = match.groups()
            members = []
            for member in split_members(source.code[inner[0] : inner[1]]):
                found = re.fullmatch(MEMBER[kind], member)
                if not found:
                    raise Refusal(f"{WIT}:{first}: not understood in {kind} `{name}`: {member}")
                members.append({"name": found.group(1), "type": found.group(2) or None})
            interface["types"].append(
                {"kind": kind, "name": name, "docs": docs, "members": members})
            continue
        match = re.fullmatch(r"use ([a-z0-9-]+)\.\{([a-z0-9%, -]+)\}", head)
        if match:
            names = [name.strip() for name in match.group(2).split(",")]
            interface["uses"].append((match.group(1), names))
            continue
        match = re.fullmatch(rf"type ({NAME}) = (.+)", head)
        if match:
            interface["types"].append({"kind": "type", "name": match.group(1),
                                       "docs": docs, "alias": match.group(2)})
            continue
        match = re.fullmatch(rf"({NAME}): func\((.*)\)(?: -> (.+))?", head)
        if match:
            name, params, result = match.groups()
            interface["functions"].append(
                {"name": name, "params": params, "result": result, "docs": docs})
            continue
        raise Refusal(f"{WIT}:{first}: not understood in interface "
                      f"`{interface['name']}`: {head}")


def parse_world(source, body, world):
    for head, first, _last, inner in statements(source, *body):
        match = re.fullmatch(r"(import|export) ([a-z0-9-]+)", head)
        if not match or inner is not None:
            raise Refusal(f"{WIT}:{first}: not understood in world `{world['name']}`: {head}")
        world[match.group(1) + "s"].append(match.group(2))


# --- writing ------------------------------------------------------------------------------


def cell(docs):
    """Comment lines as one table cell: joined, pipes and angle brackets escaped."""
    text = " ".join(line.strip() for line in docs if line.strip())
    return text.replace("|", "\\|").replace("<", "&lt;").replace(">", "&gt;")


def code(text):
    return f"`{text.lstrip('%')}`"


def render(contract):
    out = [
        f"<!-- Generated by scripts/wit-reference.sh from {WIT}. Edit the WIT, not this part. -->",
        "",
        f"### Contract reference: `{contract['package']}`",
        "",
        "Every world a plugin can be built against, and every interface in the contract, as the WIT",
        "states them. A world's name is also the scaffold's: `rcommander plugin new --type <name>`",
        "for the world `<name>-plugin`.",
        "",
        "#### Worlds",
        "",
        "| World | Imports | Exports | Scaffold |",
        "| --- | --- | --- | --- |",
    ]
    for world in contract["worlds"]:
        template = world["name"].removesuffix("-plugin")
        out.append(
            f"| {code(world['name'])} | {', '.join(map(code, world['imports']))} "
            f"| {', '.join(map(code, world['exports']))} | `--type {template}` |")
    described = [world for world in contract["worlds"] if world["docs"]]
    if described:
        out.append("")
        for world in described:
            out.append(f"- {code(world['name'])}: {cell(world['docs'])}")
    for interface in contract["interfaces"]:
        out += ["", f"#### Interface `{interface['name']}`", ""]
        if interface["docs"]:
            out += [cell(interface["docs"]), ""]
        for source, names in interface["uses"]:
            out += [f"Uses {', '.join(map(code, names))} from {code(source)}.", ""]
        for function in interface["functions"]:
            signature = f"{function['name']}: func({function['params']})"
            if function["result"]:
                signature += f" -> {function['result']}"
            line = f"- `{signature}`"
            if function["docs"]:
                line += f" - {cell(function['docs'])}"
            out.append(line)
        if interface["functions"]:
            out.append("")
        for item in interface["types"]:
            if item["kind"] == "type":
                line = f"**type {code(item['name'])}** = {code(item['alias'])}"
                if item["docs"]:
                    line += f": {cell(item['docs'])}"
                out += [line, ""]
                continue
            out += [f"**{item['kind']} {code(item['name'])}**", ""]
            if item["docs"]:
                out += [cell(item["docs"]), ""]
            if item["kind"] == "record":
                out += ["| Field | Type |", "| --- | --- |"]
                out += [f"| {code(m['name'])} | {code(m['type'])} |" for m in item["members"]]
            elif item["kind"] == "variant":
                out += ["| Case | Payload |", "| --- | --- |"]
                out += [f"| {code(m['name'])} | {code(m['type']) if m['type'] else ''} |"
                        for m in item["members"]]
            else:
                out.append(", ".join(code(member["name"]) for member in item["members"]))
            out.append("")
    while out[-1] == "":
        out.pop()
    text = "\n".join(out) + "\n"
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")
    return without_traces(text)


def without_traces(text):
    """`text` without the job-id and ADR parentheses of the WIT's comments."""
    text = TRACE.sub("", text)
    left = JOB_ID.search(text)
    if left:
        line = text[text.rfind("\n", 0, left.start()) + 1 : text.find("\n", left.end())]
        raise Refusal(f"{WIT}: a job id outside a parenthesis of its own: {line.strip()}")
    return text


def splice(page, generated, where):
    """`page` with the part between the markers replaced by `generated`."""
    begin, end = page.find(BEGIN), page.find(END)
    if begin < 0 or end < 0 or end < begin or page.count(BEGIN) != 1 or page.count(END) != 1:
        raise Refusal(f"{where}: needs exactly one `{BEGIN}` followed by one `{END}`")
    return page[: begin + len(BEGIN)] + "\n" + generated + page[end:]


def main(argv):
    if not argv or argv[0].startswith("-"):
        print(__doc__, file=sys.stderr)
        return 2
    repo, args = argv[0], argv[1:]
    check = "--check" in args
    show = "--print" in args
    wiki = None
    if "--wiki" in args:
        position = args.index("--wiki")
        if position + 1 >= len(args):
            print("--wiki needs a directory", file=sys.stderr)
            return 2
        wiki = args[position + 1]
    known = {"--check", "--print", "--wiki", wiki}
    unknown = [arg for arg in args if arg not in known]
    if unknown:
        print(f"unknown argument: {unknown[0]}", file=sys.stderr)
        return 2
    try:
        with open(os.path.join(repo, WIT), encoding="utf-8") as handle:
            contract = parse(handle.read())
        generated = render(contract)
        if show:
            sys.stdout.write(generated)
        if wiki is None:
            if check:
                print(f"wit-reference: {WIT} read completely "
                      f"({len(contract['worlds'])} worlds, "
                      f"{len(contract['interfaces'])} interfaces)")
            return 0
        path = os.path.join(wiki, PAGE)
        with open(path, encoding="utf-8") as handle:
            page = handle.read()
        updated = splice(page, generated, path)
    except (Refusal, OSError) as refusal:
        print(f"wit-reference: {refusal}", file=sys.stderr)
        return 2
    if updated == page:
        print(f"wit-reference: {path} is current")
        return 0
    if check:
        print(f"wit-reference: {path} differs from {WIT}; "
              f"run scripts/wit-reference.sh --wiki {wiki}")
        return 1
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(updated)
    print(f"wit-reference: wrote {path}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
