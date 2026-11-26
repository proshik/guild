#!/usr/bin/env bash
set -euo pipefail
app=${1:?app output}
bin=${2:?binary directory}
runtime=${3:?prepared renderer}
# This assembler replaces only owned build output, never an installed/user app.
python3 - "$app" "$0" <<'PYOUTPUT'
from pathlib import Path
import sys
root=Path(sys.argv[2]).resolve().parents[1]
output=Path(sys.argv[1]).resolve()
assert output.is_relative_to(root/'target') and output.suffix=='.app', 'app output must be under this checkout target/'
assert not Path(sys.argv[1]).is_symlink(), 'refusing linked app output'
PYOUTPUT
[[ "$(uname -m)" == arm64 ]]
version=$(grep '^version' crates/desktop/Cargo.toml | head -1 | cut -d'"' -f2)


rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

icons=$(mktemp -d)/guild.iconset
mkdir -p "$icons"
for size in 16 32 128 256 512; do
    sips -z $size $size assets/icon.png --out "$icons/icon_${size}x${size}.png" >/dev/null
    sips -z $((size*2)) $((size*2)) assets/icon.png --out "$icons/icon_${size}x${size}@2x.png" >/dev/null
done
# A new asset has a new name, so LaunchServices cannot reuse the old icon.
icon_name=guild-$(shasum -a 256 assets/icon.png | cut -c1-16)
iconutil -c icns "$icons" -o "$app/Contents/Resources/$icon_name.icns"

build_number=$(git rev-list --count HEAD)
sed -e "s/VERSION/$version/g" -e "s/BUILD_NUMBER/$build_number/g" \
    -e "s/ICON_NAME/$icon_name/g" assets/Info.plist > "$app/Contents/Info.plist"
cp "$bin/guild-desktop" "$app/Contents/MacOS/"
# The command sessions call, beside the window that answers it: the
# window puts this folder on every session's PATH (M13).
cp "$bin/guild" "$app/Contents/MacOS/"
cp assets/update-keys.json "$app/Contents/Resources/"
if [ "${GITHUB_ACTIONS:-false}" = true ] || [ "${GUILD_PRODUCTION_BUNDLE:-0}" = 1 ]; then
    "$bin/guild-release" keys-check assets/update-keys.json
fi
# One timestamp and an exact selected tag feed both identity formats.
bash scripts/write-build-info.sh "$app/Contents/Resources" "$bin/guild-release"
cp CHANGELOG.md "$app/Contents/Resources/"
# And the helper that swaps a bundle in, so an update depends on no
# checkout: the window runs its own bundle's copy (M5.15).
cp scripts/swap-guild.sh "$app/Contents/Resources/"

# Helper/JS stay separate from the Rust main binary. Only committed browser
# resources are copied; no dependency fetching or app-support component adoption.
cp "$runtime/guild-diagram-renderer" "$app/Contents/MacOS/"
cp -R crates/desktop/assets/diagrams "$app/Contents/Resources/"
cp tools/diagrams/renderer/LICENSE "$app/Contents/Resources/diagrams/HELPER-LICENSE"
# Existing ad-hoc trust route: sign secondary executables first. The outer
# app signing signs the primary executable and seals its resource envelope.
lipo "$app/Contents/MacOS/guild-desktop" -verify_arch arm64
for executable in guild guild-diagram-renderer; do
    codesign --force --sign - "$app/Contents/MacOS/$executable"
    lipo "$app/Contents/MacOS/$executable" -verify_arch arm64
done
shasum -a 256 "$app/Contents/MacOS/guild-diagram-renderer" | cut -d' ' -f1 > "$app/Contents/Resources/diagrams/helper.sha256"
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
    # Reproducible assembly from fixed compiled inputs: the normal release route
    # retains its generated UUID. No source/build identity schema is changed.
    python3 - "$app" <<'PYIDENTITY'
from pathlib import Path
import hashlib,json,sys,uuid
app=Path(sys.argv[1]);metadata=app/'Contents/Resources/build-info.json'
fingerprint=hashlib.sha256()
for path in sorted(app.rglob('*')):
    if not path.is_file() or path==metadata: continue
    fingerprint.update(path.relative_to(app).as_posix().encode()+b'\0')
    with path.open('rb') as stream:
        for chunk in iter(lambda:stream.read(1024*1024),b''):fingerprint.update(chunk)
info=json.loads(metadata.read_text())
fingerprint.update(json.dumps({k:v for k,v in info.items() if k!='build_id'},sort_keys=True).encode())
info['build_id']=str(uuid.uuid5(uuid.NAMESPACE_URL,'guild-reproducible-bundle:'+fingerprint.hexdigest()))
metadata.write_text(json.dumps(info,indent=2)+'\n')
PYIDENTITY
fi
codesign --force --sign - "$app"
codesign --verify --deep --strict "$app"
python3 tools/diagrams/renderer/verify.py "$app"
if [ -n "${SOURCE_DATE_EPOCH:-}" ]; then
    python3 - "$app" "$SOURCE_DATE_EPOCH" <<'PYTIME'
from pathlib import Path
import os,sys
app=Path(sys.argv[1]);epoch=int(sys.argv[2])
for path in sorted(app.rglob('*'),reverse=True): os.utime(path,(epoch,epoch))
os.utime(app,(epoch,epoch))
PYTIME
fi
codesign --verify --deep --strict "$app"
echo "built: $app"
