// 構造化改め文（kanpo-parser crate の Document）の描画。法令閲覧の改正履歴と官報パーサ
// playground の両方で使う。
import type { AmendDocument, AmendRun, AmendNestedTable } from "../data/api";
import { cn } from "./ui/utils";

/** Run 列を描画。傍線(下線)付きは <u> で表す（現状は常に false だが将来の傍線抽出に対応）。 */
function renderRuns(runs: AmendRun[]) {
  return runs.map((r, i) => (r.underline ? <u key={i}>{r.text}</u> : <span key={i}>{r.text}</span>));
}

/** 別表（罫線で区切られた表）を入れ子テーブルとして描画する。 */
function NestedTableView({ table }: { table: AmendNestedTable }) {
  return (
    <table className="w-full border-collapse my-1">
      <tbody>
        {table.rows.map((row, ri) => (
          <tr key={ri} className="align-top">
            {row.map((cell, ci) => (
              <td key={ci} className="border border-border/70 px-1.5 py-0.5">
                <pre className="whitespace-pre-wrap font-sans leading-relaxed">{renderRuns(cell)}</pre>
              </td>
            ))}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** 新旧対照表のセル。別表(2D表)があればそれを、無ければテキストを描画する。 */
function renderCell(runs: AmendRun[], table?: AmendNestedTable) {
  if (table && table.rows.length > 0) {
    return <NestedTableView table={table} />;
  }
  return <pre className="whitespace-pre-wrap font-sans leading-relaxed">{renderRuns(runs)}</pre>;
}

/**
 * 構造化改め文(amend_document)を描画する。段落はそのまま、新旧対照表は条（別表）ごとの
 * 行に分けて改正後/改正前を左右対比のテーブルで表示する。
 */
export function AmendDocumentView({ doc, className }: { doc: AmendDocument; className?: string }) {
  return (
    <div className={cn("rounded bg-muted/40 p-2 space-y-2", className ?? "mt-1 max-h-96 overflow-auto")}>
      {doc.blocks.map((b, bi) => {
        if (b.kind === "paragraph") {
          return (
            <pre key={bi} className="text-xs whitespace-pre-wrap font-sans leading-relaxed">
              {renderRuns(b.runs)}
            </pre>
          );
        }
        return (
          <table key={bi} className="w-full text-xs border-collapse table-fixed">
            <thead>
              <tr>
                <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">改正後</th>
                <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">改正前</th>
              </tr>
            </thead>
            <tbody>
              {b.rows.map((r, ri) => (
                <tr key={ri} className="align-top">
                  <td className="border border-border px-2 py-1">{renderCell(r.after, r.after_table)}</td>
                  <td className="border border-border px-2 py-1">{renderCell(r.before, r.before_table)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        );
      })}
    </div>
  );
}
