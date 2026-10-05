import { cn } from "./ui/utils";

export const EGOV_LAWS_URL = "https://laws.e-gov.go.jp/";
export const EGOV_TERMS_URL = "https://www.e-gov.go.jp/terms";

/** ISO 日時 (UTC) を JST の "YYYY-MM-DD" に整形。YYYY-MM-DD はそのまま返す。 */
export function toJstDate(value: string | undefined | null): string | null {
  if (!value) return null;
  if (/^\d{4}-\d{2}-\d{2}$/.test(value)) return value;
  const d = new Date(value);
  if (isNaN(d.getTime())) return null;
  return new Date(d.getTime() + 9 * 60 * 60 * 1000).toISOString().slice(0, 10);
}

/**
 * e-Gov 法令データの出典表記。e-Gov のコンテンツは公共データ利用規約（第1.0版）
 * に基づき、出典の記載と、加工した場合はその旨・加工者の明示が求められる。
 * `asOf` には「2026-10-04 時点」のように添える日付 (YYYY-MM-DD) を渡す。
 */
export function EgovAttribution({ asOf, className }: { asOf?: string | null; className?: string }) {
  return (
    <p data-testid="egov-attribution" className={cn("text-xs text-muted-foreground leading-relaxed", className)}>
      出典：
      <a className="underline hover:text-foreground" href={EGOV_LAWS_URL} target="_blank" rel="noreferrer">
        e-Gov法令検索
      </a>
      （デジタル庁）のデータを lawrenceanum が加工して作成{asOf ? `（${asOf} 時点）` : ""}。{" "}
      <a className="underline hover:text-foreground" href={EGOV_TERMS_URL} target="_blank" rel="noreferrer">
        公共データ利用規約（第1.0版）
      </a>
    </p>
  );
}
