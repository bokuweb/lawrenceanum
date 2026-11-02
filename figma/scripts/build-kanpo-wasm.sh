#!/usr/bin/env bash
# kanpo-parser を WASM にして figma/src/wasm/kanpo-parser/ に置く（官報 playground 用）。
# 引数で crate と置き場所を変えられる: `build-kanpo-wasm.sh kanpo-docx-wasm kanpo-docx`（官報 DOCX 変換用）。
#
# wasm-bindgen CLI は crate と同じ版でないと動かないので、Cargo.lock の版の CLI を
# 使う（手元に無ければ GitHub のリリースから target/ 配下に取ってくる）。
set -euo pipefail

root=$(cd "$(dirname "$0")/../.." && pwd)
crate="${1:-kanpo-parser-wasm}"
out="$root/figma/src/wasm/${2:-kanpo-parser}"
target_dir="${CARGO_TARGET_DIR:-$root/target}"
ver=$(awk '/^name = "wasm-bindgen"$/ { getline; gsub(/version = |"/, ""); print; exit }' "$root/Cargo.lock")

bindgen=wasm-bindgen
if ! command -v wasm-bindgen >/dev/null || [ "$(wasm-bindgen --version | awk '{print $2}')" != "$ver" ]; then
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) triple=x86_64-unknown-linux-musl ;;
    Linux-aarch64) triple=aarch64-unknown-linux-gnu ;;
    Darwin-arm64) triple=aarch64-apple-darwin ;;
    Darwin-x86_64) triple=x86_64-apple-darwin ;;
    *) echo "unsupported platform: $(uname -s)-$(uname -m)" >&2; exit 1 ;;
  esac
  dir="$target_dir/wasm-bindgen-$ver"
  if [ ! -x "$dir/wasm-bindgen" ]; then
    mkdir -p "$dir"
    curl -sSfL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$ver/wasm-bindgen-$ver-$triple.tar.gz" \
      | tar xz -C "$dir" --strip-components=1
  fi
  bindgen="$dir/wasm-bindgen"
fi

rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
cargo build --release --target wasm32-unknown-unknown -p "$crate" --manifest-path "$root/Cargo.toml"
rm -rf "$out"
"$bindgen" --target web --out-dir "$out" "$target_dir/wasm32-unknown-unknown/release/${crate//-/_}.wasm"
echo "wrote $out"
