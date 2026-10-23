// OKF の type ごとの表示名と色 (グラフ・バッジ共通)。ライト/ダーク両方で読める中間色。
export const WIKI_TYPES: Record<string, { label: string; color: string }> = {
  law: { label: "法令", color: "#3b82f6" },
  enforcement: { label: "施行予定", color: "#f97316" },
  meeting: { label: "会議", color: "#f59e0b" },
  person: { label: "人物", color: "#10b981" },
  topic: { label: "論点", color: "#a855f7" },
  bill: { label: "議案", color: "#ef4444" },
  committee: { label: "会議体", color: "#06b6d4" },
  // 会派は他の 8 色と色相で競合しないよう、彩度を落とした中間色にする。
  party: { label: "会派", color: "#64748b" },
  pubcomment: { label: "パブコメ", color: "#84cc16" },
  kanpo: { label: "官報", color: "#d946ef" },
  hanrei: { label: "判例", color: "#0f766e" },
};

export const WIKI_TYPE_ORDER = ["law", "hanrei", "enforcement", "topic", "committee", "party", "pubcomment", "bill", "kanpo", "meeting", "person"];

/** wiki ページ (拡張子なし) からの相対リンクを、wiki ルート相対のパス (拡張子なし) に解決する。 */
export function resolveWikiLink(from: string, href: string): string | null {
  const parts = from.split("/");
  parts.pop();
  let target: string;
  try {
    target = decodeURIComponent(href.split("#")[0]);
  } catch {
    target = href.split("#")[0];
  }
  for (const seg of target.split("/")) {
    if (!seg || seg === ".") continue;
    if (seg === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else {
      parts.push(seg);
    }
  }
  return parts.join("/").replace(/\.md$/, "");
}
