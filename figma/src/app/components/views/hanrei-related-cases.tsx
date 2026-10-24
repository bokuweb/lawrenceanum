import { useState } from "react";
import { Link } from "react-router";
import type { WikiRelatedCase } from "../../data/api";
import { articleLabel, caseArticleUrl } from "./hanrei-law-view";

type Article = WikiRelatedCase["common_articles"][number];
const articleKey = (a: Article) => JSON.stringify([a.law_id, a.article]);

export function publishedRelatedCases(caseId: string, cases: WikiRelatedCase[], wikiPaths: ReadonlySet<string> | null) {
  const seen = new Set<string>();
  return cases.map(c => ({ ...c, common_articles: [...new Map((c.common_articles ?? [])
    .filter(a => a.law_id && a.article && a.title)
    .map(a => [articleKey(a), a] as const)).values()] }))
    .filter(c => {
      if (!Number.isSafeInteger(c.case_id) || c.case_id <= 0 || String(c.case_id) === caseId
        || c.path !== `hanrei/${c.case_id}` || !wikiPaths?.has(c.path)
        || !c.common_articles.length || seen.has(c.path)) return false;
      seen.add(c.path);
      return true;
    }).slice(0, 10);
}

/** 関連付けの根拠である条文を選び、公開済みの判例と法令 Wiki をたどる。 */
export function HanreiRelatedCases({ caseId, cases, wikiPaths }: {
  caseId: string; cases: WikiRelatedCase[]; wikiPaths: ReadonlySet<string> | null;
}) {
  const [selection, setSelection] = useState("");
  const related = publishedRelatedCases(caseId, cases, wikiPaths);
  const articles = new Map(related.flatMap(c => c.common_articles.map(a => [articleKey(a), a] as const)));
  const selected = articles.has(selection) ? selection : "";
  const visible = selected ? related.filter(c => c.common_articles.some(a => articleKey(a) === selected)) : related;
  if (!related.length) return null;
  return <section className="border-t pt-4 space-y-3" data-testid="hanrei-related-cases" aria-label="共通する参照条文を持つ判例">
    <h3 className="text-sm font-semibold">共通する参照条文を持つ判例</h3>
    <p className="text-xs text-muted-foreground">同じ法令・条番号への言及を根拠に、共通条文の多い順に最大10件を表示します。先例関係・同じ争点や結論・適用法令の版の一致を示すものではありません。条文リンクは表示中の判例についての版候補です。</p>
    <label className="block text-xs space-y-1">
      <span>共通条文で絞り込み</span>
      <select aria-label="関連判例の共通条文" value={selected} onChange={e => setSelection(e.target.value)} className="block w-full min-w-0 rounded border bg-background p-2 text-sm">
        <option value="">すべての共通条文（{related.length}件）</option>
        {[...articles].map(([key, a]) => <option key={key} value={key}>{a.title} {articleLabel(a.article)}（{related.filter(c => c.common_articles.some(v => articleKey(v) === key)).length}件）</option>)}
      </select>
    </label>
    <p role="status" className="text-xs text-muted-foreground">{visible.length} / {related.length}件（公開済みの関連候補内）</p>
    <ul className="space-y-4 text-sm [overflow-wrap:anywhere]">
      {visible.map(c => <li key={c.path} className="space-y-1">
        <Link to={`/hanrei/${c.case_id}`} className="font-medium text-primary underline">{c.title}</Link>
        <p className="text-xs text-muted-foreground">{c.judged_on || "裁判日未記録"}・{c.court || "裁判所未記録"}</p>
        {c.description && <p>AI概要（LLM生成）：{c.description}</p>}
        <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
          {c.common_articles.map(a => <span key={articleKey(a)} className="space-x-2">
            <Link to={caseArticleUrl(caseId, a.law_id, a.article)} className="text-primary underline">{a.title} {articleLabel(a.article)}</Link>
            {wikiPaths?.has(`laws/${a.law_id}`) && <Link to={`/wiki/laws/${a.law_id}`} className="text-primary underline" aria-label={`${a.title}の法令Wiki`}>法令Wiki</Link>}
          </span>)}
          <Link to={`/wiki/${c.path}`} className="text-primary underline" aria-label={`${c.title}のWiki`}>判例Wiki</Link>
        </div>
      </li>)}
    </ul>
  </section>;
}
