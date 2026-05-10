/**
 * sql.js-httpvfs (WASM SQLite + HTTP Range) によるブラウザ内検索。
 *
 * - search.db は Cloudflare R2 に置く想定 (`VITE_SEARCH_DB_URL` で URL 指定)。
 * - sql.js-httpvfs が SQLite ページ (4KB) を Range fetch するので、1.5GB DB
 *   でも 1 query ≒ 100〜300KB しか DL しない。
 * - VITE_SEARCH_DB_URL 未設定時は同 origin の `./search.db` を使う (小規模時 fallback)。
 *
 * Rust 側の `crates/search-index::tokenize` と完全一致した bigram 分割を
 * クエリにも適用する。
 */

import { createDbWorker, type WorkerHttpvfs } from "sql.js-httpvfs";

export type SearchHit = {
  law_id: string;
  law_num: string | null;
  title: string;
  article_id: string;
  article_no: string;
  caption: string;
  snippet: string;
};

export type ArticleRef = {
  from_law_id: string;
  from_article_id: string;
  to_law_id: string;
  to_article_id: string | null;
  ref_text: string;
  ref_type: string;
};

function isCjk(c: string): boolean {
  const code = c.codePointAt(0);
  if (code === undefined) return false;
  return (
    (code >= 0x3040 && code <= 0x309f) ||
    (code >= 0x30a0 && code <= 0x30ff) ||
    (code >= 0x31f0 && code <= 0x31ff) ||
    (code >= 0x3400 && code <= 0x4dbf) ||
    (code >= 0x4e00 && code <= 0x9fff) ||
    (code >= 0xf900 && code <= 0xfaff) ||
    (code >= 0xff66 && code <= 0xff9d)
  );
}

function isWordChar(c: string): boolean {
  return /\p{L}|\p{N}/u.test(c) || isCjk(c);
}

export function tokenize(text: string): string[] {
  const out: string[] = [];
  let buf = "";
  let bufIsCjk = false;
  const flush = () => {
    if (!buf) return;
    if (bufIsCjk) {
      const chars = Array.from(buf);
      if (chars.length === 1) {
        out.push(chars[0]);
      } else {
        for (let i = 0; i < chars.length - 1; i++) out.push(chars[i] + chars[i + 1]);
      }
    } else {
      out.push(buf.toLowerCase());
    }
    buf = "";
  };
  for (const c of text) {
    if (!isWordChar(c)) {
      flush();
      continue;
    }
    const curIsCjk = isCjk(c);
    if (buf && curIsCjk !== bufIsCjk) flush();
    buf += c;
    bufIsCjk = curIsCjk;
  }
  flush();
  return out;
}

export function tokenizeForFts(text: string): string {
  return tokenize(text).join(" ");
}

let workerPromise: Promise<WorkerHttpvfs | null> | null = null;

async function loadWorker(): Promise<WorkerHttpvfs | null> {
  if (!workerPromise) {
    workerPromise = (async () => {
      try {
        // wasm/worker は sql.js-httpvfs の dist を Vite が ?url で解決 → 同一 host bundle に含める。
        // search.db は VITE_SEARCH_DB_URL (R2 等) を優先、未設定なら同 origin の ./search.db。
        const wasmUrl = (await import("sql.js-httpvfs/dist/sql-wasm.wasm?url")).default;
        const workerUrl = (await import("sql.js-httpvfs/dist/sqlite.worker.js?url")).default;
        const dbUrl =
          (import.meta as any).env?.VITE_SEARCH_DB_URL ||
          new URL("./search.db", document.baseURI).toString();
        const worker = await createDbWorker(
          [
            {
              from: "inline",
              config: {
                serverMode: "full",
                requestChunkSize: 4096,
                url: dbUrl,
              },
            },
          ],
          workerUrl,
          wasmUrl,
        );
        return worker;
      } catch (e) {
        console.warn("[search] httpvfs init failed", e);
        return null;
      }
    })();
  }
  return workerPromise;
}

export async function isAvailable(): Promise<boolean> {
  return (await loadWorker()) !== null;
}

async function exec<T = Record<string, unknown>>(sql: string, params: unknown[] = []): Promise<T[]> {
  const w = await loadWorker();
  if (!w) return [];
  // db.query は SqliteWorker 経由で `(sql, ...params) => row[]` の signature。
  return (await (w.db.query as any)(sql, ...params)) as T[];
}

export async function search(q: string, limit = 50): Promise<SearchHit[]> {
  const tokens = tokenizeForFts(q.trim());
  if (!tokens) return [];
  const rows = await exec<{
    law_id: string; article_id: string; article_no: string; caption: string;
    title: string; law_num: string | null; snippet: string;
  }>(
    `SELECT s.law_id, s.article_id, s.article_no, s.caption,
            l.title, l.law_num,
            snippet(search_fts, 5, '<mark>', '</mark>', '...', 16) AS snippet
       FROM search_fts s
       JOIN laws l ON l.law_id = s.law_id
      WHERE search_fts MATCH ?
      ORDER BY rank
      LIMIT ?`,
    [tokens, limit],
  );
  return rows.map(r => ({
    law_id: String(r.law_id ?? ""),
    law_num: r.law_num ?? null,
    title: String(r.title ?? ""),
    article_id: String(r.article_id ?? ""),
    article_no: String(r.article_no ?? ""),
    caption: String(r.caption ?? ""),
    snippet: String(r.snippet ?? ""),
  }));
}

export async function getOutgoingRefs(lawId: string, articleId: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = ? AND from_article_id = ? ORDER BY id`,
    [lawId, articleId],
  );
}

export async function getIncomingRefs(lawId: string, articleId: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE to_law_id = ? AND to_article_id = ? ORDER BY id`,
    [lawId, articleId],
  );
}

export async function getRefsForLaw(lawId: string): Promise<ArticleRef[]> {
  return exec<ArticleRef>(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = ? OR to_law_id = ? ORDER BY id`,
    [lawId, lawId],
  );
}

export async function getMeta(): Promise<Record<string, string> | null> {
  const w = await loadWorker();
  if (!w) return null;
  const rows = await (w.db.query as any)(`SELECT key, value FROM meta`);
  const meta: Record<string, string> = {};
  for (const r of rows as { key: string; value: string }[]) meta[r.key] = r.value;
  return meta;
}
