import thesaurusData from "./jp-law-thesaurus.json" with { type: "json" };

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

/**
 * クエリ文字列を FTS5 の MATCH 式に変換する。
 *
 * content は文字 bigram で索引されているため、1 文字トークンは検索に使えない:
 * - exact (`あ`) → bigram トークンに当たらず ほぼ 0 件
 * - prefix (`あ*`) → prefix index が無いので httpvfs 上で FTS index を
 *   ほぼ全スキャンし 30s 超でハングする
 *
 * よって 1 文字トークンは捨て、2 文字以上のトークン (= bigram) だけ残す。
 * 全トークンが 1 文字なら空文字を返す → 呼び出し側で「2文字以上」を促す。
 */
export function buildFtsMatch(text: string): string {
  return tokenize(text)
    .filter(t => Array.from(t).length >= 2)
    .map(t => `"${t.replace(/"/g, '""')}"`)
    .join(" ");
}

// ── 法令シソーラスによるクエリ同義語展開 ──────────────────────────
// 索引側 (search.db build 時) の同義語追記は法令本文のみ・再ビルドが必要なのに対し、
// クエリ側展開は全 FTS 面 (法令/官報/会議録) に即時効く。入力語に法律 term が含まれて
// いれば、その別表記を OR で足して取りこぼしを減らす。
const THESAURUS_ENTRIES: string[][] = (() => {
  const out: string[][] = [];
  const entries = (thesaurusData as any)?.entries ?? {};
  for (const [k, v] of Object.entries(entries)) {
    if (k.startsWith("_") || typeof v !== "object" || v === null) continue;
    const syns = (v as any).synonyms;
    if (!Array.isArray(syns) || syns.length === 0) continue;
    const forms = [k, ...syns].filter(s => typeof s === "string" && Array.from(s).length >= 2);
    if (forms.length >= 2) out.push(forms);
  }
  return out;
})();

// ASCII だけを小文字化し、元の文字位置を維持する。
const foldAscii = (s: string) => s.replace(/[A-Z]/g, c => c.toLowerCase());
const isAsciiWord = (c: string) => /[a-z0-9_]/i.test(c);
const MATCH_TERMS = THESAURUS_ENTRIES.flatMap(forms => forms.map(term => ({
  term, folded: foldAscii(term), forms,
})));

/** 左から最長一致で用語を区切り、重複・入れ子の同義語展開を避ける。 */
function queryParts(query: string): { text: string; alternatives: string[] }[] {
  const parts: { text: string; alternatives: string[] }[] = [];
  const foldedQuery = foldAscii(query);
  let cursor = 0;
  let remaining = 12;
  while (cursor < query.length) {
    let next: { index: number; term: string; forms: string[] } | undefined;
    for (const { term, folded, forms } of MATCH_TERMS) {
      let index = foldedQuery.indexOf(folded, cursor);
      while (index >= 0) {
        // GMP を preGMP や GMP_2026 の一部として展開しない。
        // 「GMPの適用」のような和文との境界では展開する。
        const embedded = (isAsciiWord(term[0]) && isAsciiWord(query[index - 1] ?? "")) ||
          (isAsciiWord(term[term.length - 1]) && isAsciiWord(query[index + term.length] ?? ""));
        if (!embedded) break;
        index = foldedQuery.indexOf(folded, index + 1);
      }
      if (index < 0) continue;
      if (!next || index < next.index || (index === next.index && term.length > next.term.length)) {
        next = { index, term, forms };
      }
    }
    if (!next || remaining === 0) {
      parts.push({ text: query.slice(cursor), alternatives: [] });
      break;
    }
    if (next.index > cursor) parts.push({ text: query.slice(cursor, next.index), alternatives: [] });
    const alternatives = [...new Set(next.forms)].filter(f => f !== next.term).slice(0, remaining);
    parts.push({ text: next.term, alternatives });
    remaining -= alternatives.length;
    cursor = next.index + next.term.length;
  }
  return parts;
}

/** 実際の検索で展開する別表記を UI にも表示する。 */
export function synonymExpansions(query: string): string[] {
  return [...new Set(queryParts(query.trim()).flatMap(part => part.alternatives))];
}

/** 用語ごとに同義語を OR 展開し、それ以外の検索条件は AND で維持する。 */
export function buildFtsMatchExpanded(query: string): string {
  const base = buildFtsMatch(query);
  if (!base) return base;
  const parts = queryParts(query.trim());
  if (!parts.some(part => part.alternatives.length)) return base;
  return parts.map(part => {
    const groups = [...new Set([part.text, ...part.alternatives].map(buildFtsMatch).filter(Boolean))];
    if (!groups.length) return "";
    return `(${groups.map(group => `(${group})`).join(" OR ")})`;
  }).filter(Boolean).join(" AND ");
}

/**
 * FTS5 snippet() の出力は事前 bigram トークン化されたテキスト
 * (例: `第三 三十 十一 一条 <mark>民法</mark> 法施 施行 ...`) で読みづらいため、
 * 隣接する CJK bigram のオーバーラップ (= 末尾 1 文字 = 先頭 1 文字) を畳んで
 * `第三十一条<mark>民法</mark>施行...` に復元する。
 *
 * - `<mark>` / `</mark>` は通過させる。直前の bigram と直後の bigram に
 *   挟まれていても overlap 判定は維持するので、mark 跨ぎでも崩れない。
 * - ASCII 単語同士は半角空白で区切り直す (元のスペースは separator として捨てる)。
 * - `...` (snippet の省略マーカ) は contiguity を切る。
 */
export function unbigramSnippet(s: string): string {
  type Tok = { kind: "text" | "mark"; v: string };
  const toks: Tok[] = [];
  let i = 0;
  while (i < s.length) {
    if (s.startsWith("<mark>", i)) { toks.push({ kind: "mark", v: "<mark>" }); i += 6; continue; }
    if (s.startsWith("</mark>", i)) { toks.push({ kind: "mark", v: "</mark>" }); i += 7; continue; }
    if (s[i] === " ") { i++; continue; }
    // ellipsis marker "..." は前後にスペースが入らないので、bigram にくっついた
    // `タル...` のような塊を `タル` + `...` に切る。
    if (s.startsWith("...", i)) { toks.push({ kind: "text", v: "..." }); i += 3; continue; }
    let j = i;
    while (
      j < s.length &&
      s[j] !== " " &&
      !s.startsWith("<mark>", j) &&
      !s.startsWith("</mark>", j) &&
      !s.startsWith("...", j)
    ) j++;
    toks.push({ kind: "text", v: s.slice(i, j) });
    i = j;
  }
  const isCjkBigram = (t: string): boolean => {
    const chars = Array.from(t);
    return chars.length === 2 && isCjk(chars[0]) && isCjk(chars[1]);
  };
  let out = "";
  let prev = "";
  for (const t of toks) {
    if (t.kind === "mark") { out += t.v; continue; }
    if (t.v === "...") { out += t.v; prev = ""; continue; }
    if (isCjkBigram(t.v) && isCjkBigram(prev) && prev[1] === t.v[0]) {
      out += t.v[1];
    } else {
      const lastCh = out.length > 0 ? out[out.length - 1] : "";
      if (lastCh && /[A-Za-z0-9]/.test(lastCh) && /[A-Za-z0-9]/.test(t.v[0])) out += " ";
      out += t.v;
    }
    prev = t.v;
  }
  return out;
}

