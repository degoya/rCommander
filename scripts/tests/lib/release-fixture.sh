# shellcheck shell=bash
#
# A throwaway repository the release scripts run in (scripts/tests/release-pipeline.sh,
# release-evidence.sh): the real release-pipeline.sh, tag-release.sh and their libraries copied in,
# every script they call a stand-in that prints what it was asked and, for the packages, writes an
# archive the way the real ones lay it out. Nothing builds, nothing leaves the machine: `origin` is
# a bare repository beside it, and the tools the preflight asks for are stand-ins on PATH.
#
#   source "$ROOT/scripts/tests/lib/release-fixture.sh"
#   release_fixture <dir>     # makes <dir>/tree (on development) and <dir>/origin.git
#
# Stand-ins read FAKE_FAIL_CHECK (check.sh --full exits 3 while the file exists) and
# FAKE_FAIL_PUBLIC_CI (public-ci.sh exits 1 while it exists); every call of a publishing script
# is appended to $FAKE_CALLS.
# shellcheck disable=SC2016  # the stand-ins' bodies are single-quoted on purpose: they expand when run

export GIT_AUTHOR_NAME=test GIT_AUTHOR_EMAIL=test@example.invalid GIT_COMMITTER_NAME=test \
    GIT_COMMITTER_EMAIL=test@example.invalid GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1
export RC_NO_LOCK=1 RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1

# A stand-in script $1 with the body $2, executable.
fake() {
    printf '#!/usr/bin/env bash\nset -euo pipefail\n%s\n' "$2" > "$1"
    chmod +x "$1"
}

release_fixture() {
    local dir="$1" tree="$1/tree" bin="$1/bin" file
    mkdir -p "$tree/scripts/lib" "$bin"
    cp "$ROOT/scripts/release-pipeline.sh" "$ROOT/scripts/tag-release.sh" "$tree/scripts/"
    for file in lock release-tag release-plan release-evidence release-docs release-steps-build \
        release-steps-publish changelog; do
        cp "$ROOT/scripts/lib/$file.sh" "$tree/scripts/lib/"
    done
    export FAKE_CALLS="$dir/calls" FAKE_FAIL_CHECK="$dir/fail-check" FAKE_FAIL_PUBLIC_CI="$dir/fail-ci"

    # The tools the preflight asks for, and a pgrep that sees no other cargo job.
    fake "$bin/cargo" 'case "$*" in "xwin --version") echo "cargo-xwin 0.0.0" ;; *) echo "cargo 0.0.0" ;; esac'
    for file in cargo-nextest zip aarch64-linux-gnu-gcc gh pgrep; do fake "$bin/$file" 'exit 0'; done
    # An archive as package-windows.sh and package-linux.sh lay it out: <stem>/VERSION.txt, the
    # documents, two plugins.
    fake "$bin/fake-package" 'python3 -I - "$@" << "PY"
import io, sys, tarfile, zipfile
from pathlib import Path
path, version = Path(sys.argv[1]), sys.argv[2]
stem = path.name.removesuffix(".zip").removesuffix(".tar.gz")
files = {f"{stem}/VERSION.txt": f"rCommander {version}\ncommit 1234abcd-dirty\nbuilt now\ntarget t, profile release\n",
         f"{stem}/README.md": "r", f"{stem}/LICENSE": "l", f"{stem}/NOTICE": "n",
         f"{stem}/plugins/a-1.0.0.rcplug": "a", f"{stem}/plugins/b-1.0.0.rcplug": "b"}
path.parent.mkdir(parents=True, exist_ok=True)
if path.suffix == ".zip":
    with zipfile.ZipFile(path, "w") as z:
        for name, text in files.items(): z.writestr(name, text)
else:
    with tarfile.open(path, "w:gz") as t:
        for name, text in files.items():
            data = text.encode(); info = tarfile.TarInfo(name); info.size = len(data); t.addfile(info, io.BytesIO(data))
print(f"wrote {path}")
PY'

    fake "$tree/scripts/set-version.sh" 'cd "$(dirname "$0")/.."
source scripts/lib/changelog.sh
case "${1:-}" in
    "") cat VERSION ;;
    --check) echo "versions agree" ;;
    *)
        if [[ "${2:-}" == --check ]]; then [[ "$(cat VERSION)" == "$1" ]]; echo "version is $1"; exit 0; fi
        [[ "$(cat VERSION)" == "$1" ]] || echo "$1" > VERSION
        rc_changelog_cut CHANGELOG.md "$1" 2026-10-29
        echo "version set to $1" ;;
esac'
    fake "$tree/scripts/check.sh" 'echo "check.sh $*"
[[ "$*" != --full || ! -e "$FAKE_FAIL_CHECK" ]] || { echo "a test failed"; exit 3; }
echo "==> all requested checks passed"'
    fake "$tree/scripts/build-plugins.sh" 'printf "%s\n" a b'
    fake "$tree/scripts/package-windows.sh" 'v="$(scripts/set-version.sh)"
fake-package "artifacts/windows/rcommander-$v-windows-x64.zip" "$v"'
    fake "$tree/scripts/package-linux.sh" 'v="$(scripts/set-version.sh)"
fake-package "artifacts/linux/rcommander-$v-linux-${!#}.tar.gz" "$v"'
    fake "$tree/scripts/archive-jobs.sh" 'echo "archive-jobs $*: nothing due"'
    fake "$tree/scripts/public-ci.sh" 'echo "public-ci $*" >> "$FAKE_CALLS"; echo "public-ci $*"
[[ ! -e "$FAKE_FAIL_PUBLIC_CI" ]] || { echo "ci.yml: windows red"; exit 1; }'
    for file in export-public export-wiki update-website; do
        fake "$tree/scripts/$file.sh" "echo \"$file \$*\" >> \"\$FAKE_CALLS\"; echo \"$file \$*\""
    done

    printf '/artifacts/\n' > "$tree/.gitignore"
    echo 9.9.8 > "$tree/VERSION"
    printf '# Changelog\n\n## [Unreleased]\n\n### Added\n\n- A thing.\n\n## [9.9.8] - 2026-10-01\n\n- Older.\n' \
        > "$tree/CHANGELOG.md"
    printf '# Release notes\n\n## 9.9.9\n\n- Something a user notices.\n' > "$tree/RELEASE-NOTES.md"
    git init -q -b main "$tree"
    git -C "$tree" add -A
    git -C "$tree" commit -qm base
    git -C "$tree" switch -q -c development
    git init -q --bare "$dir/origin.git"
    git -C "$tree" remote add origin "$dir/origin.git"
    export PATH="$bin:$PATH"
}
