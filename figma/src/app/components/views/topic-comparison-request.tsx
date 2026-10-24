import { useEffect, useRef, useState } from "react";
import { Link, useSearchParams } from "react-router";
import { api, type HanreiCase } from "../../data/api";

/** URL は保存命令ではなく候補。内容と保存先を確認してから比較を更新する。 */
export function TopicComparisonRequest({ comparison, projectTitle, storageReady, onApply }: {
  comparison: string[]; projectTitle?: string; storageReady: boolean; onApply: (ids: string[]) => boolean;
}) {
  const [params, setParams] = useSearchParams();
  const requested = params.getAll("case");
  const valid = requested.length > 0 && requested.length <= 2 && requested.every(id => /^[1-9]\d{0,9}$/.test(id));
  const key = valid ? [...new Set(requested)].join(",") : "";
  const requestKey = JSON.stringify(requested);
  const section = useRef<HTMLElement>(null);
  const [loaded, setLoaded] = useState<{ key: string; cases: HanreiCase[] } | null>(null);
  const [error, setError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [notice, setNotice] = useState("");
  const [saveError, setSaveError] = useState(false);
  useEffect(() => {
    if (!section.current) return;
    section.current.focus({ preventScroll: true }); section.current.scrollIntoView({ block: "start" });
  }, [requestKey]);
  useEffect(() => {
    setLoaded(null); setError(false);
    if (!key) return;
    setNotice(""); setSaveError(false);
    let active = true;
    Promise.all(key.split(",").map(async id => {
      const data = await api.hanreiCase(id);
      if (String(data.case_id) !== id) throw new Error("requested case mismatch");
      return data;
    })).then(cases => { if (active) setLoaded({ key, cases }); })
      .catch(() => { if (active) setError(true); });
    return () => { active = false; };
  }, [key, attempt]);
  const clear = () => setParams(previous => {
    const next = new URLSearchParams(previous); next.delete("case"); return next;
  }, { replace: true });
  const openComparison = () => requestAnimationFrame(() => {
    const target = document.getElementById("topic-case-comparison");
    target?.focus({ preventScroll: true }); target?.scrollIntoView({ block: "start" });
  });
  if (!requested.length) return notice ? <p role="status" className="text-sm">{notice}</p> : null;
  const incoming = key ? key.split(",") : [];
  const combined = [...new Set([...comparison, ...incoming])];
  const replace = combined.length > 2;
  const next = replace ? incoming : combined;
  const alreadySelected = incoming.length > 0 && incoming.every(id => comparison.includes(id));
  const ready = valid && loaded?.key === key && !error && storageReady;
  return <section ref={section} tabIndex={-1} aria-label="判例の追加確認" className="rounded-lg border border-primary/40 p-4 space-y-3 text-sm [overflow-wrap:anywhere]">
    <h2 className="font-semibold">判例ページから選んだ比較候補</h2>
    {!valid ? <p role="alert">比較候補の指定を確認できません。正しい判例IDを1〜2件指定してください。保存済みの比較対象は変更していません。</p> : <>
      <p>保存先: {projectTitle ? `調査案件「${projectTitle}」` : "案件を指定しない比較"}。上の調査案件欄で保存先を切り替えられます。</p>
      {error ? <p role="alert">候補の判例を取得できないか、事件IDとの対応を確認できませんでした。<button type="button" onClick={() => setAttempt(n => n + 1)} className="ml-2 text-primary underline">候補を再試行</button></p>
        : loaded?.key !== key ? <p role="status">比較候補の判例を確認中…</p>
          : <ul className="space-y-2">{loaded.cases.map(source => <li key={source.case_id}>
            <Link to={`/hanrei/${source.case_id}`} className="text-primary underline">{source.case_name || source.case_number || `判例 ${source.case_id}`}</Link>
            <p className="text-xs text-muted-foreground">{source.judged_on} · {source.court} · 判例{source.case_id}</p>
            {source.holding && <p className="text-xs mt-1 line-clamp-3 whitespace-pre-wrap">裁判所公表の判示事項: {source.holding}</p>}
          </li>)}</ul>}
      <p className="text-xs text-muted-foreground">現在の比較対象: {comparison.length ? comparison.map(id => `判例${id}`).join("・") : "なし"}</p>
      {alreadySelected ? <p>この候補はすでに比較対象に含まれています。</p>
        : <p>{replace ? `比較対象を次の${next.length}件に入れ替えます` : "追加後の比較対象"}: {next.map(id => `判例${id}`).join("・")}。比較から外れる判例のメモも保持します。</p>}
      <p className="text-xs text-muted-foreground">候補は利用者が選んだ判例です。共通条文があっても、先例関係・同じ争点や適用法令の版の一致は確定しません。</p>
      {!storageReady && <p role="alert">保存データを読み込めていないため、比較を変更できません。画面を再読み込みして保存先を確認してください。</p>}
      <button type="button" disabled={!ready} onClick={() => {
        if (!ready) return;
        if (onApply(next)) {
          setNotice("比較対象をブラウザに保存しました。"); setSaveError(false);
          clear(); openComparison();
        } else {
          setSaveError(true);
          setNotice("ブラウザに保存できませんでした。この画面内の比較は更新しています。再試行するか、調査メモを出力してください。");
        }
      }} className="text-primary underline disabled:opacity-40">{saveError ? "比較対象の保存を再試行" : alreadySelected ? "比較画面へ進む" : replace ? `比較対象を${next.length}件に入れ替える` : "比較対象に追加して保存"}</button>
    </>}
    {notice && <p role="status">{notice}</p>}
    <button type="button" onClick={() => { setNotice(""); clear(); }} className="block text-muted-foreground underline">候補を閉じる</button>
  </section>;
}
