import definitions from "../../../../config/watch-topics.json";
import type { FeedItem, WatchTopic } from "./api";

export const watchTopics: WatchTopic[] = definitions;
export const WATCH_STORAGE_KEY = "lawrenceanum:topic-watch:v1";
export type WatchState = { followed: boolean; read: string[]; note: string };
export const emptyWatchState: WatchState = { followed: false, read: [], note: "" };

export function readWatchState(): WatchState {
  const raw = localStorage.getItem(WATCH_STORAGE_KEY);
  if (!raw) return emptyWatchState;
  const value = JSON.parse(raw);
  if (value?.version !== 1 || typeof value.followed !== "boolean" || !Array.isArray(value.read) || typeof value.note !== "string") throw new Error("invalid watch state");
  return { followed: value.followed, read: value.read.filter((id: unknown) => typeof id === "string" && id.length < 2000).slice(-2000), note: value.note.slice(0, 5000) };
}

// 同じ資料でも日付が変わる更新イベントは再び未読として扱う。
export const feedItemKey = (item: FeedItem) => JSON.stringify([item.kind, item.date, item.href]);
export const matchedKeywords = (item: FeedItem, topic: WatchTopic) => topic.keywords.filter(word =>
  [item.title, item.summary ?? "", item.law_title ?? ""].some(text => text.includes(word)),
);

const plain = (value: string) => value.replace(/[\\`*_{}[\]<>#|]/g, "\\$&");
export function watchReport(topic: WatchTopic, items: FeedItem[], generatedAt: string, note: string, base: string) {
  const lines = [
    `# ${topic.title} — 調査メモ`, "", `フィード生成日時: ${generatedAt}`, `出力日時: ${new Date().toISOString()}`, "",
    "収録済み資料のキーワード一致による候補です。関連性・影響の確定や網羅性を保証しません。",
    "各種別最新50件まで。法令更新は直近14更新日分、官報は直近30日分の収録ファイルが対象です。",
    "判例の日付は裁判日であり、公表日・取得日ではありません。法令改正の日付は更新データの日付です。",
    "判示事項は裁判所公表データです。LLMによる要約・解説はリンク先で明示し、出典を確認してください。",
    "当時の条文は版候補であり、適用版が確定していない場合があります。", "",
    `対象キーワード: ${topic.keywords.join(" / ")}`, `出力件数: ${items.length}`, "",
  ];
  if (note.trim()) lines.push("## 自分のメモ", "", plain(note), "");
  for (const item of items) {
    const url = item.internal ? `${new URL("./", base).href}#${item.href}` : item.href;
    lines.push(`## ${plain(item.date)} ${plain(item.title)}`, "", `一致した語: ${matchedKeywords(item, topic).join(" / ")}`, "");
    if (item.summary) lines.push(`${item.kind === "hanrei" ? "裁判所公表の判示事項" : "収録データの説明"}: ${plain(item.summary)}`, "");
    lines.push(`確認先: ${url}`, "");
    const caseId = item.kind === "hanrei" && item.href.match(/^\/hanrei\/(\d+)$/)?.[1];
    if (caseId) lines.push(`裁判所の原文・出典: https://www.courts.go.jp/hanrei/${caseId}/detail2/index.html`, "関連Wiki・当時の条文の版候補は判例ページから確認できます（未収録の場合があります）。", "");
  }
  return lines.join("\n");
}
