#!/usr/bin/env bash
# ローカルの法令コーパス本体 (.cache/revisions/{law_id}/*.xml, 全版 ~32GB) を
# zstd 圧縮 tar (revisions.tar.zst, ~160MB) にまとめて R2 にアップロードする。
#
# このコーパスは build-json が public/laws/index.json を作り直す唯一の source。
# 32GB あり GH Actions cache (10GB 上限) には収まらないので、cache が evict された
# run では当日 fetch 分だけになり法令数が崩壊する (9000→数十)。R2 を真の
# source of truth にし、CI (update-law-data.yml「Restore revisions corpus from R2」)
# が毎回 top-up することで cache の状態に依存せず full を保つ。
#
# コーパスを作り直したら (フル cache を持つ端末で fetch-revisions / fetch-bulk 後)
# このスクリプトで R2 を最新化する。CI の取得は aws s3 (S3 互換 endpoint) だが、
# 同じ R2 バケットの同じキーを読み書きするので混在して問題ない。
#
# 必要な環境変数:
#   CLOUDFLARE_API_TOKEN   … "Workers R2 Storage: Edit" 権限のトークン
#   CLOUDFLARE_ACCOUNT_ID  … Cloudflare アカウント ID (32桁hex)
#   R2_BUCKET              … 例: lawrenceanum-search
#
# 使い方:
#   CACHE=/path/to/.cache ./scripts/upload-revisions-corpus.sh
#   (CACHE 省略時は ./.cache)
set -euo pipefail

CACHE="${CACHE:-.cache}"
REVISIONS="$CACHE/revisions"
KEY="${REVISIONS_CORPUS_KEY:-revisions.tar.zst}"
ARCHIVE="${ARCHIVE:-/tmp/revisions.tar.zst}"

: "${CLOUDFLARE_API_TOKEN:?set CLOUDFLARE_API_TOKEN (Workers R2 Storage: Edit)}"
: "${CLOUDFLARE_ACCOUNT_ID:?set CLOUDFLARE_ACCOUNT_ID}"
: "${R2_BUCKET:?set R2_BUCKET}"

# wrangler はこのリポジトリの devDependency (要 Node v22+)。スクリプト位置から
# リポジトリ root を求め、ローカル bin を優先して解決する。
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ -z "${WRANGLER:-}" ]; then
  if [ -x "$REPO/node_modules/.bin/wrangler" ]; then
    WRANGLER="$REPO/node_modules/.bin/wrangler"
  else
    WRANGLER="npx --yes wrangler"
  fi
fi

if [ ! -d "$REVISIONS" ]; then
  echo "error: $REVISIONS not found" >&2
  echo "  先にフル cache を用意してください (lawpub fetch-revisions / fetch-bulk)。" >&2
  exit 1
fi

count=$(find "$REVISIONS" -maxdepth 1 -mindepth 1 -type d | wc -l | tr -d ' ')
if [ "$count" = "0" ]; then
  echo "error: no law dirs under $REVISIONS" >&2
  exit 1
fi

# top-level を ./{law_id}/ にするため revisions ディレクトリ内から tar 化する
# (CI 側は `tar -x -C .cache/revisions` で戻す)。中身は XML なので zstd で圧縮。
echo "packing $count laws from $REVISIONS (32GB 規模 — 数分かかります) ..."
( cd "$REVISIONS" && tar -cf - . | zstd -19 --long -T0 -o "$ARCHIVE" -f )
ls -lh "$ARCHIVE"

echo "uploading to r2://$R2_BUCKET/$KEY via wrangler ..."
( cd "$REPO" && $WRANGLER r2 object put "$R2_BUCKET/$KEY" \
    --file="$ARCHIVE" \
    --content-type=application/zstd \
    --remote )

echo "done: $count laws -> r2://$R2_BUCKET/$KEY"
