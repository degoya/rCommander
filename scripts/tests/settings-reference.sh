#!/usr/bin/env bash
#
# scripts/lib/settings_reference.py against a copy of the settings sources in a temp directory: the
# page is written into a temp wiki with the rows the schema gives - constants followed into the
# area modules and into rc_ops, rc_vault and rc_plugin_host, `..base`, the Appearance tabs, the
# protected prefixes, access-key marks (`&`) left out of labels and titles but not help texts -
# --check passes on it, a changed help text makes --check fail as stale, and
# what does not read is refused with exit 2: a row with an expression the reader does not know
# (named), a spec constant left out of SETTINGS, a constant another crate no longer has, a kind of
# value without words, a label missing from the catalogue. Without a wiki --check skips the page
# with exit 0 and still reads the data.
#
# Pure python3 and bash: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/settings-reference.sh
# shellcheck disable=SC2016  # the condition is single-quoted on purpose: expect_true evaluates it
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/scripts/lib/settings_reference.py"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
WIKI="$SCRATCH/wiki"
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
PAGE="$WIKI/reference/settings.md"
SCHEMA="$TREE/crates/rc-engine/src/settings/schema.rs"
AREAS="$TREE/crates/rc-engine/src/settings/schema"
CATALOGUE="$TREE/crates/rcommander/locales/en.json"
DIALOG="crates/rcommander/src/app/settings"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$TREE/crates/rcommander/locales" "$TREE/$DIALOG" "$WIKI"
for crate in rc-engine rc-ops rc-vault rc-plugin-host; do
    mkdir -p "$TREE/crates/$crate"
    cp -r "$ROOT/crates/$crate/src" "$TREE/crates/$crate/"
done
cp "$ROOT/crates/rcommander/locales/en.json" "$CATALOGUE"
cp "$ROOT/$DIALOG.rs" "$TREE/$DIALOG.rs"
cp "$ROOT/$DIALOG/view.rs" "$ROOT/$DIALOG/tabs.rs" "$TREE/$DIALOG/"
# Restores one copied file from the repository after a case changed it.
restore() { cp "$ROOT/${1#"$TREE/"}" "$1"; }

run_status python3 -B "$SCRIPT" "$TREE" "$WIKI"
expect_status "the page is written" 0
expect_true "a choice with its words and values" \
    'grep -qF "| Theme | \`appearance.theme\` | One of System (\`system\`), Light (\`light\`), Dark (\`dark\`) | System |" "$PAGE"'
expect_true "a default computed in rc_ops" \
    'grep -qF "| \`transfers.buffer_kib\` | A whole number from 16 to 16384 | 256 |" "$PAGE"'
expect_true "bounds from rc_plugin_host" \
    'grep -qF "| \`plugins.crash_limit\` | A whole number from 1 to 10 | 3 |" "$PAGE"'
expect_true "a key from rc_vault" \
    'grep -qE "^\| [^|]+ \| \`vault.idle_lock_minutes\` \|.*\*\*Protected\.\*\* \|$" "$PAGE"'
expect_true "a constant through crate::" 'grep -qF "| \`vault.lock_on_session_lock\` |" "$PAGE"'
expect_true "..base keeps the helper's fields and overrides one" \
    'grep -qE "^\| [^|]+ \| \`exchange.export.encrypted\` \| On or off \| On \|.*\*\*Protected\.\*\* \|$" "$PAGE"'
expect_true "a setting that needs a restart" \
    'grep -qE "^\| [^|]+ \| \`folders.left_start\` \|.*\*\*Restart\.\*\* \|$" "$PAGE"'
expect_true "a text default" 'grep -qF "| \`viewer.dos_extensions\` | File extensions without dots, separated by spaces | \`nfo diz\` |" "$PAGE"'
expect_true "the Appearance tabs" 'grep -qF "### Appearance > Fonts" "$PAGE"'
expect_true "the pages in the dialog's order" \
    '[[ "$(grep -E "^## " "$PAGE" | tr "\n" ",")" == "## Appearance,## Folders,## Keyboard,## Terminal,## Vault,## MCP server,## Programs,## Transfers,## Plugins,## Import/Export,## Updates," ]]'
expect_true "markup in a help text is escaped" 'grep -qF "csv=\\<plugin id\\>" "$PAGE"'
expect_true "the plugins' own settings are linked" 'grep -qF "](../using/plugins#the-bundled-viewers)" "$PAGE"'
expect_true "the count is every row of SETTINGS" 'grep -qE "^[0-9]+ settings on 11 pages: [0-9]+ protected" "$PAGE"'
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "--check passes on a current page" 0

sed -i 's/"setting.mcp.http.port.help": "The port/"setting.mcp.http.port.help": "The TCP port/' "$CATALOGUE"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "--check fails on a stale page" 1
expect_output "--check names the script" "run scripts/settings-reference.sh"
restore "$CATALOGUE"

# Access-key marks (RC-080-16) as the window reads them: gone before a letter, `&&` one `&`, an `&`
# before anything else kept - in labels and titles; a help text is shown as written.
sed -i -e 's/"setting.appearance.theme": "Theme"/"setting.appearance.theme": "\&Overwrite"/' \
    -e 's/"setting.appearance.function_key_bar": "Show the function-key bar"/"setting.appearance.function_key_bar": "S\&kip"/' \
    -e 's/"setting.appearance.alternate_rows": "Alternate row colours"/"setting.appearance.alternate_rows": "A \&\& B"/' \
    -e 's/"setting.appearance.menu_bar": "Menu bar"/"setting.appearance.menu_bar": "\& x"/' \
    -e 's/"settings.page.folders": "Folders"/"settings.page.folders": "F\&olders"/' \
    -e 's/"setting.appearance.menu_bar.help": "The menu bar/"setting.appearance.menu_bar.help": "\&Keep the menu bar/' \
    "$CATALOGUE"
mkdir -p "$SCRATCH/marks"
run_status python3 -B "$SCRIPT" "$TREE" "$SCRATCH/marks"
expect_status "a catalogue with access-key marks is written" 0
# shellcheck disable=SC2034  # read by the conditions expect_true evaluates
MARKS="$SCRATCH/marks/reference/settings.md"
expect_true "&Overwrite loses its mark" 'grep -qF "| Overwrite | \`appearance.theme\` |" "$MARKS"'
expect_true "S&kip loses its mark" 'grep -qF "| Skip | \`appearance.function_key_bar\` |" "$MARKS"'
expect_true "A && B is one ampersand" 'grep -qF "| A & B | \`appearance.alternate_rows\` |" "$MARKS"'
expect_true "& x keeps its ampersand" 'grep -qF "| & x | \`appearance.menu_bar\` |" "$MARKS"'
expect_true "a page title loses its mark" 'grep -qxF "## Folders" "$MARKS"'
expect_true "a help text is shown as written" 'grep -qF "| &Keep the menu bar at the top" "$MARKS"'
restore "$CATALOGUE"

sed -i '/key: MCP_HEADLESS_CODE_KEY,/,/protected: true,/s/protected: true,/protected: false,/' "$AREAS/mcp.rs"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "an mcp.* row is protected by its prefix alone" 0
restore "$AREAS/mcp.rs"

sed -i 's/        default: 500,/        default: tip_default(),/' "$AREAS/fonts.rs"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a row the reader cannot evaluate is refused" 2
expect_output "the row is named" "(fonts::NAME_TIP_DELAY): "
restore "$AREAS/fonts.rs"

sed -i '/^    fonts::MARK_HIDDEN,$/d' "$SCHEMA"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a spec constant left out of SETTINGS is refused" 2
expect_output "the counts are named" "were read from SETTINGS"
restore "$SCHEMA"

sed -i 's/pub const DEFAULT_CRASH_LIMIT:/pub const DEFAULT_CRASH_LIMITS:/' "$TREE/crates/rc-plugin-host/src/health.rs"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a constant another crate no longer has is refused" 2
expect_output "the row and the constant are named" "(plugins::CRASH_LIMIT): "
restore "$TREE/crates/rc-plugin-host/src/health.rs"

sed -i 's/^pub enum SettingKind {$/pub enum SettingKind {\n    Gradient { default: \&'"'"'static str },/' "$SCHEMA"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a kind of value without words is refused" 2
expect_output "the kind is named" "SettingKind::Gradient has no words"
restore "$SCHEMA"

sed -i '/"setting.appearance.theme": /d' "$CATALOGUE"
run_status python3 -B "$SCRIPT" "$TREE" "$WIKI" --check
expect_status "a label missing from the catalogue is refused" 2
expect_output "the text is named" "no setting.appearance.theme"
restore "$CATALOGUE"

run_status python3 -B "$SCRIPT" "$TREE" "$SCRATCH/no-wiki" --check
expect_status "without a wiki --check reads the data only" 0
expect_output "the skip is said" "the page is not checked"
run_status python3 -B "$SCRIPT" "$TREE" "$SCRATCH/no-wiki"
expect_status "without a wiki nothing is written" 2

# The wrapper without RC_WIKI, in a checkout with no wiki beside it (a CI): --check skips the page
# with a notice and exit 0; the data is still read.
mkdir -p "$TREE/scripts/lib"
cp "$ROOT/scripts/settings-reference.sh" "$TREE/scripts/"
cp "$ROOT/scripts/lib/settings_reference.py" "$ROOT/scripts/lib/settings_reference_rust.py" "$TREE/scripts/lib/"
git init -q "$TREE"
run_status env -u RC_WIKI "$TREE/scripts/settings-reference.sh" --check
expect_status "no wiki beside the checkout: --check passes" 0
expect_output "the skip names the place looked at" "no wiki at $SCRATCH/rcommander.wiki"
sed -i '/"setting.appearance.theme": /d' "$CATALOGUE"
run_status env -u RC_WIKI "$TREE/scripts/settings-reference.sh" --check
expect_status "no wiki, but the data is still read" 2
restore "$CATALOGUE"
mkdir -p "$SCRATCH/rcommander.wiki"
run_status env -u RC_WIKI "$TREE/scripts/settings-reference.sh" --check
expect_status "a wiki beside the checkout without the page is stale" 1

finish_tests settings-reference
