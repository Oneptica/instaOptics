#!/usr/bin/env bash
# Linux packages: AppImage, .deb and .tar.gz in dist/, named instaoptics-<version>-linux-<arch>.<ext>.
# Usage: packaging/linux/package.sh [--skip-build]. The .deb needs nfpm; appimagetool is downloaded when missing.
set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=com.oneptica.instaoptics
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) DEB_ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
[[ "${1:-}" = --skip-build ]] || (cd "$ROOT" && cargo build --release --locked -p instaoptics -p instaoptics-cli)
BASENAME="instaoptics-$VERSION-linux-$ARCH"
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"
install -Dm755 "$BIN/instaoptics" "$STAGE/usr/bin/instaoptics"
install -Dm755 "$BIN/instaoptics-cli" "$STAGE/usr/bin/instaoptics-cli"
strip "$STAGE/usr/bin/instaoptics" "$STAGE/usr/bin/instaoptics-cli" 2>/dev/null || true
install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
install -Dm644 "$HERE/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$ROOT/assets/hicolor" "$STAGE/usr/share/icons/"
install -Dm644 -t "$STAGE/usr/share/doc/instaoptics" "$ROOT/README.md" "$ROOT/LICENSE"

mkdir -p "$WORK/tar"
cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
echo "wrote $DIST/$BASENAME.tar.gz"

if command -v nfpm >/dev/null; then
  export VERSION NFPM_ARCH="$DEB_ARCH" STAGE
  (cd "$ROOT" && nfpm package -f "$HERE/nfpm.yaml" -p deb -t "$DIST/$BASENAME.deb")
else
  echo "nfpm not found, so no .deb (https://nfpm.goreleaser.com/install/)" >&2
fi

APPDIR="$WORK/instaOptics.AppDir"
cp -R "$STAGE" "$APPDIR"
ln -s usr/bin/instaoptics "$APPDIR/AppRun"
cp "$HERE/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
cp "$ROOT/assets/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
ln -s "$APP_ID.png" "$APPDIR/.DirIcon"
TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
if [[ -z "$TOOL" ]]; then
  TOOL="$CARGO_TARGET_DIR/appimagetool-$ARCH.AppImage"
  [[ -x "$TOOL" ]] || { curl --proto '=https' -fsSL -o "$TOOL" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-$ARCH.AppImage"; chmod +x "$TOOL"; }
fi
ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$DIST/$BASENAME.AppImage"
echo "wrote $DIST/$BASENAME.AppImage"
