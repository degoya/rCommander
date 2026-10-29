#!/usr/bin/env bash
#
# scripts/lib/mcp_coverage.py against a copy of the Rust tables in a temp directory: the page
# is written (a command's stricter class from ESCALATIONS shown), --check passes on it, a summary
# edit makes --check fail as stale, and a command without a coverage row, a row for an unknown
# command, a command with rows in two group files or a missing escalation table is refused with
# exit 2 - the preflight catches a new command before anything compiles.
#
# Pure python3 and bash: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/mcp-coverage.sh
# shellcheck disable=SC2016  # the condition is single-quoted on purpose: expect_true evaluates it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/lib/mcp_coverage.py"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
REGISTRY="$TREE/crates/rc-engine/src/registry.rs"
PANEL="$TREE/crates/rc-engine/src/registry/panel.rs"
COVERAGE="$TREE/crates/rc-mcp/src/coverage"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$(dirname "$PANEL")" "$COVERAGE"
cp "$ROOT/crates/rc-engine/src/registry.rs" "$REGISTRY"
cp "$ROOT"/crates/rc-engine/src/registry/*.rs "$(dirname "$PANEL")/"
cp "$ROOT"/crates/rc-mcp/src/coverage/*.rs "$COVERAGE/"

run_status python3 "$SCRIPT" "$TREE"
expect_status "the page is written" 0
expect_true "the page lists navigate" 'grep -qF "| \`panel.navigate\` | read | \`navigate\` |" "$TREE/crates/rc-mcp/mcp-coverage.md"'
expect "the page equals the committed one" "$(cat "$ROOT/crates/rc-mcp/mcp-coverage.md")" "$(cat "$TREE/crates/rc-mcp/mcp-coverage.md")"
expect_true "the page shows a stricter class" 'grep -qF "| \`ops.copy\` | write (destructive when on_conflict overwrites) |" "$TREE/crates/rc-mcp/mcp-coverage.md"'
run_status python3 "$SCRIPT" "$TREE" --check
expect_status "--check passes on a current page" 0

sed -i 's/Select or deselect entries/Select or unselect entries/' "$PANEL"
run_status python3 "$SCRIPT" "$TREE" --check
expect_status "--check fails on a stale page" 1
expect_output "--check names the script" "run scripts/mcp-coverage.sh"

python3 - "$PANEL" <<'PY'
import sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
row = '    CommandSpec {\n        id: "panel.zz_new",\n        class: CommandClass::Destructive,\n        summary: "A new command",\n    },\n];'
open(path, "w", encoding="utf-8").write(text.replace("\n];", "\n" + row, 1))
PY
run_status python3 "$SCRIPT" "$TREE" --check
expect_status "a command without a coverage row is refused" 2
expect_output "the missing command is named" "panel.zz_new"

cp "$ROOT/crates/rc-engine/src/registry/panel.rs" "$PANEL"
sed -i 's/("panel.navigate", Coverage::Covered("navigate")),/("panel.navigate", Coverage::Covered("navigate")),\n    ("panel.gone", Coverage::Omitted("removed long ago")),/' "$COVERAGE/panel.rs"
run_status python3 "$SCRIPT" "$TREE" --check
expect_status "a row for an unknown command is refused" 2
expect_output "the unknown command is named" "panel.gone"

cp "$ROOT/crates/rc-mcp/src/coverage/panel.rs" "$COVERAGE/panel.rs"
sed -i 's/^pub(super) const ROWS: &\[(&str, Coverage)\] = &\[$/&\n    ("panel.navigate", Coverage::Covered("navigate")),/' "$COVERAGE/tabs.rs"
run_status python3 "$SCRIPT" "$TREE" --check
expect_status "a command with rows in two group files is refused" 2
expect_output "the twice-listed command is named" "panel.navigate: two coverage rows"

cp "$ROOT/crates/rc-mcp/src/coverage/tabs.rs" "$COVERAGE/tabs.rs"
rm "$(dirname "$PANEL")/escalation.rs"
run_status python3 "$SCRIPT" "$TREE" --check
expect_status "a missing escalation table is refused" 2
expect_output "the missing table is named" "escalation.rs"

finish_tests mcp-coverage
