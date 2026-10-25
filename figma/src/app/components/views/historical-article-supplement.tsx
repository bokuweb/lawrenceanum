import type { HistoricalArticleSupplement } from "../../data/historical-article-supplements";

export function HistoricalArticleSupplementPanel({ supplement, paragraph }: { supplement: HistoricalArticleSupplement; paragraph: string | null }) {
  const matching = supplement.paragraphs.some(p => p.number === paragraph);
  return <section className="border rounded-lg p-4 space-y-3" aria-label="公的資料による当時の条文の補完" data-testid="historical-article-supplement">
    <h2 className="font-semibold">公的資料で文言を照合した条文</h2>
    <p className="text-sm">対象期間：{supplement.from}〜{supplement.to} · 確認日：{supplement.checkedOn}</p>
    <p className="text-sm">法令全体の過去版が未収録でも、この条は制定時の条文・改正法・公的資料の改正注記から文言を補っています。法令全体の参考候補とは別の、条単位の補完資料です。</p>
    <p className="text-sm" role="note">事件に実際に適用された法令版・経過措置を確定するものではありません。{supplement.limitations.join("")}</p>
    <h3 className="font-medium">第{supplement.article}条（{supplement.caption}）</h3>
    {paragraph && !matching && <p className="text-sm">指定の項を確認できないため、条全体を表示します。</p>}
    {supplement.paragraphs.map(p => <p key={p.number} data-highlighted={p.number === paragraph || undefined} className={`text-sm whitespace-pre-wrap leading-8 p-2 rounded ${p.number === paragraph ? "bg-accent border border-primary" : ""}`}>{p.number}　{p.text}</p>)}
    <details className="text-sm space-y-2">
      <summary className="cursor-pointer text-primary">確認方法・出典を開く</summary>
      {supplement.findings.map(f => <p key={f}>{f}</p>)}
      <p>改正注記を使った条単位の照合です。全ての改正法を個別に再検証した法令全体の復元版ではありません。条文は公的資料から転記し、LLMによる要約ではありません。</p>
      <ul className="list-disc pl-5 space-y-1">{supplement.sources.map(s => <li key={s.url}><a className="text-primary underline" href={s.url} target="_blank" rel="noopener noreferrer">{s.title}</a></li>)}</ul>
    </details>
  </section>;
}
