import { useEffect, useState } from "react";
import { Link, useNavigate } from "react-router";
import { api, type WikiPage } from "../../data/api";
import { Button } from "../ui/button";
import { WikiMarkdown } from "./wiki-markdown";

/** 公開済み Wiki の LLM 区間だけを表示し、公式要旨・判決本文と混ぜない。 */
export function HanreiExplanation({ caseId, published }: { caseId: string; published: boolean }) {
  const navigate = useNavigate();
  const [attempt, setAttempt] = useState(0);
  const [state, setState] = useState<{ page: WikiPage | null; error: boolean }>({ page: null, error: false });
  const path = `hanrei/${caseId}`;
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
        </> : <p className="text-sm text-muted-foreground">この配信版の解説は判例Wikiで確認できます。</p>}
      <Link to={`/wiki/${path}`} className="inline-block text-sm text-primary underline">解説・関連判例をWikiで読む</Link>
    </section>
  );
}
