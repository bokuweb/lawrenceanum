import { test, expect } from "@playwright/test";
import { buildFtsMatch, buildFtsMatchExpanded, synonymExpansions, tokenize, unbigramSnippet } from "../src/app/data/search-query";

test("検索語を引用し、記号やFTS演算子を入力として扱う", () => {
  expect(buildFtsMatch('AND OR NOT "test" +郵便')).toBe('"and" "or" "not" "test" "郵便"');
  expect(buildFtsMatch("法 A 😀")).toBe("");
  expect(tokenize("東京駅 ABC123")).toEqual(["東京", "京駅", "abc123"]);
});

test("同義語のない検索は元の条件を維持する", () => {
  expect(buildFtsMatchExpanded("郵便 zzcontext")).toBe(buildFtsMatch("郵便 zzcontext"));
});

test("同義語の前後の条件もANDで維持する", () => {
  const expanded = buildFtsMatchExpanded("zzbefore BIS規制 zzafter");
  expect(expanded).toBe(`(("zzbefore")) AND ((${buildFtsMatch("BIS規制")}) OR (${buildFtsMatch("自己資本比率規制")}) OR (${buildFtsMatch("バーゼル規制")})) AND (("zzafter"))`);
  expect(synonymExpansions("BIS規制 zzafter")).toEqual(["自己資本比率規制", "バーゼル規制"]);
});

test("複数の用語はそれぞれの同義語を展開する", () => {
  const expanded = buildFtsMatchExpanded("BIS規制 GMP");
  expect(expanded.split(" AND ")).toHaveLength(2);
  expect(expanded).toContain(buildFtsMatch("バーゼル規制"));
  expect(expanded).toContain(buildFtsMatch("適正製造規範"));
});

test("入れ子の表記は最長一致にし、表示する同義語と検索を揃える", () => {
  const query = "HACCPに沿った衛生管理 zzafter";
  expect(buildFtsMatchExpanded(query).split(" AND ")).toHaveLength(2);
  expect(synonymExpansions(query)).toEqual(["HACCP", "危害分析重要管理点"]);
});

test("検索スニペットはハイライトと省略位置を保って復元する", () => {
  expect(unbigramSnippet("第三 三十 十一 一条 <mark>民法</mark> 法施 施行...hello world")).toBe("第三十一条<mark>民法</mark>施行...hello world");
});
