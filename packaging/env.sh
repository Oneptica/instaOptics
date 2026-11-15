# Shared by the packaging scripts: repository root, version and output folder.
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
DIST="${DIST:-$ROOT/dist}"
mkdir -p "$DIST"
