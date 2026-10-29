# shellcheck shell=bash
# shellcheck disable=SC2154  # the globals are the sourcing script's
#
# The evidence of scripts/release-pipeline.sh (RC-100-32, rDownloader's machinery cut down): the
# markers a step leaves in the evidence log, run_step that writes them, the resume check and the
# gate that scripts/tag-release.sh asks again. The properties it keeps:
#
#   * A step's exit status is PIPESTATUS[0], never the tail of a pipe; its output is teed, never
#     filtered, so nothing between the command and `tee` turns a failure into a pass.
#   * Evidence is per run: the log's header carries a nonce, every marker repeats it, and the gate
#     ignores markers of any other run. A log left by an earlier attempt proves nothing.
#   * A step that exited 0 without output is missing evidence, not a quiet pass.
#
# The log, artifacts/release-evidence-X.Y.Z.log (artifacts/ is ignored, so it can never enter the
# release commit it is evidence for):
#   ##RC-RELEASE version=V nonce=N push=0|1 head=<sha> branch=B started=<time> host=H
#   ##RC-RESUME nonce=N push=0|1 at=<time>          one per --resume
#   ##RC-STEP id=I nonce=N version=V exit=E bytes=B started=S ended=T tree=<tree>
#   ##RC-TREE-CHANGED nonce=N was=<tree> tree=<tree> at=<time>
#
# Expects LOG, NONCE and VERSION; run_step also RESUME; rc_release_gate the list of ids.

marker_field() { sed -n "s/.* $2=\([^ ]*\).*/\1/p" <<< "$1"; }

# The nonce, and the push flag, of the last run the log $1 holds; nothing without a header.
rc_release_nonce() { [[ ! -f "$1" ]] || sed -n 's/^##RC-RELEASE .*nonce=\([^ ]*\).*/\1/p' "$1" | tail -n 1; }
rc_release_logged_version() { [[ ! -f "$1" ]] || sed -n 's/^##RC-RELEASE version=\([^ ]*\).*/\1/p' "$1" | tail -n 1; }
rc_release_push() {
    awk -v nonce="nonce=$2" '($1 == "##RC-RELEASE" || $1 == "##RC-RESUME") && index($0, " " nonce " ") {
            for (i = 1; i <= NF; i++) if ($i ~ /^push=/) push = substr($i, 6) }
        END { print (push == "" ? 0 : push) }' "$1" 2> /dev/null
}

# The tree of the working state, uncommitted and untracked files included, ignored ones not;
# `none` when git cannot say, which no later tree equals. Through a scratch index, so the real one
# is left as it is; copied with its time (`cp -p`), since git trusts an entry's stat only when the
# file is older than the index - a fresh copy made a file rewritten in the same second at the
# same size look unchanged.
rc_worktree_tree() {
    local index tree
    index="$(mktemp)"
    cp -p "$(git rev-parse --path-format=absolute --git-path index)" "$index" 2> /dev/null || : > "$index"
    if GIT_INDEX_FILE="$index" git add -A > /dev/null 2>&1; then
        tree="$(GIT_INDEX_FILE="$index" git write-tree 2> /dev/null || true)"
    fi
    rm -f "$index"
    printf '%s\n' "${tree:-none}"
}

# The last marker of step $1 in this run, or nothing. A ##RC-TREE-CHANGED line of this run hides
# every marker before it but preflight's: after a change of the tree a step stands only on a green
# of the new tree.
last_marker() {
    awk -v id="id=$1" -v nonce="nonce=$NONCE" '
        $1 == "##RC-TREE-CHANGED" && $2 == nonce && id != "id=preflight" { marker = "" }
        $1 == "##RC-STEP" && $2 == id && $3 == nonce { marker = $0 }
        END { if (marker != "") print marker }' "$LOG" 2> /dev/null || true
}

step_is_green() {
    local marker
    marker="$(last_marker "$1")"
    [[ -n "$marker" && "$(marker_field "$marker" exit)" == 0 ]] || return 1
    [[ "$(marker_field "$marker" bytes)" =~ ^[1-9][0-9]*$ ]]
}

# On --resume: the working tree against the one this run's last step left. The same tree, and
# every green step stands; any other - a fix between the stop and the resume - writes a
# ##RC-TREE-CHANGED line, and every step but preflight runs again. preflight is the start gate,
# and its checks run again inside the test step's check.sh --full.
resume_tree_check() {
    local recorded current
    recorded="$(awk -v nonce="nonce=$NONCE" '
        ($1 == "##RC-STEP" && $3 == nonce) || ($1 == "##RC-TREE-CHANGED" && $2 == nonce) { line = $0 }
        END { print line }' "$LOG" 2> /dev/null || true)"
    [[ -n "$recorded" ]] || return 0
    recorded="$(marker_field "$recorded" tree)"
    current="$(rc_worktree_tree)"
    if [[ -n "$recorded" && "$recorded" == "$current" && "$current" != none ]]; then
        echo "==> the tree is the one the last step left (${current:0:12}); its green steps stand"
        return 0
    fi
    printf '##RC-TREE-CHANGED nonce=%s was=%s tree=%s at=%s\n' \
        "$NONCE" "${recorded:-none}" "$current" "$(date -Is)" >> "$LOG"
    echo "==> the tree changed since the last step (${recorded:0:12} -> ${current:0:12}):"
    echo "    preflight stands; every other step runs again"
}

run_step() {
    local id="$1" started before after status bytes
    shift
    local -a codes
    if [[ "${RESUME:-0}" -eq 1 ]] && step_is_green "$id"; then
        echo "==> [$id] already green in this run - skipped"
        return 0
    fi
    printf '\n===== STEP %s (%s) =====\n' "$id" "$(date -Is)" >> "$LOG"
    started="$(date -Is)"
    before="$(stat -c %s "$LOG")"
    echo
    echo "==> [$id]"
    # `set +e` is what lets the status be read at all; with errexit armed the script would leave
    # before the assignment.
    set +e
    "$@" 2>&1 | tee -a "$LOG"
    codes=("${PIPESTATUS[@]}")
    set -e
    status="${codes[0]}"
    after="$(stat -c %s "$LOG")"
    bytes="$((after - before))"
    printf '##RC-STEP id=%s nonce=%s version=%s exit=%s bytes=%s started=%s ended=%s tree=%s\n' \
        "$id" "$NONCE" "$VERSION" "$status" "$bytes" "$started" "$(date -Is)" "$(rc_worktree_tree)" >> "$LOG"
    if [[ "$status" -ne 0 ]]; then
        echo >&2
        echo "!! [$id] failed with exit $status - the pipeline stops here." >&2
        echo "   evidence so far: $LOG" >&2
        echo "   fix it, then: scripts/release-pipeline.sh $VERSION --resume" >&2
        exit "$status"
    fi
    if [[ "$bytes" -le 0 ]]; then
        echo "!! [$id] exited 0 but produced no output; that is missing evidence, not a pass." >&2
        exit 1
    fi
}

# Whether every step named in $@ has a passing, non-empty record of this run; prints one line per
# step. The header of this run must be in the log.
rc_release_gate() {
    local id marker code bytes failed=0
    if ! grep -q "^##RC-RELEASE version=$VERSION nonce=$NONCE " "$LOG" 2> /dev/null; then
        echo "!! $LOG has no header of run $NONCE for $VERSION" >&2
        return 1
    fi
    for id in "$@"; do
        marker="$(last_marker "$id")"
        if [[ -z "$marker" ]]; then
            printf '  %-20s NO EVIDENCE\n' "$id" >&2
            failed=1
            continue
        fi
        code="$(marker_field "$marker" exit)"
        bytes="$(marker_field "$marker" bytes)"
        if [[ "$code" != 0 ]]; then
            printf '  %-20s exit=%s\n' "$id" "$code" >&2
            failed=1
        elif [[ ! "$bytes" =~ ^[1-9][0-9]*$ ]]; then
            printf '  %-20s exit=0 but no output\n' "$id" >&2
            failed=1
        else
            printf '  %-20s exit=0, %s bytes\n' "$id" "$bytes"
        fi
    done
    [[ "$failed" -eq 0 ]]
}
