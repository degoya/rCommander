# shellcheck shell=bash
#
# A PowerShell command line built from WSL (RC-110-52): what scripts/run-windows.sh prints for a
# read-only C:, to be pasted into a PowerShell session unchanged. Sourced, never run.
#
#   rc_ps_quote <word>                     # one word as a PowerShell single-quoted string
#   rc_run_windows_ps1_command <ps1> <exe> [<arguments>...]
#                                          # the run-windows.ps1 command, every word quoted

# <word> in single quotes, every quote inside doubled. PowerShell takes the typographic single
# quotes (U+2018 to U+201B) for the plain one, so those are doubled too.
rc_ps_quote() {
    local word="$1" quote
    for quote in "'" $'\xe2\x80\x98' $'\xe2\x80\x99' $'\xe2\x80\x9a' $'\xe2\x80\x9b'; do
        word="${word//"$quote"/"$quote$quote"}"
    done
    printf "'%s'" "$word"
}

# The command that copies <exe> (a Windows path) with scripts/run-windows.ps1 (<ps1>, a Windows
# path) and starts it with <arguments>, each word quoted on its own: a value with spaces stays one
# argument, and an argument such as `--out` reaches run-windows.ps1 as a string.
rc_run_windows_ps1_command() {
    local ps1="$1" exe="$2" line word
    shift 2
    line="powershell -NoProfile -ExecutionPolicy Bypass -File $(rc_ps_quote "$ps1") -Binary $(rc_ps_quote "$exe")"
    for word in "$@"; do
        line+=" $(rc_ps_quote "$word")"
    done
    printf '%s\n' "$line"
}
