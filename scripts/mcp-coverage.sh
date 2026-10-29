#!/usr/bin/env bash
#
# Writes crates/rc-mcp/mcp-coverage.md: every engine command (`COMMANDS` in
# crates/rc-engine/src/registry.rs) with its class and the MCP tool that covers it, or the reason
# it is omitted (the rows of crates/rc-mcp/src/coverage/, one file per registry group).
#
# Generated, never kept by hand. It compiles nothing - the tables are read from the source by
# scripts/lib/mcp_coverage.py - so --check runs in check.sh --preflight. Two tests keep the reader
# honest: crates/rc-mcp/tests/mcp_coverage.rs fails when a command has no row (or a row no tool)
# and when the page differs from what the compiled tables render.
#
# Usage:
#   scripts/mcp-coverage.sh            # rewrite the page
#   scripts/mcp-coverage.sh --check    # exit 1 if the page is stale, 2 if a command has no row
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec python3 "$ROOT/scripts/lib/mcp_coverage.py" "$ROOT" "$@"
