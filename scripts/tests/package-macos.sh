#!/usr/bin/env bash
#
# scripts/package-macos.sh without a Mac and without a compiler (RC-100-44): copies in a scratch
# repository, with stand-ins on PATH for `cargo` (writes a fake executable where the build would
# leave it) and `codesign` (notes its calls), and a scripts/build-plugins.sh that only lists one
# plugin, whose signed package comes in through --plugins. The zip it writes is read back: the
# layout of rCommander.app, the executable bit, Info.plist with the bundle id and the version
# (a pre-release cut to its three numbers), the icon, VERSION.txt, the plugin, and the ad hoc
# signature over the bundle. Also --plugins without the package and an unknown arch.
#
# Pure bash and python3, a second.
#
#   scripts/tests/package-macos.sh
# shellcheck disable=SC2016,SC2034  # the conditions are single-quoted on purpose: expect_true
# evaluates them, and reads the variables they name
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT
TREE="$SCRATCH/repo"
BIN="$SCRATCH/bin"
SIGNED="$SCRATCH/signed"
CODESIGN_LOG="$SCRATCH/codesign.log"

# shellcheck source=lib/expect.sh
source "$ROOT/scripts/tests/lib/expect.sh"

mkdir -p "$TREE/scripts/lib" "$TREE/plugins/demo-viewer" "$TREE/crates/rcommander/resources/brand" \
    "$BIN" "$SIGNED/fresh"
cp "$ROOT/scripts/package-macos.sh" "$TREE/scripts/"
cp "$ROOT/scripts/lib/jobs.sh" "$ROOT/scripts/lib/lock.sh" "$ROOT/scripts/lib/package.sh" \
    "$ROOT/scripts/lib/workspace-version.sh" "$ROOT/scripts/lib/icns.py" "$TREE/scripts/lib/"
printf '%s\n' '#!/usr/bin/env bash' '[[ "$1" == --list ]] || exit 3' 'echo demo-viewer' \
    > "$TREE/scripts/build-plugins.sh"
printf '%s\n' '#!/usr/bin/env bash' 'exit 0' > "$TREE/scripts/set-version.sh"
printf '%s\n' '[workspace]' '' '[workspace.package]' 'version = "1.2.3-beta.1"' > "$TREE/Cargo.toml"
printf '%s\n' 'id = "demo-viewer"' 'version = "0.4.0"' > "$TREE/plugins/demo-viewer/manifest.toml"
printf '%s\n' '/artifacts/' '/target/' > "$TREE/.gitignore"
printf 'readme\n' > "$TREE/README.md"
printf 'licence\n' > "$TREE/LICENSE"
printf 'notice\n' > "$TREE/NOTICE"
printf 'signed package\n' > "$SIGNED/demo-viewer-0.4.0.rcplug"

# A PNG of $2 x $2 pixels as far as a reader of its header can tell, into $1; an .ico holding
# a 256 px PNG entry.
python3 -I - "$TREE/crates/rcommander/resources/brand/rCommander-icon.png" "$TREE/crates/rcommander/resources/rcommander.ico" << 'PY'
import struct
import sys

def png(size):
    return b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", size, size) + b"\x08\x06\x00\x00\x00rest"

with open(sys.argv[1], "wb") as handle:
    handle.write(png(512))
image = png(256)
with open(sys.argv[2], "wb") as handle:
    handle.write(struct.pack("<HHH", 0, 1, 1) + struct.pack("<BBBBHHII", 0, 0, 0, 0, 1, 32, len(image), 22) + image)
PY

# The stand-ins: cargo leaves an executable where `cargo build --target T --profile P` would.
cat > "$BIN/cargo" << 'SH'
#!/usr/bin/env bash
target="" profile=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --target) target="$2"; shift 2 ;;
        --profile) profile="$2"; shift 2 ;;
        *) shift ;;
    esac
done
mkdir -p "target/$target/$profile"
printf '#!/bin/sh\necho fake rcommander\n' > "target/$target/$profile/rcommander"
chmod 755 "target/$target/$profile/rcommander"
SH
printf '%s\n' '#!/usr/bin/env bash' 'echo "$*" >> "$RC_FAKE_CODESIGN_LOG"' > "$BIN/codesign"
chmod +x "$TREE"/scripts/*.sh "$BIN"/*
git -C "$TREE" init -q
git -C "$TREE" add -A
git -C "$TREE" -c user.name=test -c user.email=test@example.invalid commit -qm tree
ZIP="$TREE/artifacts/macos/rcommander-1.2.3-beta.1-macos-x86_64.zip"

package() {
    run_status env PATH="$BIN:$PATH" RC_NO_LOCK=1 RC_MIN_FREE_MB=0 RC_NO_SCCACHE=1 \
        RC_FAKE_CODESIGN_LOG="$CODESIGN_LOG" "$TREE/scripts/package-macos.sh" "$@"
}

# What the zip holds: one line per entry, `<name> <mode in octal>`, then the member $2 printed.
zip_listing() {
    python3 -I - "$ZIP" << 'PY'
import sys
import zipfile

with zipfile.ZipFile(sys.argv[1]) as package:
    for info in package.infolist():
        print(info.filename, oct(info.external_attr >> 16 & 0o777))
PY
}
zip_member() {
    python3 -I -c 'import sys, zipfile; sys.stdout.buffer.write(zipfile.ZipFile(sys.argv[1]).read(sys.argv[2]))' "$ZIP" "$1"
}

package --arch x86_64 --plugins "$SIGNED"
expect_status "the package is written" 0
expect_true "as the zip of its version and arch" '[[ -f "$ZIP" ]]'
listing="$(zip_listing)"
contents="rCommander.app/Contents"
expect_true "the executable is in Contents/MacOS, executable" \
    'grep -qx "$contents/MacOS/rcommander 0o755" <<< "$listing"'
for member in Info.plist Resources/README.md Resources/LICENSE Resources/NOTICE \
    Resources/VERSION.txt Resources/rcommander.icns Resources/plugins/demo-viewer-0.4.0.rcplug; do
    expect_true "rCommander.app holds Contents/$member" 'grep -q "^$contents/$member " <<< "$listing"'
done
expect_true "nothing lies beside rCommander.app at the top" \
    '! grep -qv "^rCommander.app/" <<< "$listing"'

plist="$(zip_member "$contents/Info.plist")"
expect_true "Info.plist names the bundle id" \
    'grep -A1 CFBundleIdentifier <<< "$plist" | grep -q "<string>net.rcommander.app</string>"'
expect_true "and the executable" \
    'grep -A1 CFBundleExecutable <<< "$plist" | grep -q "<string>rcommander</string>"'
expect_true "and the version without its pre-release part" \
    'grep -A1 CFBundleShortVersionString <<< "$plist" | grep -q "<string>1.2.3</string>"'
expect_true "in both version keys" \
    'grep -A1 "<key>CFBundleVersion</key>" <<< "$plist" | grep -q "<string>1.2.3</string>"'
if python3 -I -c 'import plistlib, sys; plistlib.loads(sys.stdin.buffer.read())' <<< "$plist"; then
    ok "Info.plist is a property list"
else
    fail "Info.plist is a property list" "$plist"
fi

version_txt="$(zip_member "$contents/Resources/VERSION.txt")"
expect_true "VERSION.txt names the full version, the commit and the target" \
    'grep -qx "rCommander 1.2.3-beta.1" <<< "$version_txt" && grep -q "^commit [0-9a-f]\{8\}$" <<< "$version_txt" && grep -q "target x86_64-apple-darwin, profile release" <<< "$version_txt"'
expect "the bundled plugin is the signed package" "signed package" \
    "$(zip_member "$contents/Resources/plugins/demo-viewer-0.4.0.rcplug")"
zip_member "$contents/Resources/rcommander.icns" > "$SCRATCH/icon.icns"
expect "the icon is an icns file" "icns" "$(head -c 4 "$SCRATCH/icon.icns")"
expect_true "with the 256 and 512 px images" \
    'grep -qa ic08 "$SCRATCH/icon.icns" && grep -qa ic09 "$SCRATCH/icon.icns"'
expect_true "the bundle is signed ad hoc and verified" \
    'grep -qx -- "--force --deep --sign - $TREE/artifacts/macos/rcommander-1.2.3-beta.1-macos-x86_64/rCommander.app" "$CODESIGN_LOG" && grep -q -- "--verify --deep --strict" "$CODESIGN_LOG"'
expect_true "the stage is removed" '[[ ! -e "$TREE/artifacts/macos/rcommander-1.2.3-beta.1-macos-x86_64" ]]'

package --arch x86_64 --plugins "$SIGNED"
expect_status "the same commit packs again" 0

rm -f "$ZIP"
package --arch x86_64 --plugins "$SIGNED/fresh"
expect_status "a plugin without its package stops the run" 1
expect_output "and names the package" "no plugin package $SIGNED/fresh/demo-viewer-0.4.0.rcplug"
expect_true "before anything is written" '[[ ! -e "$ZIP" ]]'

package --arch ppc
expect_status "an unknown arch is refused" 2

package --plugins "$SCRATCH/missing"
expect_status "--plugins needs an existing directory" 2

finish_tests package-macos
