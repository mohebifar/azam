#!/bin/sh
# Sparse, pinned checkout of the MakeHuman CC0 system assets the avatar pack is built from.
#
#   tools/mhpack/fetch.sh [dir]        (default: target/makehuman)
#
# then
#
#   cargo run -p azam-mhpack --release -- --makehuman target/makehuman \
#       --source "$(tools/mhpack/fetch.sh --source)" --out assets/makehuman/makehuman-cc0.bin --check
set -eu
REPO=https://github.com/makehumancommunity/makehuman.git
COMMIT=a8bc2d54ff0ac92e78ff71431b1023eda42bf482
if [ "${1:-}" = "--source" ]; then
  echo "$REPO@$COMMIT"
  exit 0
fi
DIR=${1:-target/makehuman}
if [ ! -d "$DIR/.git" ]; then
  git init -q "$DIR"
  git -C "$DIR" remote add origin "$REPO"
fi
git -C "$DIR" config core.sparseCheckout true
git -C "$DIR" sparse-checkout set --no-cone \
  '/LICENSE.md' '/LICENSE.ASSETS.md' \
  '/makehuman/data/3dobjs/base.obj' \
  '/makehuman/data/rigs/default.mhskel' '/makehuman/data/rigs/default_weights.mhw' \
  '/makehuman/data/targets/measure/' \
  '/makehuman/data/targets/breast/nipple-*' \
  '/makehuman/data/targets/macrodetails/*-young*'
git -C "$DIR" fetch -q --depth 1 --filter=blob:none origin "$COMMIT"
git -C "$DIR" checkout -q FETCH_HEAD
echo "$DIR at $(git -C "$DIR" rev-parse HEAD)"
