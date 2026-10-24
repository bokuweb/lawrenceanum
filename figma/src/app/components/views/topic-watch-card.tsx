import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "../../data/api";
import { feedItemChange, feedItemKey, matchedKeywords, readWatchState, watchTopics } from "../../data/topic-watch";

export function TopicWatchCard() {
  const [following, setFollowing] = useState(false);
  const [unread, setUnread] = useState<number | null>(null);
  const [error, setError] = useState(false);
  const [changes, setChanges] = useState<{ added: number; changed: number } | "older" | null>(null);
  const topic = watchTopics[0];
  useEffect(() => {
    let active = true;
    try {
      const state = readWatchState();
      if (!state.followed) return;
      setFollowing(true);
      api.topicFeed(topic.id).then(feed => {
        if (feed.schema_version !== 1 || feed.topic?.id !== topic.id || !Array.isArray(feed.items) || typeof feed.generated_at !== "string" || Number.isNaN(Date.parse(feed.generated_at))) throw new Error("invalid topic feed");
        const read = new Set(state.read);
        const items = feed.items.filter(item => matchedKeywords(item, topic).length);
        if (active) {
          setUnread(items.filter(item => !read.has(feedItemKey(item))).length);
          if (state.checkpoint) setChanges(Date.parse(feed.generated_at) < Date.parse(state.checkpoint.generatedAt) ? "older" : {
            added: items.filter(item => feedItemChange(item, state.checkpoint!) === "追加").length,
            changed: items.filter(item => feedItemChange(item, state.checkpoint!) === "表示情報の変更").length,
          });
        }
      }).catch(() => { if (active) setError(true); });
    } catch { /* 保存できない場合はテーマ画面の案内から利用できる。 */ }
    return () => { active = false; };
  }, [topic]);
  if (!following) return null;
  return <section aria-label="追跡中のテーマ" className="rounded-lg border border-border p-4 space-y-2">
    <h2 className="text-base">追跡中のテーマ</h2>
    <Link to="/watch" className="text-primary underline">{topic.title}</Link>
    <p className="text-sm text-muted-foreground">{error ? "資料の取得に失敗しました。テーマ画面で再試行できます。" : unread === null ? "未読を確認中…" : `収録範囲内の未読 ${unread}件`}</p>
    {!error && changes && <p className="text-xs text-muted-foreground">{changes === "older" ? "比較基準より古い配信のため、差分は未判定です。" : `保存した一覧から追加 ${changes.added}件 · 表示情報の変更 ${changes.changed}件（公表日以降の新着件数ではありません）`}</p>}
  </section>;
}
