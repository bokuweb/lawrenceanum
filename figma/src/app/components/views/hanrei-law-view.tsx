import { useEffect, useState, type ReactNode } from "react";
import { Link, useParams, useSearchParams } from "react-router";
import { api, hanreiUrl, type CaseLawReference, type HanreiLawLinks, type LawDocumentRaw } from "../../data/api";
import { useHanreiCase, useHanreiLawLinks, useWikiPaths } from "../../data/use-hanrei";
import { Button } from "../ui/button";

export const articleLabel = (article: string) => `第${article.split("_")[0]}条${article.split("_").slice(1).map(n => `の${n}`).join("")}`;

export function caseArticleUrl(caseId: string, law: string, article: string, paragraph?: number | null) {
  return `/hanrei/${encodeURIComponent(caseId)}/laws/${encodeURIComponent(law)}/${encodeURIComponent(article)}${paragraph ? `?paragraph=${paragraph}` : ""}`;
}

export function LinkedCaseText({ text, field, links }: { text: string; field: string; links: HanreiLawLinks | null }) {
  const parts: ReactNode[] = [];
  let offset = 0;
  for (const c of links?.citations ?? []) {
    if (c.field !== field || c.start < offset || c.end <= c.start || text.slice(c.start, c.end) !== c.text) continue;
    parts.push(text.slice(offset, c.start));
    parts.push(<Link key={c.start} to={caseArticleUrl(String(links!.case_id), c.law_id, c.article, c.paragraph)} className="text-primary underline underline-offset-4" title="条文の版候補と選定根拠を確認">{c.text}</Link>);
    offset = c.end;
  }
  parts.push(text.slice(offset));
  return <>{parts}</>;
}

export function CaseLawEvidence({ law }: { law: CaseLawReference }) {
  const label = ({ event_date_candidate: "本文の出来事の日付による候補", before_amendment_candidate: "改正前の記載による候補", judgment_date_candidate: "裁判日による暫定候補", unresolved: "版を特定できません" } as Record<string, string>)[law.basis] ?? "版は未確認";
  return <div className="text-sm space-y-2">
    <p className="font-medium">{label}</p>
    {law.evidence.map((e, i) => <p key={i} className="text-muted-foreground">{e.from === e.to ? e.from : `${e.from}〜${e.to}`}：{e.text}</p>)}
    {law.incomplete && <p>対象時点の過去版に未収録・未特定の範囲があります。</p>}
    {law.versions.length > 1 && <p>複数の版が候補です。原文と経過措置を確認してください。</p>}
  </div>;
}

export function HanreiLawView() {
  const { caseId = "", lawId = "", article = "" } = useParams();
  const [params, setParams] = useSearchParams();
  const { data: links, error, retry } = useHanreiLawLinks(caseId);
  const { data: caseData } = useHanreiCase(caseId);
  const paths = useWikiPaths();
  const law = links?.laws.find(l => l.law_id === lawId && l.articles.includes(article));
  const requested = params.get("revision");
  const version = law?.versions.find(v => v.revision_id === requested) ?? (!requested ? law?.versions[0] : undefined);
  const revision = version?.revision_id;
  const revisionSuffix = revision?.startsWith(`${lawId}_`) ? revision.slice(lawId.length + 1) : null;
  const officialVersionUrl = revisionSuffix ? `https://laws.e-gov.go.jp/law/${encodeURIComponent(lawId)}/${encodeURIComponent(revisionSuffix)}` : null;
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ key: string; data?: LawDocumentRaw; error?: boolean }>();
  const key = `${caseId}/${lawId}/${article}/${revision}`;
  useEffect(() => {
    let active = true;
    setState(undefined);
    if (version?.body_available && revision) {
      api.revision(lawId, revision).then(data => {
        if (data.revision_id !== revision || data.law_id !== lawId) throw new Error("revision mismatch");
        if (active) setState({ key, data });
      }).catch(() => { if (active) setState({ key, error: true }); });
    }
    return () => { active = false; };
  }, [key, version?.body_available, attempt]);
  const document = state?.key === key ? state.data : undefined;
  const provision = document?.articles.find(a => a.article_id === `art_${article}`);
  const paragraph = params.get("paragraph");
  const matchingParagraph = provision?.paragraphs.some(p => (p.paragraph_num ?? p.paragraph_no?.normalize("NFKC")) === paragraph);
  return <div className="p-4 md:p-6 max-w-5xl mx-auto [overflow-wrap:anywhere] space-y-4">
    <Link to={`/hanrei/${caseId}`} className="text-primary hover:underline">判例に戻る</Link>
    <h1 className="text-xl font-semibold">{law?.title ?? "関連条文"} {articleLabel(article)}{paragraph ? ` 第${paragraph}項` : ""}</h1>
    <div className="flex gap-4 text-sm flex-wrap" aria-label="公式資料">
      <a className="text-primary underline" href={caseData?.detail_url || hanreiUrl(Number(caseId))} target="_blank" rel="noopener noreferrer">裁判所の原文ページ</a>
      {caseData?.pdf_url && <a className="text-primary underline" href={caseData.pdf_url} target="_blank" rel="noopener noreferrer">全文PDF</a>}
      {officialVersionUrl && <a className="text-primary underline" href={officialVersionUrl} target="_blank" rel="noopener noreferrer">この版候補をe-Govで確認</a>}
    </div>
    {error ? <div role="alert">条文リンクを読み込めませんでした。<Button onClick={retry} variant="outline">再試行</Button></div>
      : !links ? <p>条文リンクを読み込み中…</p>
      : !law ? <p>この判例の条文リンクを確認できません。</p>
      : <>
        <p className="text-sm border rounded-lg p-3 bg-muted/40">{links.notice}</p>
        <CaseLawEvidence law={law} />
        <div className="flex gap-4 text-sm flex-wrap">
          {paths?.has(links.wiki_path) && <Link className="text-primary underline" to={`/wiki/${links.wiki_path}`}>判例のLLM wiki</Link>}
          {paths?.has(law.wiki_path) && <Link className="text-primary underline" to={`/wiki/${law.wiki_path}`}>関連法令のwiki</Link>}
        </div>
        {law.versions.length > 0 && <label className="block text-sm">表示する版候補<select aria-label="表示する版候補" className="block border rounded p-2 w-full mt-1 bg-background" value={revision ?? ""} onChange={e => { const next = new URLSearchParams(params); next.set("revision", e.target.value); setParams(next); }}>
          {requested && !version && <option value="">候補にない版です</option>}
          {law.versions.map(v => <option key={v.revision_id} value={v.revision_id}>{v.effective_date}施行 · {v.revision_id}{v.body_available ? "" : "（本文未収録）"}</option>)}
        </select></label>}
        {!version ? <p>対象時点の法令版を特定できません。裁判所の原文を確認してください。</p>
          : <>
            <p className="text-xs text-muted-foreground">表示版: {version.revision_id}（{version.effective_date}施行）</p>
            {!version.body_available ? <p>この過去版の本文は未収録です。現行条文での代用は行っていません。</p>
              : state?.key === key && state.error ? <div role="alert">過去版本文を読み込めませんでした。<Button variant="outline" onClick={() => setAttempt(n => n + 1)}>本文を再試行</Button></div>
              : !document ? <p>過去版本文を読み込み中…</p>
              : !provision ? <p>この版では対象の条を確認できません（条番号の変更・削除等の可能性があります）。</p>
              : <section className="border rounded-lg p-4 space-y-3" data-testid="case-law-article">
                <h2 className="font-semibold">{provision.caption} {provision.article_no}</h2>
                {paragraph && !matchingParagraph && <p className="text-sm text-muted-foreground">指定の項を確認できないため、条全体を表示します。</p>}
                {provision.paragraphs.map((p, i) => <p key={i} data-highlighted={(p.paragraph_num ?? p.paragraph_no?.normalize("NFKC")) === paragraph || undefined} className={`text-sm whitespace-pre-wrap leading-8 p-2 rounded ${(p.paragraph_num ?? p.paragraph_no?.normalize("NFKC")) === paragraph ? "bg-accent border border-primary" : ""}`}>{p.paragraph_no} {p.text}</p>)}
              </section>}
          </>}
      </>}
  </div>;
}
