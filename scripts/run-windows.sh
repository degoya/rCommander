#!/usr/bin/env bash
#
# Builds rcommander.exe for Windows from WSL (cargo-xwin), copies it to
# %LOCALAPPDATA%\rcommander-dev\ and starts it there through WSL interop, detached. Windows does
# not run an .exe from the WSL file system reliably (and SmartScreen and file locks behave
# differently there), hence the copy.
#
# Usage:
#   scripts/run-windows.sh                  # debug build
#   scripts/run-windows.sh --release        # release build
#   scripts/run-windows.sh [--release] -- <arguments for rcommander.exe>
#
# Takes the build lock (scripts/lib/lock.sh) - never wrap it in flock. A running rcommander.exe
# locks the file: close it first, or the copy fails.
set -euo pipefail

TARGET="x86_64-pc-windows-msvc"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/jobs.sh
source "$ROOT/scripts/lib/jobs.sh"
# shellcheck source=lib/lock.sh
source "$ROOT/scripts/lib/lock.sh"
rc_take_lock "$@"
cd "$ROOT"

usage() {
    echo "usage: scripts/run-windows.sh [--release] [-- <arguments>]" >&2
    exit 2
}

profile=dev
directory=debug
while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) profile=release; directory=release; shift ;;
        --) shift; break ;;
        *) usage ;;
    esac
done

# WSL appends the Windows PATH only with `appendWindowsPath`; the file is there regardless.
cmd="$(command -v cmd.exe || echo /mnt/c/Windows/System32/cmd.exe)"
if ! command -v wslpath > /dev/null 2>&1 || [[ ! -e "$cmd" ]]; then
    echo "!! wslpath or cmd.exe not found: this needs WSL" >&2
    exit 1
fi
if ! compgen -G '/proc/sys/fs/binfmt_misc/WSLInterop*' > /dev/null; then
    echo "!! WSL interop is off (no WSLInterop binfmt entry): enable [interop] in /etc/wsl.conf" >&2
    exit 1
fi

echo "==> cross-building rcommander.exe ($profile, jobs $JOBS)"
CARGO_BUILD_JOBS="$JOBS" cargo xwin build --locked --profile "$profile" --target "$TARGET" \
    -j "$JOBS" -p rcommander
binary="${CARGO_TARGET_DIR:-$ROOT/target}/$TARGET/$directory/rcommander.exe"
[[ -f "$binary" ]] || { echo "!! the build left no $binary" >&2; exit 1; }

# cmd.exe started from a WSL directory warns about UNC paths; from /mnt/c it does not.
localappdata="$(cd /mnt/c && "$cmd" /d /c 'echo %LOCALAPPDATA%' 2> /dev/null | tr -d '\r')"
if [[ -z "$localappdata" || "$localappdata" == "%LOCALAPPDATA%" ]]; then
    echo "!! could not read %LOCALAPPDATA% through cmd.exe" >&2
    exit 1
fi
destination="$(wslpath -u "$localappdata")/rcommander-dev"
mkdir -p "$destination"
install -m 755 "$binary" "$destination/rcommander.exe"
windows_exe="$(wslpath -w "$destination/rcommander.exe")"
echo "==> starting $windows_exe $*"
# `start ""` detaches on the Windows side; setsid and the redirections detach it from this shell.
(cd /mnt/c && setsid "$cmd" /d /c start "" "$windows_exe" "$@" > /dev/null 2>&1 < /dev/null &)
echo "==> started"
