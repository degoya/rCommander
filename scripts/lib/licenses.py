"""The crate licences of the About page, crates/rcommander/resources/licenses/third-party.json.

The inputs are `cargo metadata --format-version 1` (every package of Cargo.lock with its declared
licence) and the output of `cargo tree --prefix none --format '{p}'` for the rcommander binary on
each target, concatenated (`name vX.Y.Z` per line, a workspace member with its path, a repeated
crate marked `(*)`). A locked crate the trees name is listed with its licence under `rust`; every
other one of Cargo.lock is named under `rust_not_shipped`. A crate that declares no licence (or
only a licence file) takes its entry from crates/rcommander/resources/licenses/overrides.json,
`{"name@version": {"license": "<SPDX>", "source": "<where it was read>"}}`.

    python3 scripts/lib/licenses.py <repo root> <metadata.json> <linked.txt> [--check]

Exit 0 when written (or, with --check, current); 1 when --check finds the list stale; 2 when a
crate has no licence and no override, an override is for no locked crate, or an input does not
read.
"""

import json
import sys
from pathlib import Path

TARGET = "crates/rcommander/resources/licenses/third-party.json"
OVERRIDES = "crates/rcommander/resources/licenses/overrides.json"


def linked(text: str) -> set[str]:
    """`name@version` of every crate a `cargo tree` output names."""
    crates = set()
    for line in text.splitlines():
        fields = line.split()
        if len(fields) >= 2 and fields[1].startswith("v"):
            crates.add(f"{fields[0]}@{fields[1][1:]}")
    return crates


def render(metadata: dict, shipped: set[str], overrides: dict) -> tuple[str, list[str]]:
    """The file's text and the crates that have no licence."""
    locked = {
        f"{package['name']}@{package['version']}": package
        for package in metadata["packages"]
        # No source: a workspace member, rCommander itself.
        if package.get("source") is not None
    }
    missing = [f"{key}: an override for no locked crate" for key in sorted(overrides) if key not in locked]
    rust, not_shipped = [], []
    for key in sorted(locked, key=lambda key: (locked[key]["name"], locked[key]["version"])):
        package = locked[key]
        if key not in shipped:
            not_shipped.append(key)
            continue
        license = (overrides.get(key) or {}).get("license") or (package.get("license") or "").strip()
        if not license:
            missing.append(f"{key}: declares no licence")
        rust.append({"name": package["name"], "version": package["version"], "license": license})

    def rows(entries: list) -> str:
        return ",\n".join("    " + json.dumps(entry, ensure_ascii=False) for entry in entries)

    text = (
        "{\n"
        f'  "rust": [\n{rows(rust)}\n  ],\n'
        f'  "rust_not_shipped": [\n{rows(not_shipped)}\n  ]\n'
        "}\n"
    )
    return text, missing


def main(argv: list[str]) -> int:
    if len(argv) < 4 or any(arg != "--check" for arg in argv[4:]):
        print("usage: licenses.py <repo root> <metadata.json> <linked.txt> [--check]", file=sys.stderr)
        return 2
    root = Path(argv[1])
    try:
        metadata = json.loads(Path(argv[2]).read_text(encoding="utf-8"))
        shipped = linked(Path(argv[3]).read_text(encoding="utf-8"))
        overrides_path = root / OVERRIDES
        overrides = json.loads(overrides_path.read_text(encoding="utf-8")) if overrides_path.exists() else {}
        text, missing = render(metadata, shipped, overrides)
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 2
    if missing:
        print(f"error: record each in {OVERRIDES} with the place its licence was read:", file=sys.stderr)
        for line in missing:
            print(f"  {line}", file=sys.stderr)
        return 2
    target = root / TARGET
    current = target.read_text(encoding="utf-8") if target.exists() else None
    count = text.count('"license"')
    if "--check" in argv[4:]:
        if current != text:
            print(f"{TARGET} is stale: run scripts/licenses.sh", file=sys.stderr)
            return 1
        print(f"==> {TARGET} is current")
        return 0
    if current == text:
        print(f"==> {TARGET} was already current")
    else:
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")
        print(f"==> wrote {TARGET}: {count} crates")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
