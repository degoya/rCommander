#!/usr/bin/env bash
#
# scripts/package-windows.sh and scripts/package-linux.sh without a build: copies in a scratch
# repository whose scripts/build-plugins.sh is a stand-in that notes it was reached and stops the
# run with exit 3, so a run that gets past the checks of scripts/lib/package.sh ends there. A
# package of the same version from another commit or a dirty tree, an empty version and a
# failing scripts/set-version.sh --check stop the run before it; RC_REPACK=1 replaces the package.
#
# Pure bash and python3, a second.
#
#   scripts/tests/package.sh
# shellcheck disable=SC2016  # the conditions are single-quoted on purpose: expect_true evaluates them
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
REACHED="$SCRATCH/reached"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$TREE/scripts/lib" "$TREE/artifacts/windows" "$TREE/artifacts/linux"
cp "$ROOT/scripts/package-windows.sh" "$ROOT/scripts/package-linux.sh" "$TREE/scripts/"
cp "$ROOT/scripts/lib/jobs.sh" "$ROOT/scripts/lib/lock.sh" "$ROOT/scripts/lib/package.sh" \
    "$ROOT/scripts/lib/workspace-version.sh" "$TREE/scripts/lib/"
printf '%s\n' '#!/usr/bin/env bash' 'echo reached > "$RC_FAKE_REACHED"' 'exit 3' \
    > "$TREE/scripts/build-plugins.sh"
printf '%s\n' '#!/usr/bin/env bash' '[[ "${RC_FAKE_VERSION_CHECK:-ok}" == ok ]]' \
    > "$TREE/scripts/set-version.sh"
printf '%s\n' '[workspace]' '' '[workspace.package]' 'version = "1.2.3"' > "$TREE/Cargo.toml"
printf '%s\n' '/artifacts/' > "$TREE/.gitignore"
chmod +x "$TREE"/scripts/*.sh
git -C "$TREE" init -q
git -C "$TREE" add -A
git -C "$TREE" -c user.name=test -c user.email=test@example.invalid commit -qm tree
HEAD_COMMIT="$(git -C "$TREE" rev-parse --short=8 HEAD)"
ZIP="$TREE/artifacts/windows/rcommander-1.2.3-windows-x64.zip"
TARBALL="$TREE/artifacts/linux/rcommander-1.2.3-linux-x64.tar.gz"

# A Windows package whose VERSION.txt names commit $1.
zip_built_from() {
    python3 -I - "$ZIP" "$1" << 'PY'
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1], "w") as package:
    package.writestr("rcommander-1.2.3-windows-x64/VERSION.txt",
                     f"rCommander 1.2.3\ncommit {sys.argv[2]}\nbuilt now\n")
PY
}

# A Linux package whose VERSION.txt names commit $1.
tarball_built_from() {
    local stage="$SCRATCH/rcommander-1.2.3-linux-x64"
    mkdir -p "$stage"
    printf '%s\n' "rCommander 1.2.3" "commit $1" "built now" > "$stage/VERSION.txt"
    tar -C "$SCRATCH" -czf "$TARBALL" "$(basename "$stage")"
    rm -rf "$stage"
}

package() {
    rm -f "$REACHED"
    run_status env RC_NO_LOCK=1 RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1 RC_FAKE_REACHED="$REACHED" \
        "$TREE/scripts/package-$1.sh"
}
reached() { [[ -f "$REACHED" ]]; }

package windows
expect_status "no package yet: the build starts" 3

zip_built_from deadbeef
package windows
expect_status "the same version from another commit stops the run" 1
expect_output "and names both commits" "built from commit deadbeef; this build is $HEAD_COMMIT"
expect_true "before anything is built" '! reached'

RC_REPACK=1 package windows
expect_status "RC_REPACK=1 replaces it" 3

zip_built_from "$HEAD_COMMIT"
package windows
expect_status "the same commit builds again" 3

touch "$TREE/untracked.txt"
package windows
expect_status "an untracked file makes the tree dirty, which stops the run" 1
expect_output "named -dirty" "this build is $HEAD_COMMIT-dirty"
rm -f "$TREE/untracked.txt"

zip_built_from "$HEAD_COMMIT-dirty"
package windows
expect_status "a package from a dirty tree is never taken for this build" 1

printf 'not a zip\n' > "$ZIP"
package windows
expect_status "a package without a readable VERSION.txt stops the run" 1
expect_output "and says so" "built from commit (not readable)"
rm -f "$ZIP"

RC_FAKE_VERSION_CHECK=fail package windows
expect_status "a failing set-version.sh --check stops the run" 1
expect_output "and is named" "scripts/set-version.sh --check failed"
expect_true "before anything is built" '! reached'

cp "$TREE/Cargo.toml" "$SCRATCH/Cargo.toml"
printf '%s\n' '[workspace]' > "$TREE/Cargo.toml"
package windows
expect_status "an empty version stops the run" 1
expect_output "and is named" "no [workspace.package] version"
expect_true "no package without a version is written" '[[ ! -e "$TREE/artifacts/windows/rcommander--windows-x64.zip" ]]'
cp "$SCRATCH/Cargo.toml" "$TREE/Cargo.toml"

tarball_built_from deadbeef
package linux
expect_status "linux: the same version from another commit stops the run" 1
expect_output "and names both commits" "built from commit deadbeef; this build is $HEAD_COMMIT"

tarball_built_from "$HEAD_COMMIT"
package linux
expect_status "linux: the same commit builds again" 3

finish_tests package
