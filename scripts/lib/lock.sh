# shellcheck shell=bash
#
# One heavy job at a time on this machine, as code rather than as a sentence somebody has to
# remember: two cargo runs side by side in two checkouts are how a WSL machine goes into swap.
#
# Usage, at the top of a script that builds, tests, lints or packages, after ROOT is known and
# BEFORE the script changes directory or parses its arguments:
#
#     source "$ROOT/scripts/lib/lock.sh"
#     rc_take_lock "$@"
#
# The function does not hold a lock in this shell; it re-runs the script under `flock`, so the
# lock lives exactly as long as that process does and is released even on a kill -9. Always with
# `flock -o`: without it the script inherits the lock's descriptor, and so does every daemon it
# starts - the sccache server outlives the run and would hold the lock until it exits.
#
# Never wrap a script that sources this file in `flock` yourself: the outer lock and the script's
# own are the same file, and the run deadlocks until RC_LOCK_WAIT runs out.
#
# Every checkout builds in its own target/ (cargo's default), so a worktree never links another
# checkout's rlib; the lock is what keeps their builds from running at once.
#
# Environment:
#   RC_LOCK_FILE    the lock (default /tmp/rc-build.lock)
#   RC_LOCK_WAIT    seconds to wait for the lock, then for memory, before giving up (default
#                   7200); a run that gives up exits 199 (lock) or 198 (memory)
#   RC_MIN_FREE_MB  MiB of MemAvailable a run waits for once it holds the lock (default 6144;
#                   0 switches the gate off)
#   RC_NO_LOCK=1    run without the lock (say why in the log you keep)
#   RC_LOCK_HELD    set by this function before the re-exec to the PID of the script that took
#                   the lock, so a chain (package-windows.sh calling another locked script)
#                   holds ONE lock and does not deadlock on it. Trusted only while that PID is
#                   one of the caller's ancestors and the lock is held: a process that merely
#                   inherited the variable (a daemon the run started, a stale export) takes the
#                   lock like any other
#   RC_NO_SCCACHE=1 do not put sccache in front of rustc even where it is installed
#   SCCACHE_CACHE_SIZE  the cache's ceiling (default 40G), read only by an sccache server this
#                   run starts; a server already running keeps its size. The size that always
#                   holds is `[cache.disk] size` in ~/.config/sccache/config (docs/development.md)

# Resolved while this file is sourced, which is before the caller does its `cd "$ROOT"`; after
# that a relative "$0" would no longer name the script.
RC_LOCK_SELF="$0"
case "$RC_LOCK_SELF" in
    /*) ;;
    *) RC_LOCK_SELF="$PWD/$RC_LOCK_SELF" ;;
esac

# Waits while less than RC_MIN_FREE_MB of memory is available, so a heavy run does not start into
# a machine that is already short. The lock only keeps these scripts apart; the load that fills
# memory may come from anywhere (an IDE, a browser, another project's build). MemAvailable is the
# kernel's estimate of what can be had without swapping. A system without /proc/meminfo (macOS)
# is not gated.
rc_wait_for_memory() {
    local minimum="${RC_MIN_FREE_MB:-6144}" limit="${RC_LOCK_WAIT:-7200}" available waited=0
    if [[ ! "$minimum" =~ ^(0|[1-9][0-9]*)$ ]]; then
        echo "RC_MIN_FREE_MB must be a whole number of MiB, not '$minimum'" >&2
        exit 2
    fi
    [[ "$minimum" -eq 0 || ! -r /proc/meminfo ]] && return 0
    while :; do
        available="$(awk '/^MemAvailable:/ { print int($2 / 1024) }' /proc/meminfo)"
        [[ -z "$available" || "$available" -ge "$minimum" ]] && return 0
        if [[ "$waited" -ge "$limit" ]]; then
            echo "!! gave up after ${waited}s waiting for memory: ${available} MiB available," >&2
            echo "   RC_MIN_FREE_MB=${minimum} wanted; nothing was run." >&2
            exit 198
        fi
        if (( waited % 30 == 0 )); then
            echo "==> waiting for memory: ${available} MiB available, ${minimum} MiB wanted (RC_MIN_FREE_MB)"
        fi
        sleep 5
        waited=$((waited + 5))
    done
}

# Puts sccache in front of rustc when it is installed, so a worktree's first build is served from
# the cache instead of compiled again. sccache cannot cache incremental output, so
# CARGO_INCREMENTAL=0 comes with it. A RUSTC_WRAPPER the caller already set is theirs.
rc_use_sccache() {
    [[ "${RC_NO_SCCACHE:-0}" = 1 || -n "${RUSTC_WRAPPER:-}" ]] && return 0
    local sccache
    sccache="$(command -v sccache 2> /dev/null)" || return 0
    export RUSTC_WRAPPER="$sccache"
    export CARGO_INCREMENTAL=0
    export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-40G}"
    echo "==> sccache in front of rustc ($sccache; a server this run starts has the ceiling $SCCACHE_CACHE_SIZE)"
}

# Whether PID $1 is an ancestor of this shell, read from /proc or, without it, from ps.
rc_lock_ancestor() {
    local want="$1" pid="$$" parent
    while [[ "$pid" =~ ^[0-9]+$ && "$pid" -gt 1 ]]; do
        if [[ -r "/proc/$pid/status" ]]; then
            parent="$(awk '/^PPid:/ { print $2 }' "/proc/$pid/status" 2> /dev/null)"
        else
            parent="$(ps -o ppid= -p "$pid" 2> /dev/null | tr -d ' ')"
        fi
        [[ "$parent" == "$want" ]] && return 0
        pid="$parent"
    done
    return 1
}

rc_take_lock() {
    local lockfile="${RC_LOCK_FILE:-/tmp/rc-build.lock}" status=0
    # The re-executed child, and every locked script it calls, arrives here with the lock held by
    # an ancestor: the memory gate belongs inside the lock, so a run that got in first cannot take
    # what was seen.
    if [[ -n "${RC_LOCK_HELD:-}" ]]; then
        # PID 1 is everyone's ancestor, so the old value `1` never counts.
        if [[ "$RC_LOCK_HELD" =~ ^[1-9][0-9]*$ && "$RC_LOCK_HELD" -gt 1 ]] \
            && rc_lock_ancestor "$RC_LOCK_HELD" \
            && ! flock -n "$lockfile" true 2> /dev/null; then
            rc_wait_for_memory
            rc_use_sccache
            return 0
        fi
        echo "note: RC_LOCK_HELD=$RC_LOCK_HELD is no caller holding $lockfile; taking the lock" >&2
        unset RC_LOCK_HELD
    fi
    if [[ "${RC_NO_LOCK:-0}" = 1 ]] || ! command -v flock > /dev/null 2>&1; then
        [[ "${RC_NO_LOCK:-0}" = 1 ]] || echo "note: flock is not installed, so this run is not serialised" >&2
        rc_wait_for_memory
        rc_use_sccache
        return 0
    fi

    # Only to tell the waiting case from the immediate one, so a run that sits still says why.
    flock -n "$lockfile" true 2> /dev/null \
        || echo "==> waiting for $lockfile (another heavy job is running)"
    # Not `exec flock ...`: on a timeout `flock` exits 1 and prints nothing, so a run that never
    # started would look like a script that ran and failed. `-E 199` gives it a code of its own.
    RC_LOCK_HELD=$$ flock -o -E 199 -w "${RC_LOCK_WAIT:-7200}" "$lockfile" "$RC_LOCK_SELF" "$@" \
        || status=$?
    if [[ "$status" -eq 199 ]]; then
        echo "!! gave up after ${RC_LOCK_WAIT:-7200}s waiting for $lockfile; nothing was run." >&2
        echo "   diagnose with: fuser -v $lockfile" >&2
    fi
    exit "$status"
}
