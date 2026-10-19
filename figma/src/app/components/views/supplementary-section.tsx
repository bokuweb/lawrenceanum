import { useEffect, useState } from "react";
import { CalendarClock } from "lucide-react";
import {
  api,
  type EnforcementKind,
  type LawSupplementary,
  type SupplementaryEnforcement,
  type SupplementaryProvisionAnalysis,
  type TransitionalKind,
} from "../../data/api";

const KIND_LABEL: Record<EnforcementKind, string> = {
  fixed_date: "確定日",
  promulgation: "公布日",
  after_promulgation: "公布日から起算",
  delegated: "政令等に委任",
  linked: "他の施行日に連動",
  unknown: "読み取れず",
};

const TRANSITIONAL_LABEL: Record<TransitionalKind, string> = {
  prior_law: "なお従前の例",
  penalty: "罰則の経過措置",
  continued_effect: "なお効力を有する",
  deemed: "みなす規定",
  application: "適用区分",
  delegated: "政令への委任",
  review: "検討条項",
  repeal: "廃止",
  other: "その他の経過措置",
};

/** 一覧で最初に見せる附則の数 (新しい改正から)。 */
const INITIAL = 5;

function when(e: SupplementaryEnforcement): string {
  if (e.date) return e.date;
  if (e.deadline) return `${e.deadline} までに${e.delegated_to ?? "政令"}で定める日`;
  if (e.kind === "linked") return e.linked_to ?? e.when;
  return e.when;
}

function truncate(s: string, n: number): string {
  return s.length > n ? `${s.slice(0, n)}…` : s;
}

function Provision({ p }: { p: SupplementaryProvisionAnalysis }) {
  const counts = new Map<TransitionalKind, number>();
  for (const t of p.transitional ?? []) for (const k of t.kinds) counts.set(k, (counts.get(k) ?? 0) + 1);
  const readings = p.readings?.length ?? 0;
  return (
    <li className="text-xs">
      <div className="font-medium">
        {p.amend_law_num ? `改正附則（${p.amend_law_num}）` : "制定時の附則"}
        {p.promulgation_date && <span className="text-muted-foreground font-normal ml-1.5">公布 {p.promulgation_date}</span>}
      </div>
      {p.enforcement_unresolved ? (
        <p className="text-muted-foreground">施行期日を読み取れませんでした</p>
      ) : (
        <ul className="mt-0.5 space-y-0.5">
          {(p.enforcement ?? []).filter(e => e.kind !== "unknown").map((e, i) => (
            <li key={i} className="flex gap-1.5" title={e.text}>
              <span className="text-[10px] px-1 rounded bg-muted text-muted-foreground shrink-0 h-fit">{KIND_LABEL[e.kind]}</span>
              <span className="min-w-0">
                <span className="whitespace-nowrap">{when(e)}</span>
                {e.scope === "partial" && e.targets && (
                  <span className="text-muted-foreground ml-1.5">一部: {truncate(e.targets, 80)}</span>
                )}
                {e.scope === "whole" && e.excluding && (
                  <span className="text-muted-foreground ml-1.5">（{truncate(e.excluding, 60)}）</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      )}
      {(counts.size > 0 || readings > 0) && (
        <div className="flex flex-wrap gap-1 mt-1">
          {[...counts].map(([k, n]) => (
            <span key={k} className="text-[10px] px-1.5 py-0.5 rounded border border-border text-muted-foreground">
              {TRANSITIONAL_LABEL[k]} {n}
            </span>
          ))}
          {readings > 0 && (
            <span className="text-[10px] px-1.5 py-0.5 rounded border border-border text-muted-foreground">読替え {readings}</span>
          )}
        </div>
      )}
    </li>
  );
}

/** 法令詳細の「附則の施行期日・経過措置」。supplementary.json が無ければ何も出さない。 */
export function SupplementarySection({ lawId }: { lawId: string }) {
  const [data, setData] = useState<LawSupplementary | null>(null);
  const [showAll, setShowAll] = useState(false);
  useEffect(() => {
    let cancelled = false;
    setData(null);
    setShowAll(false);
    api.lawSupplementary(lawId)
      .then(d => { if (!cancelled) setData(d); })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [lawId]);
  if (!data || data.provisions.length === 0) return null;

  // 新しい改正の附則から。
  const provisions = [...data.provisions].reverse();
  const shown = showAll ? provisions : provisions.slice(0, INITIAL);
  const s = data.stats;
  return (
    <div className="border-b border-border bg-background px-6 py-3">
      <div className="flex items-center gap-1.5 text-xs font-semibold text-muted-foreground mb-2">
        <CalendarClock className="size-3" />
        附則の施行期日・経過措置 ({s.provisions})
        <span className="font-normal ml-1">
          経過措置 {s.transitional_measures} ・ 読替え {s.readings}
          {s.enforcement_unresolved > 0 && ` ・ 施行期日未解決 ${s.enforcement_unresolved}`}
        </span>
      </div>
      <ul className="space-y-2">
        {shown.map(p => <Provision key={p.index} p={p} />)}
      </ul>
      {provisions.length > INITIAL && (
        <button onClick={() => setShowAll(v => !v)} className="text-[11px] text-primary hover:underline mt-1.5">
          {showAll ? "新しい 5 件だけ表示" : `すべて表示（${provisions.length} 件）`}
        </button>
      )}
      <p className="text-[10px] text-muted-foreground mt-1.5">附則の文言から自動抽出（公布日から計算した日付を含みます）</p>
    </div>
  );
}
