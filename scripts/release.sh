#!/usr/bin/env bash
set -euo pipefail

CARGO_TOML="$(dirname "$0")/../Cargo.toml"

# --- helpers ---
die() { echo "error: $*" >&2; exit 1; }
current_version() { grep '^version' "$CARGO_TOML" | head -1 | sed 's/.*"\(.*\)".*/\1/'; }
bump() {
  local ver="$1" part="$2"
  IFS='.' read -r maj min pat <<< "$ver"
  case "$part" in
    major) echo "$((maj+1)).0.0" ;;
    minor) echo "${maj}.$((min+1)).0" ;;
    patch) echo "${maj}.${min}.$((pat+1))" ;;
  esac
}

# --- preflight ---
command -v cargo >/dev/null || die "cargo not found"
[[ -z "$(git status --porcelain)" ]] || die "working tree is dirty — commit or stash changes first"

CURRENT="$(current_version)"
echo "current version: $CURRENT"

# --- determine bump type ---
BUMP="${1:-}"
if [[ -z "$BUMP" ]]; then
  echo "bump type? [patch/minor/major] (default: patch)"
  read -r BUMP
  BUMP="${BUMP:-patch}"
fi
[[ "$BUMP" =~ ^(patch|minor|major)$ ]] || die "invalid bump type: $BUMP"

NEXT="$(bump "$CURRENT" "$BUMP")"
TAG="v${NEXT}"

echo ""
echo "  $CURRENT  →  $NEXT  (tag: $TAG)"
echo ""
read -r -p "proceed? [y/N] " CONFIRM
[[ "$CONFIRM" =~ ^[Yy]$ ]] || { echo "aborted"; exit 0; }

# --- bump version in Cargo.toml ---
if [[ "$(uname)" == "Darwin" ]]; then
  sed -i '' "s/^version = \"${CURRENT}\"/version = \"${NEXT}\"/" "$CARGO_TOML"
else
  sed -i "s/^version = \"${CURRENT}\"/version = \"${NEXT}\"/" "$CARGO_TOML"
fi

# --- verify build ---
echo "verifying build..."
cargo build --release -q

# --- commit + tag + push ---
git add "$CARGO_TOML" Cargo.lock
git commit -m "chore(release): bump version to ${NEXT}"
git tag "$TAG" -m "release ${NEXT}"

echo ""
echo "created commit and tag $TAG"
echo "push now? [y/N]"
read -r PUSH
if [[ "$PUSH" =~ ^[Yy]$ ]]; then
  NOCHK=1 git push origin HEAD
  NOCHK=1 git push origin "$TAG"
  echo "pushed — GitHub Actions will build, publish the release, and update the Homebrew formula"
else
  echo "run when ready:"
  echo "  NOCHK=1 git push origin HEAD && NOCHK=1 git push origin $TAG"
fi
