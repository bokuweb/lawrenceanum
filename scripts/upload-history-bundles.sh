#!/usr/bin/env bash
# ローカルで生成した履歴束 (public/laws/*/history.ndjson.zst, 全 ~92MB) を
# 1 つの tar にまとめて R2 にアップロードする (wrangler 経由)。
#
# 履歴の全件ビルドには 32GB の revision キャッシュが必要で CI では回せないため、
# 手元 (フル cache を持つ端末) で生成した束を R2 prebuilt として置き、
# CI (update-law-data.yml) が取得して Pages に同梱する。
#
# 必要な環境変数:
#   CLOUDFLARE_API_TOKEN   … "Workers R2 Storage: Edit" 権限のトークン
#   CLOUDFLARE_ACCOUNT_ID  … Cloudflare アカウント ID (32桁hex)
#   R2_BUCKET              … 例: lawrenceanum-search
#
# 使い方:
#   PUBLIC=/path/to/public ./scripts/upload-history-bundles.sh
#   (PUBLIC 省略時は ./public)
#
# 注: アップロードは wrangler、CI 側の取得は aws s3 (S3 互換 endpoint) だが、
#     同じ R2 バケットの同じキーを読み書きするので混在して問題ない。
set -euo pipefail

PUBLIC="${PUBLIC:-public}"
KEY="${HISTORY_BUNDLE_KEY:-history-bundles.tar}"
TARFILE="${TARFILE:-/tmp/history-bundles.tar}"

: "${CLOUDFLARE_API_TOKEN:?set CLOUDFLARE_API_TOKEN (Workers R2 Storage: Edit)}"
: "${CLOUDFLARE_ACCOUNT_ID:?set CLOUDFLARE_ACCOUNT_ID}"
: "${R2_BUCKET:?set R2_BUCKET}"

# wrangler はこのリポジトリの devDependency (要 Node v22+)。スクリプト位置から
# リポジトリ root を求め、ローカル bin を優先して解決する (PUBLIC が repo 外でも動く)。
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${WRANGLER:-}" ]; then
  if [ -x "$REPO/node_modules/.bin/wrangler" ]; then
    WRANGLER="$REPO/node_modules/.bin/wrangler"
  else
    WRANGLER="npx --yes wrangler"
  fi
fi

if [ ! -d "$PUBLIC/laws" ]; then
  echo "error: $PUBLIC/laws not found" >&2
  exit 1
fi

# laws/{id}/history.ndjson.zst を public 相対パスで tar 化する。
# CI 側は `tar -xf history-bundles.tar -C public` で laws/.../*.zst に戻せる。
count=$(cd "$PUBLIC" && find laws -name 'history.ndjson.zst' | wc -l | tr -d ' ')
if [ "$count" = "0" ]; then
  echo "error: no history.ndjson.zst under $PUBLIC/laws" >&2
  exit 1
fi
echo "packing $count history bundles from $PUBLIC ..."
( cd "$PUBLIC" && find laws -name 'history.ndjson.zst' -print0 | sort -z | tar --null -cf "$TARFILE" -T - )
ls -lh "$TARFILE"

echo "uploading to r2://$R2_BUCKET/$KEY via wrangler ..."
( cd "$REPO" && $WRANGLER r2 object put "$R2_BUCKET/$KEY" \
    --file="$TARFILE" \
    --content-type=application/x-tar \
    --remote )

echo "done: $count bundles -> r2://$R2_BUCKET/$KEY"
