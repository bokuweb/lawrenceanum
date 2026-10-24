import { useEffect, useState } from "react";
import { Link } from "react-router";
import { api } from "../../data/api";
import { feedItemKey, matchedKeywords, readWatchState, watchTopics } from "../../data/topic-watch";

export function TopicWatchCard() {
  const [following, setFollowing] = useState(false);
  const [unread, setUnread] = useState<number | null>(null);
  const [error, setError] = useState(false);
  const topic = watchTopics[0];
  useEffect(() => {
    let active = true;
    try {
      const state = readWatchState();
      if (!state.followed) return;
      setFollowing(true);
      api.topicFeed(topic.id).then(feed => {
        if (feed.schema_version !== 1 || feed.topic?.id !== topic.id || !Array.isArray(feed.items)) throw new Error("invalid topic feed");
        const read = new Set(state.read);
        if (active) setUnread(feed.items.filter(item => matchedKeywords(item, topic).length && !read.has(feedItemKey(item))).length);
      }).catch(() => { if (active) setError(true); });
    } catch { /* 保存できない場合はテーマ画面の案内から利用できる。 */ }
    return () => { active = false; };
  }, [topic]);
  if (!following) return null;
  return <section aria-label="追跡中のテーマ" className="rounded-lg border border-border p-4 space-y-2">
    <h2 className="text-base">追跡中のテーマ</h2>
    <Link to="/watch" className="text-primary underline">{topic.title}</Link>
    <p className="text-sm text-muted-foreground">{error ? "資料の取得に失敗しました。テーマ画面で再試行できます。" : unread === null ? "未読を確認中…" : `収録範囲内の未読 ${unread}件`}</p>
  </section>;
}
