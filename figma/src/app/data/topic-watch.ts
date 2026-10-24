import definitions from "../../../../config/watch-topics.json";
import { hanreiUrl, type FeedItem, type HanreiCase, type WatchTopic } from "./api";

export const watchTopics: WatchTopic[] = definitions;
export const WATCH_STORAGE_KEY = "lawrenceanum:topic-watch:v1";
export type WatchState = { followed: boolean; read: string[]; note: string; comparison: string[]; caseNotes: Record<string, string> };
export const emptyWatchState: WatchState = { followed: false, read: [], note: "", comparison: [], caseNotes: {} };
const validCaseId = (value: unknown): value is string => typeof value === "string" && /^[1-9]\d{0,9}$/.test(value);
export const watchCaseId = (item: FeedItem) => item.kind === "hanrei" && item.internal ? item.href.match(/^\/hanrei\/([1-9]\d{0,9})$/)?.[1] : undefined;

export function readWatchState(): WatchState {
  const raw = localStorage.getItem(WATCH_STORAGE_KEY);
  if (!raw) return emptyWatchState;
  const value = JSON.parse(raw);
  if (value?.version !== 1 || typeof value.followed !== "boolean" || !Array.isArray(value.read) || typeof value.note !== "string") throw new Error("invalid watch state");
  const comparison = Array.isArray(value.comparison) ? [...new Set<string>(value.comparison.filter(validCaseId))].slice(0, 2) : [];
  const caseNotes = value.caseNotes && typeof value.caseNotes === "object" && !Array.isArray(value.caseNotes)
    ? Object.fromEntries(Object.entries(value.caseNotes).filter(([id, note]) => validCaseId(id) && typeof note === "string").slice(-100).map(([id, note]) => [id, (note as string).slice(0, 2000)])) : {};
  return { followed: value.followed, read: value.read.filter((id: unknown) => typeof id === "string" && id.length < 2000).slice(-2000), note: value.note.slice(0, 5000), comparison, caseNotes };
}

export function withCaseNote(state: WatchState, caseId: string, note: string): WatchState {
  const notes = Object.entries(state.caseNotes).filter(([id]) => id !== caseId);
  return { ...state, caseNotes: Object.fromEntries([...notes, [caseId, note.slice(0, 2000)]].slice(-100)) };
}

// 同じ資料でも日付が変わる更新イベントは再び未読として扱う。
export const feedItemKey = (item: FeedItem) => JSON.stringify([item.kind, item.date, item.href]);
export const matchedKeywords = (item: FeedItem, topic: WatchTopic) => topic.keywords.filter(word =>
  [item.title, item.summary ?? "", item.law_title ?? ""].some(text => text.includes(word)),
);

const plain = (value: string) => value.replace(/[\\`*_{}[\]<>#|]/g, "\\$&");

export function caseComparisonReport(cases: HanreiCase[], notes: Record<string, string>, wikiPaths: ReadonlySet<string> | null, base: string) {
  const root = new URL("./", base).href;
  const lines = ["# 判例の比較メモ", "", `出力日時: ${new Date().toISOString()}`, "",
    "比較対象は利用者が選択した判例です。結論の違いだけでは法的な評価・判例変更を判断できません。",
    "判示事項・裁判要旨・結果・参照法条は裁判所公表データです。未収録は記載なしや争点なしを意味しません。",
    "LLMによる要約・解説はリンク先で表示します。この出力にはAIの解説本文を含めていません。",
    "当時の条文は版候補で、適用版が確定していない場合があります。原文・経過措置を確認してください。", "",
    `比較対象: ${cases.length}件`, ""];
  for (const source of cases) {
    const id = String(source.case_id);
    const path = `hanrei/${id}`;
    lines.push(`## ${plain(source.case_name || source.case_number)}`, "");
    for (const [label, text] of [["事件番号", source.case_number], ["裁判年月日", source.judged_on], ["裁判所", source.court], ["結果", source.result], ["裁判所公表の判示事項", source.holding], ["裁判所公表の裁判要旨", source.summary], ["参照法条（裁判所公表）", source.referenced_laws]]) {
      lines.push(`### ${label}`, "", plain(text?.trim() || "未収録"), "");
    }
    lines.push(`判例・条文の版候補: ${root}#/${path}`, `裁判要旨の確認: ${root}#/${path}?source=summary`, `裁判所の原文・出典: ${hanreiUrl(source.case_id)}`, "");
    if (wikiPaths?.has(path)) lines.push(`LLMによる要約・解説・関連Wiki: ${root}#/wiki/${path}`, "");
    else lines.push(`LLM解説: ${wikiPaths ? "公開済み解説の収録なし" : "収録状況を未確認"}`, "");
    if (notes[id]?.trim()) lines.push("### 自分の比較メモ", "", plain(notes[id]), "");
  }
  return lines.join("\n");
}
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
