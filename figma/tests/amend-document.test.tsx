import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { AmendDocumentView } from "../src/app/components/amend-document-view";
import sourceCashFlow from "../../crates/kanpo-parser/tests/data/native-numeric-frames/renderer-document.json";
import sourceProfitMatrix from "../../crates/kanpo-parser/tests/data/native-numeric-matrix/profit-document152.json";
import sourceStatementMatrix from "../../crates/kanpo-parser/tests/data/native-numeric-matrix/statement-document152.json";
import sourceProfitRows from "../../crates/kanpo-parser/tests/data/native-numeric-matrix/profit-source-6rows152.json";
import sourceStatementRows from "../../crates/kanpo-parser/tests/data/native-numeric-matrix/statement-source-15rows152.json";

import sourceCompoundDocument from "../../crates/kanpo-parser/tests/data/native-numeric-parents/expenses-document153.json";
import sourceCompoundRows from "../../crates/kanpo-parser/tests/data/native-numeric-parents/expenses-source-74rows153.json";
import sourceRateDocuments from "../../crates/kanpo-parser/tests/data/native-numeric-labels/rates-documents155.json";
import sourceRateRows from "../../crates/kanpo-parser/tests/data/native-numeric-labels/rates-source40rows155.json";

const block = (labels?: { after: string; before: string }) => ({
  kind: "shinkyu" as const,
  rows: [{ after: [{ text: "新しい本文" }], before: [{ text: "元の本文" }] }],
  ...(labels ? { labels } : {}),
});
const render = (blocks: ReturnType<typeof block>[]) =>
  renderToStaticMarkup(<AmendDocumentView doc={{ format: "shinkyu", blocks }} />);

test("the actual shared renderer retains source change labels", () => {
  const html = render([block({ after: "変更後", before: "変更前" })]);
  assert.match(html, /<th[^>]*>変更後<\/th>/);
  assert.match(html, /<th[^>]*>変更前<\/th>/);
  assert.doesNotMatch(html, /<th[^>]*>改正[後前]<\/th>/);
});
test("older documents keep the canonical labels", () => {
  const html = render([block()]);
  assert.match(html, /<th[^>]*>改正後<\/th>/);
  assert.match(html, /<th[^>]*>改正前<\/th>/);
});
test("declared metadata controls headers without wording dispatch", () => {
  const html = render([block({ after: "修正後", before: "修正前" })]);
  assert.match(html, /<th[^>]*>修正後<\/th>/);
  assert.match(html, /<th[^>]*>修正前<\/th>/);
});
test("mixed families retain independent headers and cells", () => {
  const html = render([block(), block({ after: "変更後", before: "変更前" })]);
  for (const label of ["改正後", "改正前", "変更後", "変更前"]) {
    assert.equal(html.split(`>${label}</th>`).length - 1, 1);
  }
  assert.equal(html.split("新しい本文").length - 1, 2);
  assert.equal(html.split("元の本文").length - 1, 2);
});

test("native source financial rows retain their own amounts in the shared renderer", () => {
  const doc = sourceCashFlow as React.ComponentProps<typeof AmendDocumentView>["doc"];
  const html = renderToStaticMarkup(<AmendDocumentView doc={doc} />);
  assert.equal((html.match(/<table\b/g) ?? []).length, 3);
  const rows = [...html.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/g)].map((match) =>
    [...match[1].matchAll(/<td\b[^>]*>([\s\S]*?)<\/td>/g)].map((cell) => cell[1].replace(/<[^>]+>/g, "")),
  );
  assert.equal(rows.length, 38 + 24 + 5);
  for (const [label, amount] of [
    ["買取債権の取得による支出", "△973,990,240,000"],
    ["小計", "744,943,589,967"],
    ["業務活動によるキャッシュ・フロー", "604,181,379,501"],
    ["有形固定資産の売却による収入", "2,794,245,000"],
  ]) {
    assert.equal(rows.filter((row) => row[0] === label && row[1] === amount).length, 1);
  }
});


const nativeDocumentRows = (doc: React.ComponentProps<typeof AmendDocumentView>["doc"]) => {
  const html = renderToStaticMarkup(<AmendDocumentView doc={doc} />);
  const rows = [...html.matchAll(/<tr\b[^>]*>([\s\S]*?)<\/tr>/g)].map((match) =>
    [...match[1].matchAll(/<td\b[^>]*>([\s\S]*?)<\/td>/g)].map((cell) => cell[1].replace(/<[^>]+>/g, "").replace(/\s/g, "")),
  );
  return { html, rows };
};
test("the backend source profit matrix keeps its blank amount column in the shared renderer", () => {
  const doc = sourceProfitMatrix as React.ComponentProps<typeof AmendDocumentView>["doc"];
  const { html, rows } = nativeDocumentRows(doc);
  assert.equal((html.match(/<table\b/g) ?? []).length, 3);
  assert.equal(rows.length, 16 + 6 + 41);
  const first = rows.findIndex((row) => row.length === 3 && row[0] === "Ⅰ処分対象利益");
  assert.ok(first >= 0);
  assert.deepEqual(rows.slice(first, first + 6), sourceProfitRows);
});
test("the backend source statement retains three amount columns and the next independent matrix", () => {
  const doc = sourceStatementMatrix as React.ComponentProps<typeof AmendDocumentView>["doc"];
  const { html, rows } = nativeDocumentRows(doc);
  assert.equal((html.match(/<table\b/g) ?? []).length, 2);
  assert.equal(rows.length, 15 + 4);
  assert.deepEqual(rows.slice(0, 15), sourceStatementRows);
  assert.equal(rows[15].length, 3);
  assert.deepEqual(rows[15], ["Ⅰ当期未処分利益", "", "0"]);
  assert.deepEqual(rows[18], ["積立金", "0", "0"]);
});

test("the backend continuation keeps all74rows in one complete four-column matrix", () => {
  const doc = sourceCompoundDocument as React.ComponentProps<typeof AmendDocumentView>["doc"];
  const { html, rows } = nativeDocumentRows(doc);
  assert.equal((html.match(/<table\b/g) ?? []).length, 1);
  assert.deepEqual(rows, sourceCompoundRows);
});


test("the actual backend documents keep every source year label with its own rate", () => {
  const results = sourceRateDocuments.map((document) => nativeDocumentRows(document as React.ComponentProps<typeof AmendDocumentView>["doc"]));
  assert.equal(results.reduce((count, result) => count + (result.html.match(/<table\b/g) ?? []).length, 0), 22);
  const rows = results.flatMap((result) => result.rows).filter((row) => row.length === 2);
  assert.deepEqual(rows, sourceRateRows.rows.map((row) => [row.label.replace(/\s/g, ""), row.amount]));
});

import sourceTerminalDocuments from "../../crates/kanpo-parser/tests/data/native-terminal-glyphs/documents156.json";
import sourceTerminalRows from "../../crates/kanpo-parser/tests/data/native-terminal-glyphs/manual-source117rows156.json";

test("actual backend terminal columns retain all117source rows and empty heading amounts", () => {
  const results = sourceTerminalDocuments.map((document) => nativeDocumentRows(document as React.ComponentProps<typeof AmendDocumentView>["doc"]));
  assert.equal(results.reduce((count, result) => count + (result.html.match(/<table\b/g) ?? []).length, 0), 4);
  const rows = results.flatMap((result) => result.rows);
  assert.deepEqual(rows, sourceTerminalRows.rows.map((row) => [row.label.replace(/\s/g, ""), row.amount]));
  assert.equal(rows.filter((row) => row[1] === "").length, 9);
});
