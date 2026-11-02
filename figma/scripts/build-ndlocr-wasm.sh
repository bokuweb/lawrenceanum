#!/usr/bin/env bash
# Build the pinned browser OCR package, or use a local checkout via NDLOCR_SOURCE.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
revision="e9d2670fb163d77d6e166362869056f032be423b"
source_dir="${NDLOCR_SOURCE:-$root/target/ndlocr-web-src-$revision}"
if [ -z "${NDLOCR_SOURCE:-}" ]; then
  mkdir -p "$source_dir"
  if [ ! -d "$source_dir/.git" ]; then
    git -C "$source_dir" init --quiet
    git -C "$source_dir" remote add origin https://github.com/bokuweb/ndlocr-lite-rs.git
  fi
  git -C "$source_dir" sparse-checkout set --no-cone '/Cargo.toml' '/src/' '/web/' '/tools/' '/examples/' '/benches/' '/docs/upstream-assets.json'
  if [ "$(git -C "$source_dir" rev-parse HEAD 2>/dev/null || true)" != "$revision" ]; then
    # Skip the repository's large benchmark archives. Only package sources are needed.
    git -C "$source_dir" config remote.origin.promisor true
    git -C "$source_dir" config remote.origin.partialclonefilter blob:none
    git -C "$source_dir" fetch --filter=blob:none --depth 1 origin "$revision"
    git -C "$source_dir" checkout --detach FETCH_HEAD
  fi
fi
npm ci --prefix "$source_dir/web"
rustup target add wasm32-unknown-unknown
bash "$source_dir/tools/build_web.sh" "$root/figma/src/wasm/ndlocr"
if [ "${NDLOCR_FETCH_MODELS:-0}" = "1" ]; then
  python3 "$source_dir/tools/fetch_upstream_assets.py" "${NDLOCR_MODEL_DIR:-$root/target/ndlocr-models}"
fi
