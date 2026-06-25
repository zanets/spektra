#!/usr/bin/env bash
# Usage: ./scripts/release.sh <version>   e.g. ./scripts/release.sh 0.2.0
set -euo pipefail

VERSION="${1:?Usage: ./scripts/release.sh <version>  (e.g. 0.2.0)}"
TAG="v$VERSION"
REPO="zanets/spektra"
ASSET_ARM64="spektra-macos-arm64.tar.gz"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TAP_DIR="$(cd "$SCRIPT_DIR/../../homebrew-tap" && pwd)"
FORMULA="$TAP_DIR/Formula/spektra.rb"

cd "$REPO_DIR"

echo "→ Releasing $TAG"

# 1. Bump version in Cargo.toml + Cargo.lock
sed -i '' "s/^version = \"[0-9.]*\"/version = \"$VERSION\"/" Cargo.toml
cargo update --package spektra

# 2. Commit, tag, push
git add Cargo.toml Cargo.lock
git commit -m "chore(release): bump version to $VERSION"
git tag "$TAG"
NOCHK=1 git push origin master
git push origin "$TAG"
echo "✓ Tag $TAG pushed — CI build started"

# 3. Wait for the GitHub release to be created by CI
echo "→ Waiting for GitHub release to appear..."
until gh release view "$TAG" --repo "$REPO" &>/dev/null 2>&1; do
  printf '.'; sleep 15
done
echo " found"

# 4. Wait for the macOS tarball asset to be uploaded
echo "→ Waiting for $ASSET_ARM64..."
until gh release view "$TAG" --repo "$REPO" --json assets \
    -q '.assets[].name' 2>/dev/null | grep -qx "$ASSET_ARM64"; do
  printf '.'; sleep 20
done
echo " ready"

# 5. Download and compute sha256
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

gh release download "$TAG" --repo "$REPO" -p "$ASSET_ARM64" -D "$TMP"
SHA_ARM64=$(shasum -a 256 "$TMP/$ASSET_ARM64" | awk '{print $1}')
echo "✓ sha256 (arm64): $SHA_ARM64"

# 6. Update formula — version and sha256
sed -i '' "s/version \"[0-9.]*\"/version \"$VERSION\"/" "$FORMULA"
sed -i '' "s/sha256 \"[a-f0-9]*\"/sha256 \"$SHA_ARM64\"/" "$FORMULA"

# 7. Commit and push homebrew tap
cd "$TAP_DIR"
git add Formula/spektra.rb
git commit -m "feat(spektra): release $TAG"
NOCHK=1 git push origin main
echo "✓ Homebrew tap updated"

echo ""
echo "Done!  Install with:"
echo "  brew tap zanets/tap"
echo "  brew install spektra"
