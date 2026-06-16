#!/usr/bin/env bash
# ローカルの国会会議録キャッシュ (.cache/proceedings/{session}/*.json, 生 kokkai JSON) を
# zstd 圧縮 tar (proceedings-cache.tar.zst) にまとめて R2 にアップロードする。
#
# 議事録は法令ワークフロー (update-law-data.yml「Build proceedings from R2 cache」) が
# R2 のこのキーを取り込み、proceedings-build-json → link → laws と同じ public/ に同梱
# して配信する。corpus workflow (update-corpus-data.yml) が定期的に同じ更新をするが、
# 初回シードや手元での作り直し時はこのスクリプトを使う。
#
# 事前に会期を取得しておくこと:
#   cargo build --release -p lawpub-cli
#   ./target/release/lawpub proceedings-fetch --session 217 --cache .cache
#   (必要な会期ぶん繰り返す)
#
# 必要な環境変数:
#   CLOUDFLARE_API_TOKEN   … "Workers R2 Storage: Edit" 権限のトークン
#   CLOUDFLARE_ACCOUNT_ID  … Cloudflare アカウント ID (32桁hex)
#   R2_BUCKET              … 例: lawrenceanum-search
#
# 使い方:
#   CACHE=/path/to/.cache ./scripts/upload-proceedings-cache.sh
#   (CACHE 省略時は ./.cache)
set -euo pipefail

CACHE="${CACHE:-.cache}"
PROC="$CACHE/proceedings"
KEY="${PROCEEDINGS_CACHE_KEY:-proceedings-cache.tar.zst}"
ARCHIVE="${ARCHIVE:-/tmp/proceedings-cache.tar.zst}"

: "${CLOUDFLARE_API_TOKEN:?set CLOUDFLARE_API_TOKEN (Workers R2 Storage: Edit)}"
: "${CLOUDFLARE_ACCOUNT_ID:?set CLOUDFLARE_ACCOUNT_ID}"
: "${R2_BUCKET:?set R2_BUCKET}"

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${WRANGLER:-}" ]; then
  if [ -x "$REPO/node_modules/.bin/wrangler" ]; then
    WRANGLER="$REPO/node_modules/.bin/wrangler"
  else
    WRANGLER="npx --yes wrangler"
  fi
fi

if [ ! -d "$PROC" ]; then
  echo "error: $PROC not found" >&2
  echo "  先に会期を取得してください: lawpub proceedings-fetch --session N --cache $CACHE" >&2
  exit 1
fi

count=$(find "$PROC" -name '*.json' | wc -l | tr -d ' ')
if [ "$count" = "0" ]; then
  echo "error: no meeting JSON under $PROC" >&2
  exit 1
fi

# top-level を ./{session}/ にするため proceedings ディレクトリ内から tar 化する
# (CI 側は `tar -x -C .cache/proceedings` で戻す)。
echo "packing $count meeting files from $PROC ..."
( cd "$PROC" && tar -cf - . | zstd -19 --long -T0 -o "$ARCHIVE" -f )
ls -lh "$ARCHIVE"

echo "uploading to r2://$R2_BUCKET/$KEY via wrangler ..."
( cd "$REPO" && $WRANGLER r2 object put "$R2_BUCKET/$KEY" \
    --file="$ARCHIVE" \
    --content-type=application/zstd \
    --remote )

echo "done: $count meeting files -> r2://$R2_BUCKET/$KEY"
