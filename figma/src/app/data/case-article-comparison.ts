import type { CaseLawCitation, CaseLawReference, HanreiCase, HanreiLawLinks } from "./api";

export type ComparisonLawState = { data: HanreiLawLinks | null; error: boolean };
export const citationFields = { holding: "判示事項", summary: "裁判要旨", referenced_laws: "参照法条", body_text: "本文" } as const;
export type ComparedArticle = {
  key: string; lawId: string; article: string;
  sides: (null | { law: CaseLawReference; citations: CaseLawCitation[] })[];
};

// Compare law IDs and article numbers, never titles, paragraphs, or revision IDs.
export function compareCaseArticles(cases: HanreiCase[], links: HanreiLawLinks[]): ComparedArticle[] {
  const rows = new Map<string, ComparedArticle>();
  cases.forEach((source, index) => {
    const data = links[index];
    if (!data || data.case_id !== source.case_id) return;
    for (const law of data.laws) {
      if (!law.law_id || !law.title || !Array.isArray(law.articles)) continue;
      for (const article of law.articles) {
        if (typeof article !== "string" || !/^\d+(?:_\d+)*$/.test(article)) continue;
        const key = JSON.stringify([law.law_id, article]);
        const row = rows.get(key) ?? { key, lawId: law.law_id, article, sides: cases.map(() => null) };
        if (!row.sides[index]) {
          const seen = new Set<string>();
          const citations = data.citations.filter(c => {
            if (c.law_id !== law.law_id || c.article !== article || !Object.hasOwn(citationFields, c.field)) return false;
            const text = source[c.field as keyof typeof citationFields];
            const identity = JSON.stringify([c.field, c.start, c.end]);
            if (typeof text !== "string" || !Number.isInteger(c.start) || !Number.isInteger(c.end)
              || c.start < 0 || c.end <= c.start || !c.text || text.slice(c.start, c.end) !== c.text || seen.has(identity)) return false;
            seen.add(identity);
            return true;
          });
          row.sides[index] = { law, citations };
        }
        rows.set(key, row);
      }
    }
  });
  return [...rows.values()].sort((a, b) => Number(b.sides.every(Boolean)) - Number(a.sides.every(Boolean))
    || a.lawId.localeCompare(b.lawId) || a.article.localeCompare(b.article, "ja", { numeric: true }));
}
