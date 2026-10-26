import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { strict as assert } from "node:assert";
import { test } from "node:test";
import { AmendDocumentView } from "../src/app/components/amend-document-view";

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
