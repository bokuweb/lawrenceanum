#!/usr/bin/env bash
# ローカルで生成した search.db を R2 にアップロードする (wrangler 経由)。
#
# search.db は大容量 (1GB 超) のため GitHub Pages に乗せず R2 に置く。
# ブラウザは sql.js-httpvfs で HTTP Range fetch するので公開バケットが必要。
#
# 必要な環境変数:
#   CLOUDFLARE_API_TOKEN   … "Workers R2 Storage: Edit" 権限のトークン
#   CLOUDFLARE_ACCOUNT_ID  … Cloudflare アカウント ID (32桁hex)
#   R2_BUCKET              … 例: lawrenceanum-search
#
# 使い方:
#   PUBLIC=/path/to/public ./scripts/upload-search-db.sh
#   (PUBLIC 省略時は ./public)
#
# search.db を先に生成するには (proceedings 込みで索引したい場合):
#   cargo build --release -p lawpub-cli
#   # public/laws と public/proceedings が揃っている状態で:
#   ./target/release/lawpub build-search-db --public "$PUBLIC"
set -euo pipefail

PUBLIC="${PUBLIC:-public}"
DB_FILE="$PUBLIC/search.db"
KEY="${SEARCH_DB_KEY:-search.db}"

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

if [ ! -f "$DB_FILE" ]; then
  echo "error: $DB_FILE not found" >&2
  echo "  先に lawpub build を実行してください:" >&2
  echo "    cargo build --release -p lawpub-cli" >&2
  echo "    ./target/release/lawpub build --public $PUBLIC" >&2
  exit 1
fi

ls -lh "$DB_FILE"

echo "uploading to r2://$R2_BUCKET/$KEY via wrangler ..."
( cd "$REPO" && $WRANGLER r2 object put "$R2_BUCKET/$KEY" \
    --file="$DB_FILE" \
    --content-type=application/x-sqlite3 \
    --remote )

echo "done: $DB_FILE -> r2://$R2_BUCKET/$KEY"
