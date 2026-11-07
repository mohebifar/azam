#!/usr/bin/env sh
# Fetches azam's BUI fork into third_party/bui: upstream BUI at the pinned commit plus azam's patch.
# Run once after cloning azam (and again after third_party/bui-azam.patch changes).
set -eu
root="$(cd "$(dirname "$0")/.." && pwd)"
dir="$root/third_party/bui"
commit="$(cat "$root/third_party/bui-base-commit.txt")"
if [ ! -d "$dir/.git" ]; then
  git clone https://github.com/ppmpreetham/BUI "$dir"
fi
cd "$dir"
git fetch --quiet origin "$commit" 2>/dev/null || git fetch --quiet origin
git checkout --quiet --force "$commit"
git clean --quiet -fd
git apply "$root/third_party/bui-azam.patch"
echo "BUI fork ready at $dir ($(git rev-parse --short HEAD) + azam patch)"
