export type LawCategory = "民事" | "刑事" | "行政" | "商事" | "労働" | "税務" | "憲法";

export type LawSummary = {
  law_id: string;
  law_num: string;
  title: string;
  category: LawCategory;
  promulgation_date: string;
  effective_date: string;
  last_updated: string;
  status: "current" | "amended" | "scheduled";
  article_count: number;
};

export type Article = {
  article_id: string;
  article_no: string;
  caption?: string;
  paragraphs: { paragraph_no: string; text: string }[];
};

export type LawDocument = LawSummary & {
  revision_id: string;
  articles: Article[];
};

export type TimelineEvent = {
  event_id: string;
  event_type: "enactment" | "partial_amendment" | "full_amendment" | "abolition";
  amending_law_num: string;
  promulgation_date: string;
  effective_date: string;
  status: "effective" | "promulgated_not_yet_effective";
  summary: string;
  kanpo_linked: boolean;
};

export const LAWS: LawSummary[] = [
  { law_id: "129AC0000000089", law_num: "明治二十九年法律第八十九号", title: "民法", category: "民事", promulgation_date: "1896-04-27", effective_date: "1898-07-16", last_updated: "2026-04-01", status: "current", article_count: 1050 },
  { law_id: "140AC0000000045", law_num: "明治四十年法律第四十五号", title: "刑法", category: "刑事", promulgation_date: "1907-04-24", effective_date: "1908-10-01", last_updated: "2026-03-15", status: "current", article_count: 264 },
  { law_id: "322AC0000000049", law_num: "昭和二十二年法律第四十九号", title: "労働基準法", category: "労働", promulgation_date: "1947-04-07", effective_date: "1947-09-01", last_updated: "2026-04-20", status: "amended", article_count: 121 },
  { law_id: "321CONSTITUTION", law_num: "昭和二十一年憲法", title: "日本国憲法", category: "憲法", promulgation_date: "1946-11-03", effective_date: "1947-05-03", last_updated: "1947-05-03", status: "current", article_count: 103 },
  { law_id: "417AC0000000086", law_num: "平成十七年法律第八十六号", title: "会社法", category: "商事", promulgation_date: "2005-07-26", effective_date: "2006-05-01", last_updated: "2026-05-01", status: "amended", article_count: 979 },
  { law_id: "340AC0000000033", law_num: "昭和四十年法律第三十三号", title: "所得税法", category: "税務", promulgation_date: "1965-03-31", effective_date: "1965-04-01", last_updated: "2026-04-01", status: "current", article_count: 243 },
  { law_id: "412AC0000000088", law_num: "平成十二年法律第八十八号", title: "行政手続オンライン化法", category: "行政", promulgation_date: "2000-11-27", effective_date: "2001-04-01", last_updated: "2026-02-10", status: "current", article_count: 67 },
  { law_id: "423AC0000000056", law_num: "平成二十三年法律第五十六号", title: "個人情報保護法", category: "行政", promulgation_date: "2003-05-30", effective_date: "2005-04-01", last_updated: "2026-04-01", status: "amended", article_count: 185 },
  { law_id: "508AC0000000030", law_num: "令和八年法律第三十号", title: "デジタル社会形成基本法改正", category: "行政", promulgation_date: "2026-05-01", effective_date: "2026-06-01", last_updated: "2026-05-01", status: "scheduled", article_count: 42 },
];

export const RECENT_UPDATES = [
  { date: "2026-05-01", count: 12, laws: ["会社法", "デジタル社会形成基本法改正"] },
  { date: "2026-04-20", count: 7, laws: ["労働基準法"] },
  { date: "2026-04-01", count: 23, laws: ["民法", "所得税法", "個人情報保護法"] },
  { date: "2026-03-15", count: 4, laws: ["刑法"] },
  { date: "2026-02-10", count: 9, laws: ["行政手続オンライン化法"] },
];

export const UPDATE_TREND = [
  { month: "2025-12", count: 18 },
  { month: "2026-01", count: 27 },
  { month: "2026-02", count: 22 },
  { month: "2026-03", count: 31 },
  { month: "2026-04", count: 45 },
  { month: "2026-05", count: 12 },
];

export const CATEGORY_DISTRIBUTION: { name: LawCategory; value: number }[] = [
  { name: "民事", value: 184 },
  { name: "刑事", value: 76 },
  { name: "行政", value: 412 },
  { name: "商事", value: 138 },
  { name: "労働", value: 92 },
  { name: "税務", value: 156 },
  { name: "憲法", value: 8 },
];

export const TIMELINE_EVENTS: Record<string, TimelineEvent[]> = {
  "129AC0000000089": [
    { event_id: "e1", event_type: "enactment", amending_law_num: "明治二十九年法律第八十九号", promulgation_date: "1896-04-27", effective_date: "1898-07-16", status: "effective", summary: "民法制定", kanpo_linked: false },
    { event_id: "e2", event_type: "partial_amendment", amending_law_num: "平成二十九年法律第四十四号", promulgation_date: "2017-06-02", effective_date: "2020-04-01", status: "effective", summary: "債権法改正", kanpo_linked: true },
    { event_id: "e3", event_type: "partial_amendment", amending_law_num: "令和八年法律第十二号", promulgation_date: "2026-03-15", effective_date: "2026-04-01", status: "effective", summary: "成年年齢に関する規定の一部改正", kanpo_linked: true },
  ],
};

export const ARTICLES_V1: Article[] = [
  { article_id: "art_1", article_no: "第一条", caption: "基本原則", paragraphs: [
    { paragraph_no: "1", text: "私権は、公共の福祉に適合しなければならない。" },
    { paragraph_no: "2", text: "権利の行使及び義務の履行は、信義に従い誠実に行わなければならない。" },
    { paragraph_no: "3", text: "権利の濫用は、これを許さない。" },
  ]},
  { article_id: "art_2", article_no: "第二条", caption: "解釈の基準", paragraphs: [
    { paragraph_no: "1", text: "この法律は、個人の尊厳と両性の本質的平等を旨として、解釈しなければならない。" },
  ]},
  { article_id: "art_3", article_no: "第三条", caption: "権利能力", paragraphs: [
    { paragraph_no: "1", text: "私権の享有は、出生に始まる。" },
    { paragraph_no: "2", text: "外国人は、法令又は条約の規定により禁止される場合を除き、私権を享有する。" },
  ]},
];

export const ARTICLES_V2: Article[] = [
  { article_id: "art_1", article_no: "第一条", caption: "基本原則", paragraphs: [
    { paragraph_no: "1", text: "私権は、公共の福祉に適合しなければならない。" },
    { paragraph_no: "2", text: "権利の行使及び義務の履行は、信義誠実の原則に従って行わなければならない。" },
    { paragraph_no: "3", text: "権利の濫用は、これを許さない。" },
  ]},
  { article_id: "art_2", article_no: "第二条", caption: "解釈の基準", paragraphs: [
    { paragraph_no: "1", text: "この法律は、個人の尊厳と両性の本質的平等を旨として、解釈しなければならない。" },
  ]},
  { article_id: "art_3", article_no: "第三条", caption: "権利能力", paragraphs: [
    { paragraph_no: "1", text: "私権の享有は、出生に始まる。" },
    { paragraph_no: "2", text: "外国人は、法令又は条約の規定により禁止される場合を除き、私権を享有する。ただし、別段の定めがある場合はこの限りでない。" },
  ]},
  { article_id: "art_3_2", article_no: "第三条の二", caption: "意思能力", paragraphs: [
    { paragraph_no: "1", text: "法律行為の当事者が意思表示をした時に意思能力を有しなかったときは、その法律行為は、無効とする。" },
  ]},
];
