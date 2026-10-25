import type { CaseLawReference } from "./api";
import { administrativeAndLaborSupplements } from "./administrative-labor-historical-articles";

// Article-level evidence, deliberately separate from e-Gov's whole-law revisions.
// Research and limits: docs/historical-article-supplements.md.
const sources = {
  annotated: { title: "厚生労働省：生活保護法（第3条・第8条の本文と改正注記）", url: "https://www.mhlw.go.jp/web/t_doc?dataId=82048000&dataType=0" },
  original: { title: "衆議院：昭和25年法律第144号（制定時の第3条・第8条）", url: "https://www.shugiin.go.jp/internet/itdb_housei.nsf/html/houritsu/00719500504144.htm" },
  amendment: { title: "衆議院：平成11年法律第160号（第618条・附則第1条）", url: "https://www.shugiin.go.jp/internet/itdb_housei.nsf/html/housei/h146160.htm" },
  municipal: { title: "徳島市：生活保護法の掲載資料（PDF 3・4ページ）", url: "https://www.city.tokushima.tokushima.jp/shisei/jorei/dokuziriyou.files/01-2_seikatuhogo.pdf#page=3" },
};

export type HistoricalArticleSupplement = {
  lawId: string;
  article: string;
  from: string;
  to: string;
  checkedOn: string;
  caption: string;
  paragraphs: { number: string; text: string }[];
  findings: string[];
  limitations: string[];
  sources: { title: string; url: string }[];
};

const supplements: HistoricalArticleSupplement[] = [
  {
    lawId: "325AC0000000144", article: "3", from: "2013-01-01", to: "2015-12-31", checkedOn: "2026-10-25",
    caption: "最低生活",
    limitations: ["生活扶助の金額を定める保護基準（告示）の当時の内容は、この補完には含みません。"],
    paragraphs: [{ number: "1", text: "この法律により保障される最低限度の生活は、健康で文化的な生活水準を維持することができるものでなければならない。" }],
    findings: [
      "制定時の第3条と厚生労働省掲載の第3条の文言が一致し、同省の第3条には改正注記がありません。徳島市掲載資料の第3条とも照合しました。",
      "この照合に基づき、2013〜2015年について同じ文言を参照できる資料として補完しています。",
    ],
    sources: [sources.original, sources.annotated, sources.municipal],
  },
  {
    lawId: "325AC0000000144", article: "8", from: "2013-01-01", to: "2015-12-31", checkedOn: "2026-10-25",
    caption: "基準及び程度の原則",
    limitations: ["生活扶助の金額を定める保護基準（告示）の当時の内容は、この補完には含みません。"],
    paragraphs: [
      { number: "1", text: "保護は、厚生労働大臣の定める基準により測定した要保護者の需要を基とし、そのうち、その者の金銭又は物品で満たすことのできない不足分を補う程度において行うものとする。" },
      { number: "2", text: "前項の基準は、要保護者の年齢別、性別、世帯構成別、所在地域別その他保護の種類に応じて必要な事情を考慮した最低限度の生活の需要を満たすに十分なものであつて、且つ、これをこえないものでなければならない。" },
    ],
    findings: [
      "厚生労働省の第8条の改正注記は「平一一法一六〇・一部改正」です。同法第618条は本則の「厚生大臣」を「厚生労働大臣」に改め、附則第1条により2001年1月6日に施行されています。",
      "第2項は制定時の文言と照合しました。徳島市掲載資料でも両項の文言と同じ改正注記を確認し、2013〜2015年についてこの文言を補完しています。",
    ],
    sources: [sources.annotated, sources.amendment, sources.original, sources.municipal],
  },
  ...administrativeAndLaborSupplements,
];

function validDate(value: string) {
  return /^\d{4}-\d{2}-\d{2}$/.test(value) && Number.isFinite(Date.parse(value))
    && new Date(value).toISOString().slice(0, 10) === value;
}

export function historicalArticleSupplement(law: CaseLawReference, article: string) {
  if (law.basis !== "unresolved" || law.versions.length || !law.articles.includes(article) || !law.evidence.length) return undefined;
  return supplements.find(s => s.lawId === law.law_id && s.article === article && law.evidence.every(e =>
    validDate(e.from) && validDate(e.to) && e.from <= e.to && e.from >= s.from && e.to <= s.to));
}
