#!/usr/bin/env bash
#
# Pins the checksum of a new SQLite migration in crates/rc-db/migrations.sha384.
#
# sqlx stores the SHA-384 of every migration file it applies and compares it on each start, so a
# database that applied a file refuses to open once a single byte of it changes - a comment
# included. crates/rc-db/tests/migration_checksums.rs fails when a file no longer matches its pin
# or has none; that second failure names this script. Pin a migration when you add it.
#
# Usage:
#   scripts/migration-pin.sh                      # pin every migration that has no entry yet
#   scripts/migration-pin.sh 0002_something.sql   # pin exactly these (name or path)
#
# The script only appends. A file whose bytes differ from its pin is refused by name: the fix is
# to restore the file and write a new migration, never to move the pin. The file is plain
# `sha384sum` output, so coreutils verify it too:
#
#   (cd crates/rc-db/migrations && sha384sum -c ../migrations.sha384)
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MIGRATIONS="$ROOT/crates/rc-db/migrations"
PINS="$ROOT/crates/rc-db/migrations.sha384"

cd "$MIGRATIONS"
touch "$PINS"

if [[ $# -eq 0 ]]; then
    shopt -s nullglob
    names=(*.sql)
else
    names=()
    for argument in "$@"; do
        names+=("$(basename "$argument")")
    done
fi

added=0
failed=0
for name in "${names[@]}"; do
    if [[ ! -f "$name" ]]; then
        echo "error: no migration $name in crates/rc-db/migrations" >&2
        failed=1
        continue
    fi
    sum="$(sha384sum -- "$name" | cut -d' ' -f1)"
    pinned="$(awk -v name="$name" '$2 == name { print $1 }' "$PINS")"
    if [[ -z "$pinned" ]]; then
        printf '%s  %s\n' "$sum" "$name" >> "$PINS"
        echo "pinned $name"
        added=$((added + 1))
    elif [[ "$pinned" != "$sum" ]]; then
        echo "error: $name differs from its pin. An applied migration never changes - restore" >&2
        echo "       its bytes and put the change in a new migration. The pin stays as it is." >&2
        failed=1
    fi
done

# One line per file, ordered by name and therefore by number.
sorted="$(LC_ALL=C sort -k2,2 "$PINS")"
[[ -z "$sorted" ]] || printf '%s\n' "$sorted" > "$PINS"

echo "$added new pin(s) in crates/rc-db/migrations.sha384"
exit "$failed"
