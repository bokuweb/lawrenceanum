import type { WikiPage } from "../../data/api";

const roles: Record<string, string> = { government: "政府側（役職表記による）", expert: "有識者（役職表記による）", industry: "業界側（所属表記による）", witness: "参考人（立場未特定）", committee_member: "委員（立場未特定）", unknown: "立場未確認" };
const statuses: Record<string, string> = { unreviewed: "支持関係は未審査", needs_review: "要確認", unsupported: "根拠が支持しないとの判定", indeterminate: "判定不能", llm_supported: "LLM は支持と判定（暫定・人による確認なし）" };
const kinds: Record<string, string> = { government_view: "政府見解", opinion: "意見", request: "要望", question: "質問", decision: "決定・結果", unknown: "区分未確認" };

export function WikiEvidence({ page }: { page: WikiPage }) {
  const scope = page.frontmatter.source_scope as { total_units: number; included_units: number; partial_refs: string[]; omitted_refs: string[]; limitations: string } | undefined;
  const source = page.frontmatter.source_fingerprint;
  const summarySource = page.frontmatter.llm_source_fingerprint;
  const stale = typeof source === "string" && source !== "" && source !== summarySource;
  return (
    <section className="text-xs border border-border rounded-md p-3 mb-5 space-y-2" data-testid="wiki-evidence">
      {page.frontmatter.type === "meeting" && (
        <p role={stale ? "alert" : undefined}>{stale
          ? "原資料の版に対応する要約の更新が未完了です。本文・引用・発言者の立場を原資料で再確認してください。"
          : source ? "要約は最後に取得した資料の版に対応しています。原サイトでの最新性や主張の支持関係を保証するものではありません。"
          : "この要約には原資料の版の記録がありません。取得後の訂正への追随状況は未確認です。"}</p>
      )}
      <p>引用の原文一致は、主張の正しさや意味的な支持を保証しません。以下に記録のない主張の支持関係も未確認です。判定は生成時の LLM の自己点検で、独立レビューではありません。</p>
      {scope ? (
        <details>
          <summary>抜粋の確認範囲：全 {scope.total_units} 発言中 {scope.included_units} 発言、部分収録 {scope.partial_refs.length}、未収録 {scope.omitted_refs.length}</summary>
          <p className="mt-2">{scope.limitations}</p>
          <p>部分収録：{scope.partial_refs.join("、") || "なし"}</p>
          <p>未収録：{scope.omitted_refs.join("、") || "なし"}</p>
        </details>
      ) : <p>このページの取得範囲・主張ごとの根拠情報は未整備です。全文・留保・反対意見を原資料で確認してください。</p>}
      {(page.claim_assessments ?? []).map((claim, i) => (
        <details key={i} className="border-t border-border pt-2">
          <summary>{claim.text} — {statuses[claim.review_status] ?? "支持関係は未確認"}</summary>
          <p className="mt-2">{kinds[claim.kind] ?? "区分未確認"}。{claim.review_note}</p>
          {claim.sources.map((s, j) => (
            <div key={j} className="mt-2 pl-2 border-l border-border">
              <p>{s.ref === claim.attribution ? "主張の帰属先：" : "関連する根拠："}{s.speaker ?? "発言者未確認"} / {s.group || "所属未確認"} / {s.position || "役職未確認"} / {roles[s.role] ?? "立場未確認"} / {s.date}</p>
              <blockquote className="my-1">「{s.quote}」</blockquote>
              {/^https?:\/\//.test(s.url) && <a className="text-primary underline" href={s.url} target="_blank" rel="noreferrer">原文（{s.ref}）</a>}
            </div>
          ))}
        </details>
      ))}
    </section>
  );
}
