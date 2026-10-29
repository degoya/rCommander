#!/usr/bin/env bash
#
# The private-name guard of the public exports (scripts/lib/public-names.py, RC-100-30): this
# checkout first - outside scripts/public-exclude.txt it names nothing of
# scripts/public-names.txt beyond what scripts/public-names-allow.txt allows, and the allowlist
# carries no entry the tree no longer needs - then hand-written cases in a scratch tree: a git
# checkout and a plain directory, an excluded path, an allowed placeholder and a real name
# beside it, case, a wiki entry, a stale entry and a malformed one.
#
# The public repository has no scripts/public-names.txt (the export leaves it out), so there the
# checkout's half is skipped, saying so. Pure python3, git and bash: it runs in a second.
#
#   scripts/tests/public-names.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GUARD="$ROOT/scripts/lib/public-names.py"
# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

if [[ -f "$ROOT/scripts/public-names.txt" ]]; then
    run_status python3 -B "$GUARD" "$ROOT" --terms "$ROOT/scripts/public-names.txt" \
        --allow "$ROOT/scripts/public-names-allow.txt" --exclude "$ROOT/scripts/public-exclude.txt" --unused
    expect_status "this checkout names nothing private outside the allowlist" 0
    [[ "$status" -eq 0 ]] || printf '%s\n' "$output"
else
    echo "skip this checkout: no scripts/public-names.txt (the public tree leaves it out)"
fi

# The names are made up and built from pieces, so this file names none of them itself.
HOST="private.example""-host.test"
cat > "$SCRATCH/terms.txt" <<LIST
# A comment.
${HOST//./\\.}
/home/someone\\b
LIST
cat > "$SCRATCH/exclude.txt" <<'LIST'
docs/
LIST
cat > "$SCRATCH/allow.txt" <<'LIST'
# A comment.
src/paths.rs | /home/someone/example\b | a placeholder in a test
wiki/using/a.md | /home/someone/example\b | a placeholder on a page
LIST

TREE="$SCRATCH/tree"
mkdir -p "$TREE/docs" "$TREE/src"
git -C "$TREE" init -q
printf 'let p = "/home/someone/example/x";\n' > "$TREE/src/paths.rs"
printf 'Internal: clone from %s.\n' "$HOST" > "$TREE/docs/notes.md"
printf '# Readme\n' > "$TREE/README.md"

guard() { run_status python3 -B "$GUARD" "$TREE" --terms "$SCRATCH/terms.txt" --allow "$SCRATCH/allow.txt" "$@"; }

guard --exclude "$SCRATCH/exclude.txt"
expect_status "an allowed placeholder and an excluded path pass" 0

guard
expect_status "without the exclude list the excluded file is read" 1
expect_output "and named" "docs/notes.md:1: names something private"

printf 'Clone from https://%s/x.\n' "${HOST^^}" >> "$TREE/README.md"
guard --exclude "$SCRATCH/exclude.txt"
expect_status "a private host fails, in any case" 1
expect_output "and is named with file and line" "README.md:2: names something private"
printf '# Readme\n' > "$TREE/README.md"

printf 'let q = "/home/someone/example/x"; // and /home/someone/real\n' >> "$TREE/src/paths.rs"
guard --exclude "$SCRATCH/exclude.txt"
expect_status "a real name beside an allowed placeholder fails" 1
expect_output "and is named" "src/paths.rs:2: names something private"
printf 'let p = "/home/someone/example/x";\n' > "$TREE/src/paths.rs"

printf '/home/someone2 is another word\n' > "$TREE/src/other.rs"
guard --exclude "$SCRATCH/exclude.txt"
expect_status "a name inside a longer word is not the name" 0
rm "$TREE/src/other.rs"

printf 'x\0/home/someone\n' > "$TREE/src/blob.bin"
guard --exclude "$SCRATCH/exclude.txt"
expect_status "a binary file is not read" 0
rm "$TREE/src/blob.bin"

guard --exclude "$SCRATCH/exclude.txt" --unused
expect_status "--unused: every repository entry is needed" 0
printf 'src/gone.rs | /home/someone | a file that is gone\n' >> "$SCRATCH/allow.txt"
guard --exclude "$SCRATCH/exclude.txt" --unused
expect_status "--unused: an entry no line needs fails" 1
expect_output "and is named" "allow.txt:4: no line needs this entry any more"
sed -i '$d' "$SCRATCH/allow.txt"

printf 'src/*.rs | /home/someone\n' >> "$SCRATCH/allow.txt"
guard --exclude "$SCRATCH/exclude.txt"
expect_status "an entry without a reason fails" 1
expect_output "and is named" "allow.txt:4: not '<path> | <expression> | <reason>'"
sed -i '$d' "$SCRATCH/allow.txt"

# A plain directory, as the export's staging tree is: every file is read, the wiki's entries apply.
WIKI="$SCRATCH/wiki"
mkdir -p "$WIKI/using"
printf 'Your files are in /home/someone/example.\n' > "$WIKI/using/a.md"
printf 'Your files are in /home/someone/example.\n' > "$WIKI/b.md"
run_status python3 -B "$GUARD" "$WIKI" --terms "$SCRATCH/terms.txt" --allow "$SCRATCH/allow.txt" --wiki
expect_status "wiki: a page the entry names passes, another fails" 1
expect_output "and the other is named" "wiki/b.md:1: names something private"
expect_true "the allowed page is not named" '! grep -qF "wiki/using/a.md" <<< "$output"'
rm "$WIKI/b.md"
run_status python3 -B "$GUARD" "$WIKI" --terms "$SCRATCH/terms.txt" --allow "$SCRATCH/allow.txt"
expect_status "a repository check does not take the wiki's entries" 1

run_status python3 -B "$GUARD" "$WIKI" --allow "$SCRATCH/allow.txt"
expect_status "without --terms it is a usage error" 2

finish_tests public-names
