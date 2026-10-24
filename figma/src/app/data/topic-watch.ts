import definitions from "../../../../config/watch-topics.json";
import { hanreiUrl, type FeedItem, type HanreiCase, type WatchTopic } from "./api";

export const watchTopics: WatchTopic[] = definitions;
export const WATCH_STORAGE_KEY = "lawrenceanum:topic-watch:v1";
export type ResearchProject = {
  id: string; title: string; question: string; findings: string; pending: string;
  comparison: string[]; caseNotes: Record<string, string>; updatedAt: string;
};
export const MAX_RESEARCH_PROJECTS = 10;
export type WatchState = { followed: boolean; read: string[]; note: string; comparison: string[]; caseNotes: Record<string, string>; projects: ResearchProject[]; activeProject: string | null };
export const emptyWatchState: WatchState = { followed: false, read: [], note: "", comparison: [], caseNotes: {}, projects: [], activeProject: null };
const validCaseId = (value: unknown): value is string => typeof value === "string" && /^[1-9]\d{0,9}$/.test(value);
export const watchCaseId = (item: FeedItem) => item.kind === "hanrei" && item.internal ? item.href.match(/^\/hanrei\/([1-9]\d{0,9})$/)?.[1] : undefined;
const readComparison = (value: unknown) => Array.isArray(value) ? [...new Set<string>(value.filter(validCaseId))].slice(0, 2) : [];
const readCaseNotes = (value: unknown): Record<string, string> => value && typeof value === "object" && !Array.isArray(value)
  ? Object.fromEntries(Object.entries(value).filter(([id, note]) => validCaseId(id) && typeof note === "string").slice(-100).map(([id, note]) => [id, (note as string).slice(0, 2000)])) : {};

function readProjects(value: unknown): ResearchProject[] {
  if (!Array.isArray(value)) return [];
  const projects: ResearchProject[] = [];
  for (const entry of value) {
    if (!entry || typeof entry.id !== "string" || !/^[a-zA-Z0-9-]{1,64}$/.test(entry.id) || projects.some(project => project.id === entry.id)
      || typeof entry.title !== "string" || !entry.title.trim() || typeof entry.updatedAt !== "string" || Number.isNaN(Date.parse(entry.updatedAt))) continue;
    const comparison = readComparison(entry.comparison);
    const notes = readCaseNotes(entry.caseNotes);
    projects.push({ id: entry.id, title: entry.title.trim().slice(0, 100), comparison,
      caseNotes: notes, updatedAt: new Date(entry.updatedAt).toISOString(),
      question: typeof entry.question === "string" ? entry.question.slice(0, 2000) : "",
      findings: typeof entry.findings === "string" ? entry.findings.slice(0, 4000) : "",
      pending: typeof entry.pending === "string" ? entry.pending.slice(0, 4000) : "" });
    if (projects.length === MAX_RESEARCH_PROJECTS) break;
  }
  return projects;
}

export function readWatchState(): WatchState {
  const raw = localStorage.getItem(WATCH_STORAGE_KEY);
  if (!raw) return emptyWatchState;
  const value = JSON.parse(raw);
  if (value?.version !== 1 || typeof value.followed !== "boolean" || !Array.isArray(value.read) || typeof value.note !== "string") throw new Error("invalid watch state");
  const projects = readProjects(value.projects);
  return { followed: value.followed, read: value.read.filter((id: unknown) => typeof id === "string" && id.length < 2000).slice(-2000), note: value.note.slice(0, 5000),
    comparison: readComparison(value.comparison), caseNotes: readCaseNotes(value.caseNotes), projects,
    activeProject: projects.some(project => project.id === value.activeProject) ? value.activeProject : null };
}

export function withCaseNote<T extends { caseNotes: Record<string, string> }>(state: T, caseId: string, note: string): T {
  const notes = Object.entries(state.caseNotes).filter(([id]) => id !== caseId);
  return { ...state, caseNotes: Object.fromEntries([...notes, [caseId, note.slice(0, 2000)]].slice(-100)) };
}

// 同じ資料でも日付が変わる更新イベントは再び未読として扱う。
export const feedItemKey = (item: FeedItem) => JSON.stringify([item.kind, item.date, item.href]);
export const matchedKeywords = (item: FeedItem, topic: WatchTopic) => topic.keywords.filter(word =>
  [item.title, item.summary ?? "", item.law_title ?? ""].some(text => text.includes(word)),
);

const plain = (value: string) => value.replace(/[\\`*_{}[\]<>#|]/g, "\\$&");

function projectRecordLines(project: ResearchProject) {
  const lines = ["## 利用者の調査記録", "", `案件名: ${plain(project.title)}`, `案件の保存日時: ${project.updatedAt}`, "",
    "以下は利用者が記入した記録です。法的な結論や資料の確認状況をシステムが判定したものではありません。", "表示する資料は再開時に取得します。案件保存時の資料を固定したものではありません。", ""];
  for (const [label, text] of [["調査する問い", project.question], ["確認した出典・箇所", project.findings], ["未確認の点・次に調べること", project.pending]]) {
    lines.push(`### ${label}`, "", plain(text.trim() || "未記入"), "");
  }
  return lines;
}

/** 取得失敗時や比較対象が2件未満でも、利用者の記録は持ち出せる。 */
export function researchProjectReport(project: ResearchProject, base: string) {
  const root = new URL("./", base).href;
  const lines = ["# 調査案件の記録", "", `出力日時: ${new Date().toISOString()}`, "", ...projectRecordLines(project),
    "## 比較対象の確認先", "", "この出力は利用者の記録と確認先リンクです。公式要旨やLLMによる解説本文は含みません。当時の条文は適用版が未確定の候補である場合があります。", "",
    `比較対象: ${project.comparison.length}件`, ""];
  for (const id of project.comparison) {
    lines.push(`### 判例 ${id}`, "", `判例・条文の版候補・関連Wikiの確認先: ${root}#/hanrei/${id}`, `裁判所の原文・出典: ${hanreiUrl(Number(id))}`, "");
    if (project.caseNotes[id]?.trim()) lines.push("自分の比較メモ:", "", plain(project.caseNotes[id]), "");
  }
  const otherNotes = Object.entries(project.caseNotes).filter(([id, note]) => !project.comparison.includes(id) && note.trim());
  if (otherNotes.length) lines.push("## 比較対象から外した判例のメモ", "");
  for (const [id, note] of otherNotes) lines.push(`### 判例 ${id}`, "", `確認先: ${root}#/hanrei/${id}`, "", plain(note), "");
  return lines.join("\n");
}

export function caseComparisonReport(cases: HanreiCase[], notes: Record<string, string>, wikiPaths: ReadonlySet<string> | null, base: string, project?: ResearchProject) {
  const root = new URL("./", base).href;
  const lines = ["# 判例の比較メモ", "", `出力日時: ${new Date().toISOString()}`, "",
    "比較対象は利用者が選択した判例です。結論の違いだけでは法的な評価・判例変更を判断できません。",
    "判示事項・裁判要旨・結果・参照法条は裁判所公表データです。未収録は記載なしや争点なしを意味しません。",
    "LLMによる要約・解説はリンク先で表示します。この出力にはAIの解説本文を含めていません。",
    "当時の条文は版候補で、適用版が確定していない場合があります。原文・経過措置を確認してください。", "",
    `比較対象: ${cases.length}件`, ""];
  if (project) lines.push(...projectRecordLines(project));
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
