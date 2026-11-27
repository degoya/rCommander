#!/usr/bin/env bash
#
# The PowerShell command scripts/run-windows.sh prints for a read-only C: (RC-110-52), without
# Windows: lib/powershell.sh quotes every word in single quotes (a quote inside doubled, the
# typographic ones too), a value with spaces stays one word, an argument such as `--out` is a
# quoted string; and run-windows.sh prints its command through the library and exits 3 then. The
# Windows half (run-windows.ps1 reading $args and quoting for the command line) needs PowerShell;
# where pwsh is installed, its quoting is checked against the words a command line splits into.
#
# Pure bash and python3: it runs in a second. check.sh --preflight runs it.
#
#   scripts/tests/run-windows.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"
# shellcheck source=../lib/powershell.sh
source "$ROOT/scripts/lib/powershell.sh"

expect "a plain word" "'--out'" "$(rc_ps_quote --out)"
expect "a value with spaces" "'C:\\Program Files\\x'" "$(rc_ps_quote 'C:\Program Files\x')"
expect "a quote inside is doubled" "'it''s'" "$(rc_ps_quote "it's")"
expect "a typographic quote is doubled" $'\'a\xe2\x80\x99\xe2\x80\x99s\'' "$(rc_ps_quote $'a\xe2\x80\x99s')"
expect "the empty word" "''" "$(rc_ps_quote '')"
expect "nothing PowerShell expands" "'\$env:X \`n \$(y) \"z\"'" "$(rc_ps_quote '$env:X `n $(y) "z"')"

expect "the command without arguments" \
    "powershell -NoProfile -ExecutionPolicy Bypass -File 'C:\\r w\\run-windows.ps1' -Binary 'C:\\r w\\rc.exe'" \
    "$(rc_run_windows_ps1_command 'C:\r w\run-windows.ps1' 'C:\r w\rc.exe')"
expect "the command of screenshots.sh, every word quoted" \
    "powershell -NoProfile -ExecutionPolicy Bypass -File 'X:\\s.ps1' -Binary 'X:\\rc.exe' '--screenshots' 'C:\\Users\\A B\\scenes.json' '--out' 'C:\\Users\\A B\\out' '--demo-root' 'C:\\rCommander-Demo'" \
    "$(rc_run_windows_ps1_command 'X:\s.ps1' 'X:\rc.exe' --screenshots 'C:\Users\A B\scenes.json' \
        --out 'C:\Users\A B\out' --demo-root 'C:\rCommander-Demo')"

# PowerShell's single-quoted strings read back: the words arrive as they went in.
run_status python3 -B -I - "$(rc_run_windows_ps1_command 'a b' "it's" '--out' "C:\\x y\\" '')" << 'EOF_PY'
import re, sys
words = re.findall(r"'((?:[^']|'')*)'", sys.argv[1])
got = [w.replace("''", "'") for w in words]
want = ["a b", "it's", "--out", "C:\\x y\\", ""]
sys.exit(0 if got == want else f"got {got!r}, want {want!r}")
EOF_PY
expect_status "every word reads back unchanged" 0

# shellcheck disable=SC2034  # SCRIPT and PS1 are read by the conditions expect_true evaluates
SCRIPT="$ROOT/scripts/run-windows.sh"
expect_true "run-windows.sh sources the library" \
    'grep -qF "source \"\$ROOT/scripts/lib/powershell.sh\"" "$SCRIPT"'
expect_true "and prints its command through it" 'grep -qE "^ +rc_run_windows_ps1_command " "$SCRIPT"'
expect_true "and no longer splices the arguments in unquoted" '! grep -qF "\${*:+ \$*}" "$SCRIPT"'
expect_true "the printed command ends in exit 3" \
    'grep -A2 "rc_run_windows_ps1_command " "$SCRIPT" | grep -qE "^ +exit 3$"'
PS1="$ROOT/scripts/run-windows.ps1"
expect_true "run-windows.ps1 is no advanced script" '! grep -qE "^[^#]*\[(Parameter|CmdletBinding)" "$PS1"'
expect_true "and has no param() block" '! grep -qE "^[[:space:]]*param[[:space:]]*\(" "$PS1"'

if command -v pwsh > /dev/null 2>&1; then
    # The quoting function of run-windows.ps1, dot-sourced in isolation, against Python's
    # reading of the MSVC rules (subprocess.list2cmdline is their inverse).
    func="$(sed -n '/^function ConvertTo-CommandLineArgument/,/^}/p' "$PS1")"
    run_status pwsh -NoProfile -Command "$func"'
        $words = @("plain", "a b", "", "q""uote", "C:\x y\", "back\\""slash", "--out")
        ($words | ForEach-Object { ConvertTo-CommandLineArgument $_ }) -join " "'
    line="$output"
    run_status python3 -B -I -c 'import subprocess, sys
words = ["plain", "a b", "", "q\"uote", "C:\\x y\\", "back\\\\\"slash", "--out"]
sys.exit(0 if subprocess.list2cmdline(words) == sys.argv[1] else f"{sys.argv[1]!r} != {subprocess.list2cmdline(words)!r}")' "$line"
    expect_status "run-windows.ps1 quotes for the command line as the MSVC rules read it" 0
else
    echo "skip pwsh not installed: run-windows.ps1's quoting is checked on Windows"
fi

finish_tests run-windows
