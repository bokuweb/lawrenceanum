import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { AmendDocumentView } from "../src/app/components/amend-document-view";
import sourceCashFlow from "../../crates/kanpo-parser/tests/data/native-numeric-frames/renderer-document.json";

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
