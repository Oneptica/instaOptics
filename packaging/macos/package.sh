#!/usr/bin/env bash
# macOS: a universal instaOptics.app in dist/instaoptics-<version>-macos-universal.dmg, and the CLI as a zip.
# Signed ad hoc unless MACOS_SIGN_IDENTITY names a Developer ID.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"
export MACOSX_DEPLOYMENT_TARGET=11.0
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/instaOptics.app"
DMG="$DIST/instaoptics-$VERSION-macos-universal.dmg"
CLI_ZIP="$DIST/instaoptics-cli-$VERSION-macos-universal.zip"
[[ "${1:-}" = --skip-build ]] || (cd "$ROOT" && cargo build --release --locked -p instaoptics -p instaoptics-cli --target "${TARGETS[0]}" --target "${TARGETS[1]}")
rm -rf "$WORK"
mkdir -p "$WORK/bin" "$APP/Contents/MacOS" "$APP/Contents/Resources"
for bin in instaoptics instaoptics-cli; do
  lipo -create -output "$WORK/bin/$bin" "$CARGO_TARGET_DIR/${TARGETS[0]}/release/$bin" "$CARGO_TARGET_DIR/${TARGETS[1]}/release/$bin"
done
ICONSET="$WORK/instaOptics.iconset"
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s "$ROOT/assets/icon.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$ROOT/assets/icon.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/instaOptics.icns"
cp "$WORK/bin/instaoptics" "$APP/Contents/MacOS/instaOptics"
sed "s/@VERSION@/$VERSION/g" "$HERE/Info.plist.in" >"$APP/Contents/Info.plist"
printf 'APPL????' >"$APP/Contents/PkgInfo"
codesign --force --options runtime --sign "$IDENTITY" "$APP"
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
ditto "$APP" "$STAGE/instaOptics.app"
ln -s /Applications "$STAGE/Applications"
rm -f "$DMG"
hdiutil create -volname instaOptics -srcfolder "$STAGE" -format UDZO -o "$DMG"
echo "wrote $DMG"
CLI_DIR="$WORK/instaoptics-cli-$VERSION-macos-universal"
mkdir -p "$CLI_DIR"
cp "$WORK/bin/instaoptics-cli" "$ROOT/README.md" "$ROOT/LICENSE" "$CLI_DIR/"
codesign --force --sign "$IDENTITY" "$CLI_DIR/instaoptics-cli"
ditto -c -k --keepParent "$CLI_DIR" "$CLI_ZIP"
echo "wrote $CLI_ZIP"
