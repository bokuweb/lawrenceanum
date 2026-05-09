/**
 * sql.js + FTS5 によるブラウザ内検索。
 *
 * `lawpub` が生成する `public/search.db` を一度だけ取得して、SQL.js (sqlite-wasm)
 * で開く。クエリは Rust 側と同じ bigram トークナイズで前段分割し、FTS5 へ流す。
 *
 * トークナイザ実装は `crates/search-index/src/lib.rs` の `tokenize` と一致させる。
 */

import initSqlJs, { type Database } from "sql.js";
// Vite の ?url import で sql.js のビルド済 wasm を相対パスで埋め込む。
// 出力は assets/sql-wasm-{hash}.wasm として bundle に同梱され、Pages 配信下で動く。
import sqlWasmUrl from "sql.js/dist/sql-wasm.wasm?url";

export type SearchHit = {
  law_id: string;
  law_num: string | null;
  title: string;
  article_id: string;
  article_no: string;
  caption: string;
  snippet: string;
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

/** Rust の `crates/search-index::tokenize` と完全に同じ動作を保つ。 */
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
        for (let i = 0; i < chars.length - 1; i++) {
          out.push(chars[i] + chars[i + 1]);
        }
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

let dbPromise: Promise<Database | null> | null = null;

async function loadDb(): Promise<Database | null> {
  if (!dbPromise) {
    dbPromise = (async () => {
      try {
        const SQL = await initSqlJs({ locateFile: () => sqlWasmUrl });
        const url = new URL("./search.db", document.baseURI).toString();
        const res = await fetch(url, { cache: "no-cache" });
        if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
        const buf = new Uint8Array(await res.arrayBuffer());
        return new SQL.Database(buf);
      } catch (e) {
        console.warn("[search] failed to load search.db", e);
        return null;
      }
    })();
  }
  return dbPromise;
}

export async function isAvailable(): Promise<boolean> {
  return (await loadDb()) !== null;
}

export async function search(q: string, limit = 50): Promise<SearchHit[]> {
  const db = await loadDb();
  if (!db) return [];
  const tokens = tokenizeForFts(q.trim());
  if (!tokens) return [];

  // FTS5: 空白区切りのトークンは AND 結合される。bigram は word_char 化の段階で
  // FTS5 メタ文字 (",-,",")) を含まないので素直に渡せる。
  const stmt = db.prepare(
    `SELECT s.law_id, s.article_id, s.article_no, s.caption,
            l.title, l.law_num,
            snippet(search_fts, 5, '<mark>', '</mark>', '...', 16) AS snippet
       FROM search_fts s
       JOIN laws l ON l.law_id = s.law_id
      WHERE search_fts MATCH $q
      ORDER BY rank
      LIMIT $limit`
  );
  stmt.bind({ $q: tokens, $limit: limit });
  const out: SearchHit[] = [];
  while (stmt.step()) {
    const row = stmt.getAsObject() as Record<string, unknown>;
    out.push({
      law_id: String(row.law_id ?? ""),
      law_num: (row.law_num as string | null) ?? null,
      title: String(row.title ?? ""),
      article_id: String(row.article_id ?? ""),
      article_no: String(row.article_no ?? ""),
      caption: String(row.caption ?? ""),
      snippet: String(row.snippet ?? ""),
    });
  }
  stmt.free();
  return out;
}

export type ArticleRef = {
  from_law_id: string;
  from_article_id: string;
  to_law_id: string;
  to_article_id: string | null;
  ref_text: string;
  ref_type: string;
};

/** ある article から出ている参照 (同一法内の「第○条」など)。 */
export async function getOutgoingRefs(lawId: string, articleId: string): Promise<ArticleRef[]> {
  const db = await loadDb();
  if (!db) return [];
  const stmt = db.prepare(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = $law AND from_article_id = $art
       ORDER BY id`
  );
  stmt.bind({ $law: lawId, $art: articleId });
  const out: ArticleRef[] = [];
  while (stmt.step()) out.push(stmt.getAsObject() as unknown as ArticleRef);
  stmt.free();
  return out;
}

/** ある article への参照 (被参照: backlinks)。 */
export async function getIncomingRefs(lawId: string, articleId: string): Promise<ArticleRef[]> {
  const db = await loadDb();
  if (!db) return [];
  const stmt = db.prepare(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE to_law_id = $law AND to_article_id = $art
       ORDER BY id`
  );
  stmt.bind({ $law: lawId, $art: articleId });
  const out: ArticleRef[] = [];
  while (stmt.step()) out.push(stmt.getAsObject() as unknown as ArticleRef);
  stmt.free();
  return out;
}

/** 指定法令のすべての outgoing refs を 1 query で取得 (詳細ビューでまとめて使う)。 */
export async function getRefsForLaw(lawId: string): Promise<ArticleRef[]> {
  const db = await loadDb();
  if (!db) return [];
  const stmt = db.prepare(
    `SELECT from_law_id, from_article_id, to_law_id, to_article_id, ref_text, ref_type
       FROM refs WHERE from_law_id = $law OR to_law_id = $law
       ORDER BY id`
  );
  stmt.bind({ $law: lawId });
  const out: ArticleRef[] = [];
  while (stmt.step()) out.push(stmt.getAsObject() as unknown as ArticleRef);
  stmt.free();
  return out;
}

/** メタ情報 (law_count / article_count / built_at) を返す。失敗時は null。 */
export async function getMeta(): Promise<Record<string, string> | null> {
  const db = await loadDb();
  if (!db) return null;
  const meta: Record<string, string> = {};
  const stmt = db.prepare("SELECT key, value FROM meta");
  while (stmt.step()) {
    const row = stmt.getAsObject() as { key: string; value: string };
    meta[row.key] = row.value;
  }
  stmt.free();
  return meta;
}
