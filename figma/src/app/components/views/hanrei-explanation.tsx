import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router";
import { api, type HanreiCase, type WikiPage } from "../../data/api";
import { Button } from "../ui/button";
import { HanreiCitationContext } from "./hanrei-citation-context";
import { WikiMarkdown } from "./wiki-markdown";
import { articleLabel, caseArticleUrl } from "./hanrei-law-view";

/** 公開済み Wiki の LLM 区間だけを表示し、公式要旨・判決本文と混ぜない。 */
export function HanreiExplanation({ caseId, wikiPaths, source }: { caseId: string; wikiPaths: ReadonlySet<string> | null; source: HanreiCase }) {
  const navigate = useNavigate();
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ page: WikiPage | null; error: boolean }>({ page: null, error: false });
  const path = `hanrei/${caseId}`;
  const published = wikiPaths?.has(path) ?? false;
  useEffect(() => {
    let active = true;
    setState({ page: null, error: false });
    if (!published) return;
    api.wikiPage(path).then(page => {
      const fm = page.frontmatter;
      if (page.path !== path || fm.type !== "hanrei" || String(fm.case_id) !== caseId || page.stub
        || typeof fm.source_fingerprint !== "string" || !fm.source_fingerprint
        || fm.source_fingerprint !== fm.llm_source_fingerprint) throw new Error("case explanation mismatch");
      if (active) setState({ page, error: false });
    }).catch(() => { if (active) setState({ page: null, error: true }); });
    return () => { active = false; };
  }, [caseId, path, published, attempt]);
  if (!published) return null;
  const page = state.page;
  const fm = page?.frontmatter;
  const input = fm?.summary_input as { total_chars?: number; included_chars?: number } | undefined;
  const markdown = page?.hanrei_explanation;
  const related = (page?.hanrei_related_cases ?? []).filter(c =>
    Number.isSafeInteger(c.case_id) && c.case_id > 0 && String(c.case_id) !== caseId
    && c.path === `hanrei/${c.case_id}` && wikiPaths?.has(c.path)
  ).slice(0, 10);
  return (
    <section className="mt-6 border rounded-lg p-4 md:p-6 space-y-3" data-testid="hanrei-explanation" aria-label="AIによる要約・解説">
      <h2 className="text-base font-semibold">AIによる要約・解説</h2>
      <p className="text-xs text-muted-foreground">公開済みの判例Wikiから表示しています。裁判所の公式要旨とは区別し、引用先の原文と照合してください。</p>
      {state.error ? <div role="alert" className="text-sm">
        <p>解説を読み込めないか、判例・資料の版との対応を確認できませんでした。</p>
        <Button variant="outline" size="sm" className="mt-2" onClick={() => setAttempt(n => n + 1)}>解説を再試行</Button>
      </div> : !page ? <p className="text-sm text-muted-foreground">解説を読み込み中…</p>
        : typeof markdown === "string" && markdown.trim() ? <>
          {typeof fm?.source_scope === "string" && <p className="text-xs text-muted-foreground">収録資料：{fm.source_scope}</p>}
          {typeof input?.total_chars === "number" && typeof input.included_chars === "number" && input.included_chars < input.total_chars && (
            <p className="text-xs text-muted-foreground">原資料 {input.total_chars.toLocaleString()} 文字中 {input.included_chars.toLocaleString()} 文字を入力した解説です。抜粋外の理由・留保・反対意見は未確認です。</p>
          )}
          <WikiMarkdown body={markdown} from={path} onOpen={target => navigate(`/wiki/${target}`)} />
          <HanreiCitationContext caseId={caseId} citations={page.hanrei_citations ?? []} source={source} />
        </> : <p className="text-sm text-muted-foreground">この配信版の解説は判例Wikiで確認できます。</p>}
      {related.length > 0 && <section className="border-t pt-4 space-y-3" data-testid="hanrei-related-cases" aria-label="共通する参照条文を持つ判例">
        <h3 className="text-sm font-semibold">共通する参照条文を持つ判例</h3>
        <p className="text-xs text-muted-foreground">同じ法令・条番号への言及を根拠に、共通条文の多い順に最大10件を表示します。先例関係・同じ争点や結論・適用法令の版の一致を示すものではありません。条文リンクは表示中の判例についての版候補です。</p>
        <ul className="space-y-4 text-sm [overflow-wrap:anywhere]">
          {related.map(c => <li key={c.path} className="space-y-1">
            <Link to={`/hanrei/${c.case_id}`} className="font-medium text-primary underline">{c.title}</Link>
            <p className="text-xs text-muted-foreground">{c.judged_on || "裁判日未記録"}・{c.court || "裁判所未記録"}</p>
            {c.description && <p>AIによる概要：{c.description}</p>}
            <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs">
              {(c.common_articles ?? []).map(a => <Link key={`${a.law_id}/${a.article}`} to={caseArticleUrl(caseId, a.law_id, a.article)} className="text-primary underline">{a.title} {articleLabel(a.article)}</Link>)}
              <Link to={`/wiki/${c.path}`} className="text-primary underline" aria-label={`${c.title}のWiki`}>判例Wiki</Link>
            </div>
          </li>)}
        </ul>
      </section>}
      <Link to={`/wiki/${path}`} className="inline-block text-sm text-primary underline">解説・関連判例をWikiで読む</Link>
    </section>
  );
}
