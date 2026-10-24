import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router";
import { Bell, Check, Download, Rss } from "lucide-react";
import { api, type TopicFeed } from "../../data/api";
import { emptyWatchState, feedItemKey, matchedKeywords, readWatchState, WATCH_STORAGE_KEY, watchCaseId, watchReport, watchTopics, withCaseNote, type WatchState } from "../../data/topic-watch";
import { FeedRow } from "./feed-view";
import { TopicCaseComparison } from "./topic-case-comparison";

const topic = watchTopics[0];

export function TopicWatchView() {
  const [data, setData] = useState<TopicFeed | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [storageError, setStorageError] = useState(false);
  const [state, setState] = useState<WatchState>(emptyWatchState);
  const [onlyUnread, setOnlyUnread] = useState(false);
  useEffect(() => {
    try { setState(readWatchState()); } catch { setStorageError(true); }
  }, []);
  useEffect(() => {
    let active = true;
    setLoading(true); setError(false);
    api.topicFeed(topic.id).then(value => {
      if (value.schema_version !== 1 || value.topic?.id !== topic.id || !Array.isArray(value.items)) throw new Error("topic feed mismatch");
      if (active) { setData(value); setLoading(false); }
    }).catch(() => { if (active) { setError(true); setLoading(false); } });
    return () => { active = false; };
  }, [attempt]);

  const update = (next: WatchState) => {
    setState(next);
    try { localStorage.setItem(WATCH_STORAGE_KEY, JSON.stringify({ version: 1, ...next })); setStorageError(false); }
    catch { setStorageError(true); }
  };
  const items = useMemo(() => [...(data?.items ?? [])].filter(item => matchedKeywords(item, topic).length > 0).sort((a, b) => b.date.localeCompare(a.date)), [data]);
  const read = useMemo(() => new Set(state.read), [state.read]);
  const unread = items.filter(item => !read.has(feedItemKey(item)));
  const shown = onlyUnread ? unread : items;
  const toggleRead = (key: string) => update({ ...state, read: read.has(key) ? state.read.filter(id => id !== key) : [...state.read, key].slice(-2000) });
  const toggleComparison = (id: string) => {
    const selected = state.comparison.includes(id);
    if (!selected && state.comparison.length >= 2) return;
    update({ ...state, comparison: selected ? state.comparison.filter(value => value !== id) : [...state.comparison, id] });
  };
  const download = () => {
    if (!data) return;
    const blob = new Blob([watchReport(topic, shown, data.generated_at, state.note, window.location.href)], { type: "text/markdown;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url; link.download = `${topic.id}-${new Date().toISOString().slice(0, 10)}.md`; link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };

  return <div className="p-4 sm:p-6 max-w-5xl space-y-5">
    <header>
      <p className="text-xs text-muted-foreground mb-1">テーマ追跡 · 無料試用</p>
      <h1 className="text-2xl flex items-center gap-2"><Bell className="size-6 shrink-0" />{topic.title}</h1>
      <p className="text-sm text-muted-foreground mt-2">{topic.description} 判例から当時の条文の版候補・関連Wikiへ進み、確認した資料を調査メモに残せます。</p>
    </header>

    <section aria-label="テーマの追跡設定" className="rounded-lg border border-border p-4 space-y-3">
      <div className="flex flex-wrap items-center gap-3">
        <button type="button" aria-pressed={state.followed} onClick={() => update({ ...state, followed: !state.followed })} className="rounded-md bg-primary text-primary-foreground px-3 py-2 text-sm inline-flex gap-2 items-center">
          {state.followed && <Check className="size-4" />}{state.followed ? "追跡中（解除する）" : "このテーマを追跡"}
        </button>
        <a href={`./feeds/topics/${topic.id}.xml`} target="_blank" rel="noreferrer" className="text-sm inline-flex items-center gap-1.5 text-primary underline"><Rss className="size-4" />テーマ別RSSを購読</a>
      </div>
      <p className="text-xs text-muted-foreground">追跡設定・既読・自分のメモはこのブラウザに保存します。追跡中のテーマはダッシュボードに未読件数を表示します。継続通知はRSSリーダーで購読できます。</p>
      {storageError && <p role="status" className="text-sm text-amber-700 dark:text-amber-400">ブラウザの保存データを読み書きできません。現在の操作はこの画面内で利用できますが、再読み込みすると失われる場合があります。</p>}
      <p className="text-xs text-muted-foreground">抽出する語: {topic.keywords.join(" / ")}。タイトル・判示事項などの収録メタデータとの一致です。関連性や重要度の確定には原文確認が必要です。</p>
    </section>

    <section aria-label="収録範囲" className="text-xs text-muted-foreground space-y-1">
      <p>テーマに一致した各種別の最新50件まで。法令更新は直近14更新日分、官報は直近30日分の収録ファイルが対象です。全文・全判例を網羅するものではありません。</p>
      <p>判例は裁判日順（公表日・取得日ではありません）。法令改正は更新データの日付であり、施行日とは異なります。パブコメは募集開始・結果公示日、会議は開催日、調達は公示日です。</p>
      <p>判示事項は裁判所公表データです。LLMによる要約・解説はリンク先で明示します。当時の条文の適用版が未確定のことがあります。</p>
      {data && <p>フィード生成日時: {data.generated_at}</p>}
    </section>

    {state.comparison.length > 0 && <TopicCaseComparison ids={state.comparison} notes={state.caseNotes} onRemove={toggleComparison} onNote={(id, note) => update(withCaseNote(state, id, note))} />}

    <div className="flex flex-wrap items-center justify-between gap-3">
      <h2 className="text-base">追跡する資料 <span className="text-sm text-muted-foreground">{loading || error ? "" : `${items.length}件 · 未読${unread.length}件`}</span></h2>
      <div className="flex flex-wrap gap-3 items-center">
        <label className="text-sm flex gap-2 items-center"><input type="checkbox" checked={onlyUnread} onChange={event => setOnlyUnread(event.target.checked)} />未読のみ</label>
        <button type="button" onClick={download} disabled={loading || error || shown.length === 0} className="inline-flex items-center gap-1.5 text-sm border border-border rounded-md px-3 py-2 disabled:opacity-40"><Download className="size-4" />表示中の資料を調査メモに出力</button>
      </div>
    </div>
    <p className="text-xs text-muted-foreground">判例は「比較に追加」から2件まで選べます。{state.comparison.length === 2 && "比較対象を入れ替えるには、選択済みの判例を比較から外してください。"}</p>
    <section aria-label="追跡資料一覧" className="space-y-3">
      {loading ? <p role="status" className="text-sm text-muted-foreground">テーマの資料を読み込み中…</p>
        : error ? <div role="alert" className="rounded-lg border border-border p-5 text-sm"><p>テーマのフィードを読み込めませんでした。</p><button type="button" onClick={() => setAttempt(n => n + 1)} className="text-primary underline mt-2">再試行</button></div>
          : shown.length === 0 ? <p className="text-sm text-muted-foreground rounded-lg border border-border p-5">{items.length ? "未読の資料はありません。" : "収録範囲内に一致する資料はありません。更新がないことを意味するものではありません。"}</p>
            : shown.map(item => <article key={feedItemKey(item)} className="rounded-lg border border-border overflow-hidden">
              <FeedRow item={item} />
              <div className="px-4 py-2 flex flex-wrap items-center justify-between gap-2 text-xs">
                <span className="text-muted-foreground">一致した語: {matchedKeywords(item, topic).join(" / ")}</span>
                <div className="flex flex-wrap gap-3">
                  {watchCaseId(item) && <button type="button" aria-pressed={state.comparison.includes(watchCaseId(item)!)} disabled={state.comparison.length >= 2 && !state.comparison.includes(watchCaseId(item)!)} onClick={() => toggleComparison(watchCaseId(item)!)} className="text-primary underline disabled:opacity-40">{state.comparison.includes(watchCaseId(item)!) ? "比較から外す" : "比較に追加"}</button>}
                  <button type="button" aria-pressed={read.has(feedItemKey(item))} onClick={() => toggleRead(feedItemKey(item))} className="text-primary underline">{read.has(feedItemKey(item)) ? "未読に戻す" : "既読にする"}</button>
                </div>
              </div>
            </article>)}
    </section>
    <label className="block text-sm">自分の調査メモ（ブラウザ内に保存）<textarea value={state.note} onChange={event => update({ ...state, note: event.target.value })} maxLength={5000} rows={4} placeholder="確認した点、次に読む資料、調査にかかった時間など" className="block w-full mt-2 p-3 rounded-md border border-border bg-background text-sm" /></label>
    <p className="text-xs text-muted-foreground">調査メモには表示中の資料、確認先URL、一致した語、収録範囲、自分のメモを出力します。<Link to="/feed" className="text-primary underline">すべての新着</Link> · <Link to="/hanrei" className="text-primary underline">判例を検索</Link> · <Link to="/wiki" className="text-primary underline">Wikiを読む</Link></p>
  </div>;
}
