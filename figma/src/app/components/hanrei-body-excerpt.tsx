import { useHanreiCase } from "../data/use-hanrei";
import { synonymExpansions, tokenize } from "../data/search-query";

// 本文の該当箇所を原文から抜粋する。検索語に表記揺れがある場合は展開語も探す。
export function bodyExcerpt(text: string, query: string, size = 240) {
  const terms = [...query.trim().split(/\s+/), ...synonymExpansions(query), ...tokenize(query)]
    .filter(t => Array.from(t).length >= 2).sort((a, b) => b.length - a.length);
  // PDF の行折り返しをまたぐ語も探せるよう、照合時だけ空白を除き原文位置に戻す。
  const positions: number[] = [];
  let folded = "";
  for (let i = 0; i < text.length; i++) {
    if (/\s/.test(text[i])) continue;
    positions.push(i);
    folded += text[i].replace(/[A-Z]/g, c => c.toLowerCase());
  }
  let found = -1;
  let length = 0;
  for (const term of terms) {
    const needle = term.replace(/\s/g, "").replace(/[A-Z]/g, c => c.toLowerCase());
    const at = folded.indexOf(needle);
    if (at >= 0) {
      found = positions[at];
      length = positions[at + needle.length - 1] - found + 1;
      break;
    }
  }
  const start = Math.max(0, (found < 0 ? 0 : found) - 65);
  const end = Math.min(text.length, Math.max(start + size, found + length));
  return {
    before: `${start ? "…" : ""}${text.slice(start, found < 0 ? end : found)}`,
    match: found < 0 ? "" : text.slice(found, found + length),
    after: `${found < 0 ? "" : text.slice(found + length, end)}${end < text.length ? "…" : ""}`,
  };
}

export function HanreiBodyExcerpt({ caseId, query, holding }: { caseId: number; query: string; holding: string }) {
  const { data, loading, error, retry } = useHanreiCase(caseId);
  const text = data?.body_text?.trim();
  const excerpt = text ? bodyExcerpt(text, query) : null;
  return (
    <div className="mt-2 text-sm leading-relaxed" data-testid="hanrei-body-excerpt">
      {excerpt ? (
        <>
          <p className="text-xs text-muted-foreground mb-1">本文（PDFから抽出・抜粋）</p>
          <p className="whitespace-pre-wrap [overflow-wrap:anywhere]">{excerpt.before}<mark className="bg-amber-300/40 rounded-sm">{excerpt.match}</mark>{excerpt.after}</p>
        </>
      ) : (
        <>
          {holding && <p className="text-muted-foreground line-clamp-2">{holding}</p>}
          <p className="text-xs text-muted-foreground mt-1">
            {loading ? "本文を読み込み中…" : error ? "本文を読み込めませんでした。" : data?.body_text === "" ? "PDFから抽出できる本文テキストがありません。" : "本文テキストは未取得です。"}
            {error && <button type="button" onClick={retry} className="ml-1 text-primary underline">再試行</button>}
          </p>
        </>
      )}
    </div>
  );
}
