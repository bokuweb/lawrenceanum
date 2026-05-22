# lawrenceanum

A static-hosted JSON API + WASM-SQLite-powered SPA for Japanese statute data
(法令), built on top of e-Gov 法令API. GitHub Actions periodically pulls the
upstream data, the Rust CLI (`lawpub`) normalizes it into stable JSON, and the
result is served from GitHub Pages.

Detailed design: [docs/plan.md](docs/plan.md).

## What you get

- Static JSON API at `https://<owner>.github.io/<repo>/...`
  - `index.json`, `manifest.json`, `health.json`
  - `laws/index.json`, `laws/{law_id}/{current,versions,timeline}.json`
  - `laws/{law_id}/revisions/{rev_id}.json`, `laws/{law_id}/articles/{art_id}.json`
  - `updates/latest.json`, `updates/{YYYY-MM-DD}.json`
  - `kanpo/{YYYY-MM-DD}/index.json`
  - `sitemap.xml`, `robots.txt`, `laws/all.ndjson`
- A React SPA at the same origin that consumes those JSON files
  - HashRouter, deep-linkable to any law / article / version
  - Browser-side full-text search via **WASM SQLite (sql.js) + FTS5**
  - Cross-reference graph: `第○条` → article links, backlinks panel,
    cross-law jumps for `民法第七百九条` style references

## Local quickstart

```bash
cargo build --release -p lawpub-cli
./target/release/lawpub update --public public --cache .cache --provider mock
./target/release/lawpub validate --public public

# Serve the SPA on top
cd figma && pnpm install
pnpm dev          # http://localhost:5173/   (a custom Vite middleware
                  # serves ../public/*.json so the SPA reads live JSON)
# or production build:
pnpm build        # writes index.html + assets/ next to the JSON
```

Generated files under `public/`:

```
public/
├── index.json / manifest.json / health.json / sitemap.xml / robots.txt
├── laws/
│   ├── index.json
│   ├── all.ndjson
│   └── {law_id}/
│       ├── current.json
│       ├── versions.json
│       ├── timeline.json
│       ├── revisions/{rev_id}.json
│       └── articles/{art_id}.json
├── updates/{latest.json,{YYYY-MM-DD}.json}
├── kanpo/{YYYY-MM-DD}/index.json
├── schema/{law-document,manifest,updates}.json
├── search.db                                  # SQLite + FTS5
├── index.html / assets/                       # SPA build output
state/latest.json                              # cron-managed pointer
```

## CLI surface

```text
lawpub update         --public public --cache .cache [--provider http|mock] [--date YYYY-MM-DD] [--force]
lawpub fetch-update   --date YYYY-MM-DD --cache .cache
lawpub fetch-range    --from YYYY-MM-DD --to YYYY-MM-DD --cache .cache [--provider http|mock]
lawpub fetch-bulk     --category N [--limit M] --cache .cache [--provider http|mock]
lawpub build-json     --input .cache --output public
lawpub build-index    --output public
lawpub kanpo-fetch    --date YYYY-MM-DD --cache .cache
lawpub kanpo-link     --output public
lawpub validate       --public public
lawpub status         --public public --cache .cache
```

The provider defaults to `http` and uses `https://laws.e-gov.go.jp/api/1` (v1
API; v2 has a different path scheme — `/api/2/laws`, `/api/2/law_data/{id}`).
Override with `LAWPUB_PROVIDER` and `LAWPUB_EGOV_BASE_URL`.

## Workspace layout

| crate | purpose |
|---|---|
| `crates/egov-client`     | e-Gov fetcher (`HttpProvider`, `MockProvider`) |
| `crates/law-normalizer`  | LawXML → normalized `LawDocument` |
| `crates/kanpo-client`    | 官報 site scraper (Phase 3, mock for now) |
| `crates/kanpo-linker`    | amendment ↔ 官報 PDF matching with confidence score |
| `crates/search-index`    | bigram tokenizer + SQLite FTS5 builder + ref-graph extractor |
| `crates/lawpub-cli`      | the `lawpub` binary |

## Browser search (WASM SQLite + FTS5 over Cloudflare R2)

`lawpub` emits `public/search.db` (SQLite + FTS5, ~1.5 GB at full bulk) at
build time. The SPA reads it through **sql.js-httpvfs** (sqlite.org's
Emscripten WASM build + an HTTP-Range VFS). Each query downloads only the
SQLite **pages** (4 KB) it needs — typically 100-300 KB / query — so the
1.5 GB DB stays remote.

Hosting options:

| Option | search.db location | When to use |
|---|---|---|
| **GitHub Pages only (default)** | `public/search.db` (same origin) | OK for tiny demos (<50 MB), hard limit 100 MB git |
| **Cloudflare R2 (recommended)**  | `https://<r2-pub>/search.db` via `VITE_SEARCH_DB_URL` | Production / full bulk. R2 free tier (10 GB storage + free egress) covers personal use indefinitely |
| Turso / D1                       | Their HTTP API | Only if edge-replicated reads matter |

### R2 setup (one-time)

1. Sign up for Cloudflare (free). R2 dashboard → **Create bucket** (e.g.
   `lawrenceanum`).
2. Bucket settings → **Public access** → enable "r2.dev subdomain". Note the
   public URL `https://pub-<hash>.r2.dev`.
3. Bucket settings → **CORS policy** → allow your Pages origin:

   ```json
   [
     {
       "AllowedOrigins": ["https://<owner>.github.io"],
       "AllowedMethods": ["GET"],
       "AllowedHeaders": ["range", "if-match", "if-none-match"],
       "ExposeHeaders": ["content-length", "content-range", "etag"],
       "MaxAgeSeconds": 86400
     }
   ]
   ```

4. R2 → **Manage R2 API tokens** → create token with **Object Read & Write**
   on that single bucket.
5. GitHub repo → Settings → Secrets and variables → Actions → add:

   | Secret | Example |
   |---|---|
   | `R2_ACCOUNT_ID`       | your account id |
   | `R2_ACCESS_KEY_ID`    | from step 4 |
   | `R2_SECRET_ACCESS_KEY`| from step 4 |
   | `R2_BUCKET`           | `lawrenceanum` |
   | `R2_ENDPOINT`         | `https://<R2_ACCOUNT_ID>.r2.cloudflarestorage.com` |
   | `R2_PUBLIC_URL`       | `https://pub-<hash>.r2.dev` |

When all of `R2_BUCKET` / `R2_ENDPOINT` are set, the workflow uploads
`search.db` to R2 after `validate`, removes it from the Pages artifact, and
builds the SPA with `VITE_SEARCH_DB_URL=$R2_PUBLIC_URL/search.db`. With the
secrets unset, everything still works (search.db stays in `public/`).

- Indexed at the article level. The FTS5 virtual table has columns
  `law_id` / `article_id` / `article_no` / `caption` / `title_tokens` /
  `content_tokens`.
- Japanese is pre-tokenized as **character bigrams**
  (`crates/search-index::tokenize` and
  `figma/src/app/data/search-engine::tokenize` are kept in lockstep).
- Queries go through the same bigram tokenizer; FTS5 `snippet()` produces
  highlighted excerpts.
- A `meta` table stores `built_at` / `law_count` / `article_count` /
  `ref_count`.
- A `refs` table stores cross-references between articles:

  ```sql
  CREATE TABLE refs (
    from_law_id TEXT, from_article_id TEXT,
    to_law_id   TEXT, to_article_id   TEXT,
    ref_text TEXT,
    ref_type TEXT  -- 'self_article' | 'previous_article' | 'next_article' | 'cross_law'
  );
  ```

  Extraction uses Aho-Corasick (`MatchKind::LeftmostLongest`) to keep build
  time linear in body length × match count even with thousands of laws.

The browser exposes `getOutgoingRefs` / `getIncomingRefs` / `getRefsForLaw` and
the Browse detail view linkifies article text in place. Clicking a reference
scrolls to `#article_id`; cross-law references navigate to
`/laws/{other_id}#{article_id}`. Each article header also lists incoming
references as backlinks.

`/search` lazy-loads sql-wasm (~320KB gzip) + `search.db` on first navigation;
falling back to a mock filter when the DB is unreachable so local dev still
works.

Inspired by ellisii's [`jp-tokenizer-bigram`](../ellisii/crates/jp-tokenizer-bigram/)
and [`store-sqlite`](../ellisii/crates/store-sqlite/).

## Web UI (static SPA)

`figma/` doubles as the design source-of-truth and the actual UI implementation
(Vite + React + Tailwind v4 + shadcn/ui). It builds straight into the same
`public/` directory the JSON lives in.

- `base: './'` so assets are relative — works on any GitHub Pages sub-path
- `outDir: ../public`, `emptyOutDir: false` so the JSON survives a Vite build
- `publicDir: false` to avoid copying assets into themselves
- Dev mode: `lawpubJsonDevServer` Vite middleware serves `../public/*.json`
  on the fly so `pnpm dev` sees live data without a separate server
- Lazy-loaded chart bundle (recharts ≈ 420 KB) via `React.lazy`, kept out of
  the initial dashboard render

### CI step order

1. `lawpub update` writes JSON via atomic `public.tmp/` → rename
2. `lawpub kanpo-link` overlays 官報 matches on each `timeline.json`,
   recomputes `manifest.json`
3. **Change detection**: read `state/last_run.json.changed`; if `false`, skip
   the rest
4. `pnpm build` adds `index.html` + `assets/` to `public/` (JSON untouched)
5. `lawpub validate` cross-checks every manifest entry's sha256
6. `actions/configure-pages` → `actions/upload-pages-artifact`
7. `git commit && git push` (`public/` plus `state/latest.json`)
8. Separate `deploy` job runs `actions/deploy-pages`

## Auto-update via GitHub Actions

`update-law-data.yml` is driven by three triggers:

| Trigger | Behaviour |
|---|---|
| `schedule` (JST 06:30 / 12:30 / 18:30 / 00:30) | Pull latest e-Gov diff, commit + deploy if anything changed |
| `push` (merge to `main`) | Rebuild SPA over the existing committed `public/` and redeploy. **No** e-Gov fetch, **no** auto-commit |
| `workflow_dispatch` | Pick `provider` / `date` / `force` / `from_date` / `to_date` / `bulk_category` / `bulk_limit` |

Auto-commits use `GITHUB_TOKEN`, which by GitHub policy does not re-trigger
workflows — so a cron auto-commit cannot create a deploy loop.

### Change detection (no-op suppression)

`lawpub update` writes `state/last_run.json` (gitignored) on every run:

```json
{
  "version": 1,
  "ran_at": "2026-05-09T03:30:00Z",
  "provider": "http",
  "dates": ["2026-05-06", "2026-05-07", "2026-05-08", "2026-05-09"],
  "new_xmls": 0,
  "errors": [],
  "changed": false
}
```

If the sha256-deduped revision store (`.cache/revisions/`) gained no new XMLs
**and** `public/manifest.json` already exists, the run reports `changed=false`
and every downstream step (build / commit / deploy) is skipped. So idle hours
on the e-Gov side do not bloat git history.

### Failure handling

- HttpProvider retries each request three times with exponential backoff. A
  failed date is logged in `errors` and other dates keep going (plan §14).
- `public/` is replaced atomically via `public.tmp/` → `public.bak/` →
  rename. A failure mid-swap is rolled back from the backup.
- `concurrency: update-law-json` serializes overlapping schedule + dispatch
  runs.

### Manual triggers

```bash
# Single date (overrides the auto state-based range)
gh workflow run update-law-data.yml -f date=2026-05-01

# Range backfill (fill in dates before cron started)
gh workflow run update-law-data.yml \
  -f from_date=2024-04-01 -f to_date=2026-05-09

# Bulk fetch (one-shot collection of every law in a category)
#   1 = 憲法・法律
#   2 = 政令・勅令
#   3 = 府省令・規則
gh workflow run update-law-data.yml -f bulk_category=1
gh workflow run update-law-data.yml -f bulk_category=2 -f bulk_limit=500

# Force a redeploy without touching e-Gov
gh workflow run update-law-data.yml -f force=true
```

### One-time amendment-history backfill (e-Gov API v2)

`/api/1/lawdata/{id}` (currently used by bulk/cron) only returns the law's *current*
snapshot — no historical revisions. To populate the timeline with actual amendment
history (e.g. 民法 has ~33 revisions back to Heisei era), we use e-Gov API v2's
`/law_revisions/{id}` endpoint. This is a one-time backfill done **locally** (not in
Actions) because it makes ~9000 requests and would be slow / risky in CI.

```bash
# 1. Smoke-test on a few laws first. ID 源は public/laws/index.json (auto-committed
#    by Actions) なので fresh checkout でも .cache 不要で回せる。
./target/release/lawpub fetch-revisions --from-public ./public --limit 5

# 2. Full backfill. Concurrency 2 is e-Gov-friendly (CloudFront rate-limits at ~4+).
#    Resumes if interrupted; existing per-law JSONs are skipped (use --force to redo).
./target/release/lawpub fetch-revisions --from-public ./public --concurrency 2

#    Alternative: when .cache/revisions/ is already populated locally:
# ./target/release/lawpub fetch-revisions --all --concurrency 2

# 3. Pack the per-law JSONs into a single jsonl for shipping.
./target/release/lawpub bundle-revisions-meta --mode pack \
  --dir .cache/revisions_meta --file .cache/revisions_meta.jsonl

# 4. Upload to R2 via wrangler (uses your `wrangler login` session — no R2
#    access key needed locally). CI's "Restore revisions_meta from R2" step
#    later pulls the same object back via the S3 API.
export R2_BUCKET=<bucket>
pnpm install               # installs wrangler (root devDependency)
pnpm upload-revisions-meta # = wrangler r2 object put "$R2_BUCKET/revisions_meta.jsonl" ...

# 5. Trigger a force rebuild so build-json picks up the new meta.
gh workflow run "Update law JSON" -f force=true
```

The upload uses `wrangler` (a root `devDependency`); `pnpm upload-revisions-meta`
wraps `wrangler r2 object put ... --remote`. CI reads the object back with the
S3 API + `R2_*` secrets — upload and download paths differ but hit the same
R2 object.

After this, the cron path (`lawpub update`) refreshes the meta for *only* the
laws updated that day, so the timeline stays fresh without re-running the
full backfill.

Priority is `bulk_category > from_date/to_date > date > automatic state-based`.
Bulk runs do thousands of requests × 200 ms throttle, so the workflow's
`timeout-minutes` is 360. If a bulk run dies partway through, the in-job
`.cache/revisions/` still holds whatever it managed to fetch and `build-json`
will produce a partial `public/`.

## Status

Up and running on Pages. The cron is incremental from the moment it starts;
historical revisions only accumulate going forward unless you explicitly
backfill via `bulk_category=N` or `from_date=…/to_date=…`. There is no e-Gov
endpoint that returns historical revisions of a single law (only the current
version + a daily-update list), so deeper history requires the daily snapshots
to keep stacking up over time.
