import { useState } from "react";
import { MAX_RESEARCH_PROJECTS, researchProjectReport, type ResearchProject } from "../../data/topic-watch";

export function TopicResearchProjects({ projects, active, comparisonCount, storageError, onCreate, onOpen, onEdit, onDelete }: {
  projects: ResearchProject[]; active?: ResearchProject; comparisonCount: number; storageError: boolean;
  onCreate: (title: string) => void; onOpen: (id: string | null) => void;
  onEdit: (project: ResearchProject) => void; onDelete: (id: string) => void;
}) {
  const [title, setTitle] = useState("");
  const [deleting, setDeleting] = useState<string | null>(null);
  const download = () => {
    if (!active) return;
    const blob = new Blob([researchProjectReport(active, window.location.href)], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = `research-project-${active.id}.md`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  return <section aria-label="調査案件" className="rounded-lg border border-border p-4 space-y-3 [overflow-wrap:anywhere]">
    <h2 className="text-base font-semibold">調査案件</h2>
    <p className="text-xs text-muted-foreground">比較対象と判例ごとのメモを案件別に保存できます。調査する問い、確認した出典、残った疑問を残して次回の調査を再開できます。最大{MAX_RESEARCH_PROJECTS}件、このブラウザ内のみです。</p>
    <label className="block text-sm">調査案件を選択
      <select value={active?.id ?? ""} onChange={event => { onOpen(event.target.value || null); setDeleting(null); }} className="block mt-1 w-full min-w-0 border border-border rounded p-2 bg-background">
        <option value="">案件を選ばずに調査</option>
        {projects.map(project => <option key={project.id} value={project.id}>{project.title}</option>)}
      </select>
    </label>
    <form onSubmit={event => { event.preventDefault(); if (!title.trim() || projects.length >= MAX_RESEARCH_PROJECTS) return; onCreate(title.trim()); setTitle(""); setDeleting(null); }} className="flex flex-col sm:flex-row sm:items-end gap-2">
      <label className="text-sm w-full sm:flex-1 min-w-0">新しい案件名<input maxLength={100} value={title} onChange={event => setTitle(event.target.value)} placeholder="例：生活扶助基準の改定を比較" className="block w-full mt-1 p-2 border border-border rounded bg-background" /></label>
      <button type="submit" disabled={!title.trim() || projects.length >= MAX_RESEARCH_PROJECTS} className="text-sm text-left shrink-0 text-primary underline py-2 disabled:opacity-40">現在の比較から案件を作成</button>
    </form>
    <p className="text-xs text-muted-foreground">現在の比較{comparisonCount}件とメモを新しい案件にコピーします。作成後の編集は案件ごとに分かれます。{projects.length >= MAX_RESEARCH_PROJECTS && "上限に達しています。不要な案件を削除してから作成してください。"}</p>
    {active && <div className="border-t border-border pt-3 space-y-3">
      <h3 className="font-medium text-sm">{active.title}</h3>
      <p role="status" className="text-xs text-muted-foreground">{storageError ? "案件の変更を保存できていない可能性があります。再読み込み前に案件の記録を出力してください。" : `案件の変更は自動保存します。保存日時: ${new Date(active.updatedAt).toLocaleString("ja-JP")}`}</p>
      {([ ["question", "調査する問い", 2000, "何を明らかにするために、この判例を比較しますか"], ["findings", "確認した出典・箇所", 4000, "判例番号、原文の頁・段落、確認した内容など"], ["pending", "未確認の点・次に調べること", 4000, "事案の違い、適用時点・経過措置、次に読む資料など"] ] as const).map(([field, label, max, placeholder]) => <label key={field} className="block text-sm">{label}<textarea rows={2} maxLength={max} value={active[field]} onChange={event => onEdit({ ...active, [field]: event.target.value })} placeholder={placeholder} className="block w-full mt-1 p-2 border border-border rounded bg-background text-sm" /></label>)}
      <p className="text-xs text-muted-foreground">これらは自分の調査記録です。資料は再開時に読み直すため、保存時の資料を固定するものではありません。「案件の記録を出力」は記録と確認先リンクを、「比較メモを出力」は公式要旨も含めて出力します（判例2件の読み込み後）。</p>
      <button type="button" onClick={download} className="text-sm text-primary underline">案件の記録を出力</button>
      {deleting === active.id ? <div className="text-sm space-y-2"><p>「{active.title}」の比較対象とメモを削除します。必要な記録は先に出力してください。</p><div className="flex flex-wrap gap-4"><button type="button" onClick={() => { onDelete(active.id); setDeleting(null); }} className="text-destructive underline">この案件を削除</button><button type="button" onClick={() => setDeleting(null)} className="text-primary underline">削除をやめる</button></div></div>
        : <button type="button" onClick={() => setDeleting(active.id)} className="text-xs text-muted-foreground underline">案件を削除</button>}
    </div>}
  </section>;
}
