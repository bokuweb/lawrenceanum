#!/usr/bin/env bash
# Build the pinned browser OCR package, or use a local checkout via NDLOCR_SOURCE.
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
revision="e9d2670fb163d77d6e166362869056f032be423b"
source_url="https://github.com/bokuweb/ndlocr-lite-rs.git"
source_dir="${NDLOCR_SOURCE:-$root/target/ndlocr-web-src-$revision}"
if [ -z "${NDLOCR_SOURCE:-}" ]; then
  git_source() {
    env -u GIT_DIR -u GIT_WORK_TREE -u GIT_COMMON_DIR git -C "$source_dir" "$@"
  }
  mkdir -p "$source_dir"
  if [ ! -d "$source_dir/.git" ]; then
    git_source init --quiet
    git_source remote add origin "$source_url"
  fi
  # A restored checkout must not borrow the enclosing workspace or its origin.
  git_dir=$(git_source rev-parse --absolute-git-dir)
  expected_git_dir="$(cd "$source_dir" && pwd -P)/.git"
  if [ "$(cd "$git_dir" && pwd -P)" != "$expected_git_dir" ]; then
    echo "Browser OCR source is not an independent checkout: $source_dir" >&2
    exit 1
  fi
  git_source remote set-url origin "$source_url"
  # Configure patterns directly: sparse-checkout set can reject a cached or
  # unborn checkout on newer Git versions before the pinned commit is fetched.
  git_source config --worktree core.sparseCheckout true
  git_source config --worktree core.sparseCheckoutCone false
  git_dir=$(git_source rev-parse --absolute-git-dir)
  mkdir -p "$git_dir/info"
  printf '%s\n' '/Cargo.toml' '/src/' '/web/' '/tools/' '/examples/' '/benches/' '/docs/upstream-assets.json' > "$git_dir/info/sparse-checkout"
  if [ "$(git_source rev-parse HEAD 2>/dev/null || true)" != "$revision" ]; then
    # Skip the repository's large benchmark archives. Only package sources are needed.
    git_source config remote.origin.promisor true
    git_source config remote.origin.partialclonefilter blob:none
    printf 'Fetching browser OCR source %s from %s\n' "$revision" "$source_url"
    # The origin above is the declared URL, including after cache restoration.
    # A server can reject a reachable SHA that it did not advertise. Fetching
    # its advertised HEAD is transport only: the selected revision stays pinned.
    if git_source -c protocol.version=2 fetch --filter=blob:none --depth 1 origin "$revision"; then
      resolved=$(git_source rev-parse FETCH_HEAD)
    else
      git_source -c protocol.version=1 fetch --filter=blob:none --no-tags origin HEAD
      resolved=$(git_source rev-parse --verify "$revision^{commit}")
    fi
    if [ "$resolved" != "$revision" ]; then
      echo "Browser OCR source did not resolve to the pinned commit" >&2
      exit 1
    fi
    git_source checkout --detach "$revision"
  fi
fi
npm ci --prefix "$source_dir/web"
rustup target add wasm32-unknown-unknown
bash "$source_dir/tools/build_web.sh" "$root/figma/src/wasm/ndlocr"
if [ "${NDLOCR_FETCH_MODELS:-0}" = "1" ]; then
  python3 "$source_dir/tools/fetch_upstream_assets.py" "${NDLOCR_MODEL_DIR:-$root/target/ndlocr-models}"
fi
