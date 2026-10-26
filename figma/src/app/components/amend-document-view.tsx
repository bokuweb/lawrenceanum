// 構造化改め文（kanpo-parser crate の Document）の描画。法令閲覧の改正履歴と官報パーサ
// playground の両方で使う。
import type { AmendDocument, AmendRun, AmendNestedTable } from "../data/api";
import { cn } from "./ui/utils";

// Older documents omit labels; new documents preserve the actual source pair.
type ComparisonBlock = Extract<AmendDocument["blocks"][number], { kind: "shinkyu" }> & {
  labels?: { after: string; before: string };
};
function comparisonLabels(block: ComparisonBlock) {
  return block.labels ?? { after: "改正後", before: "改正前" };
}

/** テキストの Run 列を描画。傍線(下線)付きは <u> で表す（現状は常に false だが将来の傍線抽出に対応）。 */
function renderTextRuns(runs: AmendRun[]) {
  return runs.map((r, i) => (r.underline ? <u key={i}>{r.text}</u> : <span key={i}>{r.text}</span>));
}

type RunChunk = { kind: "text"; runs: AmendRun[] } | { kind: "table"; table: AmendNestedTable };

/**
 * Run 列を読み順のまとまりに分ける。表の Run（table）はそれだけで 1 つ、間のテキストの Run は
 * 続く分をまとめる。表の直前・直後の改行は表のブロックが担うので 1 つ落とす。
 */
function chunkRuns(runs: AmendRun[]): RunChunk[] {
  const chunks: RunChunk[] = [];
  for (const r of runs) {
    if (r.table) {
      chunks.push({ kind: "table", table: r.table });
      continue;
    }
    const last = chunks[chunks.length - 1];
    if (last?.kind === "text") last.runs.push(r);
    else chunks.push({ kind: "text", runs: [r] });
  }
  return chunks.flatMap((c, i): RunChunk[] => {
    if (c.kind !== "text") return [c];
    let runs = c.runs;
    if (chunks[i - 1]?.kind === "table" && runs.length) {
      runs = [{ ...runs[0], text: runs[0].text.replace(/^\n/, "") }, ...runs.slice(1)];
    }
    if (chunks[i + 1]?.kind === "table" && runs.length) {
      const l = runs.length - 1;
      runs = [...runs.slice(0, l), { ...runs[l], text: runs[l].text.replace(/\n$/, "") }];
    }
    return runs.some((r) => r.text) ? [{ kind: "text" as const, runs }] : [];
  });
}

/** Run 列を描画する。表の Run は前後のテキストの間に入れ子の表として置く。 */
function RunsView({ runs, className }: { runs: AmendRun[]; className?: string }) {
  const chunks = chunkRuns(runs);
  return (
    <div className={cn("whitespace-pre-wrap break-words leading-relaxed", className)}>
      {chunks.map((c, i) =>
        c.kind === "table" ? <NestedTableView key={i} table={c.table} /> : <div key={i}>{renderTextRuns(c.runs)}</div>,
      )}
    </div>
  );
}

/**
 * 別表（罫線で区切られた表）を入れ子テーブルとして描画する。結合セル（merges）は左上のマスに
 * rowSpan/colSpan を付け、結合で覆われるマスは描かない。
 */
function NestedTableView({ table }: { table: AmendNestedTable }) {
  const spans = new Map<string, { rowspan: number; colspan: number }>();
  const covered = new Set<string>();
  for (const m of table.merges ?? []) {
    spans.set(`${m.row}:${m.col}`, m);
    for (let r = m.row; r < m.row + m.rowspan; r++) {
      for (let c = m.col; c < m.col + m.colspan; c++) {
        if (r !== m.row || c !== m.col) covered.add(`${r}:${c}`);
      }
    }
  }
  return (
    <table className="w-full border-collapse my-1 whitespace-normal">
      <tbody>
        {table.rows.map((row, ri) => (
          <tr key={ri} className="align-top">
            {row.map((cell, ci) => {
              if (covered.has(`${ri}:${ci}`)) return null;
              const span = spans.get(`${ri}:${ci}`);
              return (
                <td
                  key={ci}
                  rowSpan={span && span.rowspan > 1 ? span.rowspan : undefined}
                  colSpan={span && span.colspan > 1 ? span.colspan : undefined}
                  className="border border-foreground/25 px-1.5 py-0.5 min-w-[4em]"
                >
                  <RunsView runs={cell} />
                </td>
              );
            })}
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/**
 * 新旧対照表のセル。旧形式の別表（after_table/before_table）があればそれを、無ければ Run 列
 * （表の Run を含みうる）を描画する。
 */
function renderCell(runs: AmendRun[], table?: AmendNestedTable) {
  if (table && table.rows.length > 0) {
    return <NestedTableView table={table} />;
  }
  return <RunsView runs={runs} />;
}

/**
 * 構造化改め文(amend_document)を描画する。段落はそのまま、新旧対照表は条（別表）ごとの
 * 行に分けて改正後/改正前を左右対比のテーブルで表示する。段落・セルの中の表（Run の table）は
 * 読み順の位置に入れ子の表として描く。
 */
export function AmendDocumentView({ doc, className }: { doc: AmendDocument; className?: string }) {
  return (
    <div className={cn("rounded bg-muted/40 p-2 space-y-2", className ?? "mt-1 max-h-96 overflow-auto")}>
      {doc.blocks.map((b, bi) => {
        if (b.kind === "paragraph") {
          return <RunsView key={bi} runs={b.runs} className="text-xs" />;
        }
        const labels = comparisonLabels(b);
        return (
          <table key={bi} className="w-full text-xs border-collapse table-fixed">
            <thead>
              <tr>
                <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">{labels.after}</th>
                <th className="w-1/2 border border-border bg-background/60 px-2 py-1 text-left font-medium">{labels.before}</th>
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
