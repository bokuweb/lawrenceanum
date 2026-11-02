#!/usr/bin/env bash
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
revision=a0647cc6542cc14a922a5d67c9997a588efb77b9
source_dir="${JADE_CORE_SOURCE:-$root/.cache/jade-core-$revision}"
out="$root/figma/src/wasm/jade-core"
if [ -z "${JADE_CORE_SOURCE:-}" ]; then
  mkdir -p "$source_dir"
  if [ ! -d "$source_dir/.git" ]; then
    git -C "$source_dir" init --quiet
    git -C "$source_dir" remote add origin https://github.com/jade-rs/jade-core.git
  fi
  git -C "$source_dir" sparse-checkout set --no-cone '/Cargo.toml' '/Cargo.lock' '/crates/' '/fonts/' '/third_party/' '/scripts/optimize-wasm-js.mjs' '/LICENSE' '/THIRD_PARTY_LICENSES.html'
  if [ "$(git -C "$source_dir" rev-parse HEAD 2>/dev/null || true)" != "$revision" ]; then
    git -C "$source_dir" config remote.origin.promisor true
    git -C "$source_dir" fetch --filter=blob:none --depth 1 origin "$revision"
    git -C "$source_dir" checkout --detach FETCH_HEAD
  fi
fi
version=$(awk '/^name = "wasm-bindgen"$/ {getline; gsub(/version = |"/, "");print;exit}' "$source_dir/Cargo.lock")
target_dir="${CARGO_TARGET_DIR:-$source_dir/target}"
bindgen=wasm-bindgen
if ! command -v wasm-bindgen >/dev/null || [ "$(wasm-bindgen --version)" != "wasm-bindgen $version" ]; then
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64) triple=x86_64-unknown-linux-musl ;;
    Linux-aarch64) triple=aarch64-unknown-linux-gnu ;;
    Darwin-arm64) triple=aarch64-apple-darwin ;;
    Darwin-x86_64) triple=x86_64-apple-darwin ;;
    *) echo 'Install the matching wasm-bindgen CLI for your platform' >&2;exit 1 ;;
  esac
  directory="$root/target/wasm-bindgen-$version"
  if [ ! -x "$directory/wasm-bindgen" ]; then
    mkdir -p "$directory"
    curl -fsSL "https://github.com/wasm-bindgen/wasm-bindgen/releases/download/$version/wasm-bindgen-$version-$triple.tar.gz" | tar xz -C "$directory" --strip-components=1
  fi
  bindgen="$directory/wasm-bindgen"
fi
rustup toolchain install 1.93.1 --profile minimal --target wasm32-unknown-unknown --component llvm-tools-preview
sysroot=$(rustup run 1.93.1 rustc --print sysroot)
host=$(rustup run 1.93.1 rustc -vV | awk '/^host:/ {print $2}')
# The host ar may omit WebAssembly symbols; use the matching LLVM archiver.
export AR_wasm32_unknown_unknown="$sysroot/lib/rustlib/$host/bin/llvm-ar"
cargo +1.93.1 build --locked --manifest-path "$source_dir/Cargo.toml" -p jade-docx-editor --release --target wasm32-unknown-unknown
mkdir -p "$out"
"$bindgen" --target web --out-name jade_docx --out-dir "$out" "$target_dir/wasm32-unknown-unknown/release/jade_docx_editor.wasm"
node "$source_dir/scripts/optimize-wasm-js.mjs" "$out/jade_docx.js"
node --input-type=module - "$out/jade_docx_bg.wasm" <<'JS'
import {readFileSync} from 'node:fs';
const module=new WebAssembly.Module(readFileSync(process.argv[2]));
const unresolved=WebAssembly.Module.imports(module).filter(item=>item.module==='env');
if(unresolved.length) throw new Error(`Unresolved native WASM imports: ${unresolved.map(item=>item.name).join(', ')}. Rebuild ring with the configured LLVM archiver.`);
JS
cp "$source_dir/fonts/Noto_Sans_JP/static/NotoSansJP-Regular.ttf" "$out/"
cp "$source_dir/fonts/Noto_Sans_JP/OFL.txt" "$out/NotoSansJP-OFL.txt"
cp "$source_dir/LICENSE" "$out/jade-core-LICENSE.txt"
cp "$source_dir/THIRD_PARTY_LICENSES.html" "$out/jade-core-THIRD_PARTY-LICENSES.html"
echo "JADE Core preview built: $out"
