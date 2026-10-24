import { Link } from "react-router";
import type { HanreiCase } from "../../data/api";
import { citationFields, compareCaseArticles, type ComparedArticle, type ComparisonLawState } from "../../data/case-article-comparison";
import { articleLabel, caseArticleUrl, CaseLawEvidence } from "./hanrei-law-view";

export function TopicArticleComparison({ cases, states, paths }: {
  cases: HanreiCase[]; states: (ComparisonLawState & { retry: () => void })[]; paths: ReadonlySet<string> | null;
}) {
  const ready = states.every(state => state.data);
  const rows = ready ? compareCaseArticles(cases, states.map(state => state.data!)) : [];
  return <section aria-label="抽出条文の比較" className="rounded border border-border p-3 space-y-3">
    <h3 className="text-sm font-semibold">抽出条文の比較</h3>
    <p className="text-xs text-muted-foreground">法令IDと条番号の一致を機械的に比較しています。項・適用版・争点・先例関係の一致を示しません。「未抽出」は言及や適用がないことを意味しません。</p>
    {!ready ? states.map((state, index) => !state.data && <p key={cases[index].case_id} role="status" className="text-xs">
      判例{cases[index].case_id}：{state.error ? <>条文データを確認できないため、共通・片方のみの判定を保留しています。<button type="button" onClick={state.retry} className="ml-1 text-primary underline">判例{cases[index].case_id}の条文比較を再試行</button></> : "条文データを確認中…"}
    </p>) : <>
      <p className="text-xs">共通して抽出：{rows.filter(row => row.sides.every(Boolean)).length}条 / 片方のみで抽出：{rows.filter(row => !row.sides.every(Boolean)).length}条</p>
      {!rows.length && <p className="text-xs text-muted-foreground">比較できる条文は未抽出です。裁判所の参照法条・裁判要旨・本文を確認してください。</p>}
      {rows.map(row => <article key={row.key} className="border-t border-border pt-3 space-y-2">
        <h4 className="text-sm font-medium">{row.sides.find(Boolean)!.law.title} {articleLabel(row.article)}</h4>
        <p className="text-xs text-muted-foreground">{row.sides.every(Boolean) ? "両判例で抽出（条単位）" : `判例${cases[row.sides.findIndex(Boolean)].case_id}のみで抽出（他方は未抽出）`}</p>
        {paths?.has(`laws/${row.lawId}`) && <Link to={`/wiki/laws/${row.lawId}`} className="text-xs text-primary underline">法令Wikiで解説・関連資料を確認</Link>}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
          {row.sides.map((side, index) => <div key={cases[index].case_id} className="min-w-0 rounded border border-border p-2 space-y-2 text-xs">
            <p className="font-medium">判例{cases[index].case_id}・{cases[index].judged_on || "裁判日未収録"}</p>
            {side ? <ArticleSide side={side} row={row} caseId={String(cases[index].case_id)} />
              : <p className="text-muted-foreground">この条文は未抽出です。言及の有無は原文で確認してください。</p>}
          </div>)}
        </div>
      </article>)}
    </>}
  </section>;
}

function ArticleSide({ side, row, caseId }: { side: NonNullable<ComparedArticle["sides"][number]>; row: ComparedArticle; caseId: string }) {
  const paragraphs = [...new Set(side.citations.map(c => c.paragraph).filter((n): n is number => Number.isInteger(n) && n! > 0))].sort((a, b) => a - b);
  return <>
    <Link to={caseArticleUrl(caseId, row.lawId, row.article)} className="text-primary underline">判例{caseId}の条文・版候補</Link>
    <p>抽出された項：{paragraphs.length ? paragraphs.map((n, index) => <span key={n}>{index > 0 && "・"}<Link to={caseArticleUrl(caseId, row.lawId, row.article, n)} aria-label={`判例${caseId}の第${n}項の候補`} className="text-primary underline">第{n}項</Link></span>) : "項の特定なし"}（言及箇所ごとの指定）</p>
    <details>
      <summary className="cursor-pointer text-primary underline">版の選定根拠・言及箇所</summary>
      <div className="mt-2 space-y-3">
        <CaseLawEvidence law={side.law} />
        {side.citations.length ? <ul className="space-y-2">{side.citations.map(c => <li key={`${c.field}:${c.start}:${c.end}`}>
          <Link to={`/hanrei/${caseId}?source=${c.field}`} className="text-primary underline">{citationFields[c.field as keyof typeof citationFields]}の言及箇所を確認</Link>
          <p className="whitespace-pre-wrap mt-1">「{c.text}」</p>
        </li>)}</ul> : <p>現在の判例データと一致する言及箇所を確認できません。抽出データの版候補と原文を照合してください。</p>}
      </div>
    </details>
  </>;
}
