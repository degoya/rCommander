# shellcheck shell=bash
# shellcheck disable=SC2154  # the globals are release-pipeline.sh's, which sources this file
#
# The release chain's steps from preflight to verify-artifacts (scripts/release-pipeline.sh; the
# order and the reasons: scripts/lib/release-plan.sh). Expects VERSION, NONCE, PRERELEASE,
# DO_PUSH, RELEASE_BRANCH, MAIN_BRANCH and the working directory at the checkout root.

# The cargo processes that build, one `<pid> <command line>` each: named exactly `cargo`, with a
# subcommand that builds. Not `pgrep -f`, which counts every shell whose command line names one.
heavy_cargo_jobs() {
    local pid args pattern='(^|/)cargo( \+[^ ]+)? (build|test|clippy|nextest|xwin|rustc)( |$)'
    for pid in $(pgrep -x cargo || true); do
        args="$(ps -o args= -p "$pid" 2> /dev/null)" || continue
        if [[ "$args" =~ $pattern ]]; then printf '%s %s\n' "$pid" "$args"; fi
    done
}

step_preflight() {
    local branch tool others
    if [[ "$PRERELEASE" -eq 1 ]]; then
        echo "pre-release $VERSION on $RELEASE_BRANCH; $MAIN_BRANCH stays on the last stable release"
    else
        echo "release $VERSION from $RELEASE_BRANCH into $MAIN_BRANCH"
    fi
    echo "run nonce: $NONCE"
    branch="$(git rev-parse --abbrev-ref HEAD)"
    [[ "$branch" == "$RELEASE_BRANCH" ]] || {
        echo "on '$branch', but a release is cut from '$RELEASE_BRANCH'" >&2; return 1; }
    if [[ -n "$(git status --porcelain)" ]]; then
        echo "the working tree is dirty; a release starts from a committed state" >&2
        git status --short >&2
        return 1
    fi
    if git rev-parse -q --verify "refs/tags/v$VERSION" > /dev/null; then
        echo "tag v$VERSION already exists" >&2
        return 1
    fi
    git rev-parse -q --verify "refs/heads/$MAIN_BRANCH" > /dev/null || {
        echo "there is no branch '$MAIN_BRANCH' to merge the release into" >&2; return 1; }
    for tool in cargo cargo-nextest python3 zip aarch64-linux-gnu-gcc; do
        command -v "$tool" > /dev/null || { echo "missing tool: $tool" >&2; return 1; }
    done
    cargo xwin --version > /dev/null 2>&1 || { echo "missing: cargo xwin" >&2; return 1; }
    if [[ "$DO_PUSH" -eq 1 ]]; then
        command -v gh > /dev/null || { echo "missing tool: gh (the public CI needs it)" >&2; return 1; }
    fi
    echo "tools: $(cargo --version), $(cargo xwin --version 2>&1 | sed -n 1p)"
    # Concurrency is the other way this machine dies: one heavy job at a time.
    others="$(heavy_cargo_jobs)"
    if [[ -n "$others" ]]; then
        echo "$(wc -l <<< "$others") cargo job(s) already running; refusing to pile on" >&2
        printf '%s\n' "$others" >&2
        return 1
    fi
    rc_release_docs_check "$VERSION" --before-bump || return 1
    scripts/check.sh --preflight || {
        echo "the preflight is red; the release does not start (every finding above)" >&2; return 1; }
    echo "preflight ok at $(git rev-parse --short HEAD)"
}

step_version_bump() {
    local reported
    scripts/set-version.sh "$VERSION" || return 1
    reported="$(scripts/set-version.sh)" || return 1
    [[ "$reported" == "$VERSION" ]] || { echo "the version is $reported after the bump" >&2; return 1; }
    scripts/set-version.sh "$VERSION" --check || return 1
    echo "the workspace, Cargo.lock and the SDK report $reported"
}

# The Definition of done's gate, whole: Linux clippy and every test, the Windows lint.
step_test() { scripts/check.sh --full; }

# The packages are checks: the release's assets come from the tag's release.yml. They are built
# from the bumped, not yet committed tree, so VERSION.txt names `<commit>-dirty`; RC_REPACK=1 lets
# the release candidate replace an earlier local package of the same version, which the release
# is the one build of.
step_package_windows() { RC_REPACK=1 scripts/package-windows.sh --profile release; }
step_package_linux_x64() { RC_REPACK=1 scripts/package-linux.sh --profile release --arch x64; }
step_package_linux_arm64() { RC_REPACK=1 scripts/package-linux.sh --profile release --arch arm64; }

# The archives, as the asset names of release.yml spell them, and what each must carry.
rc_release_archives() {
    printf '%s\n' "artifacts/windows/rcommander-$1-windows-x64.zip" \
        "artifacts/linux/rcommander-$1-linux-x64.tar.gz" \
        "artifacts/linux/rcommander-$1-linux-arm64.tar.gz"
}

step_verify_artifacts() {
    local expected archive failed=0
    expected="$(scripts/build-plugins.sh --list | wc -l)" || return 1
    echo "bundled plugins expected: $expected"
    while IFS= read -r archive; do
        if [[ ! -s "$archive" ]]; then
            echo "MISS $archive" >&2
            failed=1
            continue
        fi
        python3 -I - "$archive" "$VERSION" "$expected" << 'PY' || failed=1
import sys
import tarfile
import zipfile

archive, version, expected = sys.argv[1], sys.argv[2], int(sys.argv[3])
stem = archive.rsplit("/", 1)[-1].removesuffix(".zip").removesuffix(".tar.gz")
problems = []
names, lines = [], []
try:
    if zipfile.is_zipfile(archive):
        with zipfile.ZipFile(archive) as package:
            names = package.namelist()
            text = package.read(f"{stem}/VERSION.txt")
    else:
        with tarfile.open(archive) as package:
            names = package.getnames()
            text = package.extractfile(f"{stem}/VERSION.txt").read()
    lines = text.decode("utf-8").splitlines()
except (OSError, KeyError, AttributeError, UnicodeDecodeError, tarfile.TarError, zipfile.BadZipFile) as error:
    problems.append(f"no readable VERSION.txt ({error})")
if lines and lines[0] != f"rCommander {version}":
    problems.append(f"VERSION.txt names '{lines[0]}'")
if lines and not any(line.endswith("profile release") for line in lines):
    problems.append("VERSION.txt names no release profile")
plugins = [n for n in names if n.startswith(f"{stem}/plugins/") and n.endswith(".rcplug")]
if len(plugins) != expected:
    problems.append(f"{len(plugins)} plugins, expected {expected}")
for name in ("README.md", "LICENSE", "NOTICE"):
    if f"{stem}/{name}" not in names:
        problems.append(f"no {name}")
if problems:
    print(f"!! {archive}: " + "; ".join(problems), file=sys.stderr)
    sys.exit(1)
print(f"ok   {archive}: {len(names)} entries, {len(plugins)} plugins, {lines[1] if len(lines) > 1 else ''}")
PY
    done < <(rc_release_archives "$VERSION")
    [[ "$failed" -eq 0 ]]
}
