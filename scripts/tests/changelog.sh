#!/usr/bin/env bash
#
# scripts/lib/changelog.sh against a small changelog in a temp directory: a cut puts the
# entries of `## [Unreleased]` under `## [X.Y.Z] - date` below a new, empty `## [Unreleased]`,
# keeps the older sections, a second cut of the same version changes nothing, and a changelog
# without `## [Unreleased]` is refused.
#
# Pure bash and awk: it runs at once. check.sh --preflight runs it.
#
#   scripts/tests/changelog.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
LOG="$SCRATCH/CHANGELOG.md"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=../lib/changelog.sh
source "$ROOT/scripts/lib/changelog.sh"

printf '%s\n' '# Changelog' '' '## [Unreleased]' '' '### Added' '' '- New thing.' '' \
    '## [0.6.0] - 2026-10-25' '' '### Fixed' '' '- Old fix.' > "$LOG"

rc_changelog_cut "$LOG" 0.6.1 2026-10-26
expected="$(printf '%s\n' '# Changelog' '' '## [Unreleased]' '' '## [0.6.1] - 2026-10-26' '' \
    '### Added' '' '- New thing.' '' '## [0.6.0] - 2026-10-25' '' '### Fixed' '' '- Old fix.')"
expect "the entries go under the new version, below an empty Unreleased" "$expected" "$(cat "$LOG")"

rc_changelog_cut "$LOG" 0.6.1 2026-10-27
expect "a second cut of the same version changes nothing" "$expected" "$(cat "$LOG")"

printf '%s\n' '# Changelog' '' '## [0.6.0] - 2026-10-25' > "$LOG"
rc_changelog_cut "$LOG" 0.6.1 2026-10-26 2> "$SCRATCH/err" && status=0 || status=$?
expect_true "a changelog without Unreleased is refused" \
    '[[ $status -eq 1 ]] && grep -qF "no '"'"'## [Unreleased]'"'"' heading" "$SCRATCH/err"'

finish_tests changelog
