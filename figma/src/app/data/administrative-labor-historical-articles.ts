import type { HistoricalArticleSupplement } from "./historical-article-supplements";

// Bounded article-level transcriptions, not reconstructed whole-law revisions.
// Evidence, commencement dates and exclusions: docs/historical-article-supplements.md.
const administrativeSources = {
  annotated: { title: "厚生労働省：行政事件訴訟法（本文・条ごとの改正注記）", url: "https://www.mhlw.go.jp/web/t_doc?dataId=07235000" },
  original: { title: "衆議院：昭和37年法律第139号（制定時の行政事件訴訟法）", url: "https://www.shugiin.go.jp/internet/itdb_housei.nsf/html/houritsu/04019620516139.htm" },
  amendment: { title: "衆議院：平成16年法律第84号（第4条・第12条・第14条の改正、附則第2条〜第4条）", url: "https://www.shugiin.go.jp/internet/itdb_housei.nsf/html/housei/15920040609084.htm" },
  commencement: { title: "法務省：平成16年改正行政事件訴訟法の概要（施行日：2005年4月1日）", url: "https://www.moj.go.jp/content/000103905.pdf" },
};
const laborSources = {
  annotated: { title: "厚生労働省：労働基準法（本文・条ごとの改正注記）", url: "https://www.mhlw.go.jp/web/t_doc?dataId=73022000&dataType=0&pageNo=1" },
  genderAmendment: { title: "衆議院：平成9年法律第92号（第3条による労働基準法第19条の改正・附則第1条）", url: "https://www.shugiin.go.jp/internet/itdb_housei.nsf/html/houritsu/14019970618092.htm" },
  ministryAmendment: { title: "衆議院：平成11年法律第160号（第590条・附則第1条）", url: "https://www.shugiin.go.jp/internet/itdb_housei.nsf/html/housei/h146160.htm" },
};
const administrative = {
  lawId: "337AC0000000139", from: "2005-04-01", to: "2017-03-31", checkedOn: "2026-10-25",
  limitations: ["参照先の他の条文・別表・民事訴訟の規定を当時の内容に復元するものではありません。平成16年法律第84号附則の経過措置により、改正前の規定が適用される場合があります。"],
};
const labor = {
  lawId: "322AC0000000049", from: "2001-01-06", to: "2016-03-31", checkedOn: "2026-10-25",
  limitations: ["参照先の他の条文・省令・労災保険給付の規定を当時の内容に復元するものではありません。"],
};
const unchangedAdministrative = [
  "制定時の条文と厚生労働省掲載の本文を照合しました。同省の当該条には改正注記がありません。確認対象期間に同じ文言を参照できる資料として補完しています。",
];
const amendedAdministrative = [
  "平成16年法律第84号による改正文と厚生労働省掲載の本文を照合しました。同省の当該条の改正注記は「平一六法八四・一部改正」で、それ以後の改正注記はありません。",
  "法務省の改正概要で2005年4月1日の施行を確認しました。施行前に生じた事項や係属事件、出訴期間については同改正法附則第2条〜第4条も確認してください。",
];

export const administrativeAndLaborSupplements: HistoricalArticleSupplement[] = [
  {
    ...administrative, article: "1", caption: "この法律の趣旨",
    paragraphs: [{ number: "1", text: "行政事件訴訟については、他の法律に特別の定めがある場合を除くほか、この法律の定めるところによる。" }],
    findings: unchangedAdministrative,
    sources: [administrativeSources.original, administrativeSources.annotated],
  },
  {
    ...administrative, article: "4", caption: "当事者訴訟",
    paragraphs: [{ number: "1", text: "この法律において「当事者訴訟」とは、当事者間の法律関係を確認し又は形成する処分又は裁決に関する訴訟で法令の規定によりその法律関係の当事者の一方を被告とするもの及び公法上の法律関係に関する確認の訴えその他の公法上の法律関係に関する訴訟をいう。" }],
    findings: amendedAdministrative,
    sources: [administrativeSources.annotated, administrativeSources.amendment, administrativeSources.commencement],
  },
  {
    ...administrative, article: "7", caption: "この法律に定めがない事項",
    paragraphs: [{ number: "1", text: "行政事件訴訟に関し、この法律に定めがない事項については、民事訴訟の例による。" }],
    findings: unchangedAdministrative,
    sources: [administrativeSources.original, administrativeSources.annotated],
  },
  {
    ...administrative, article: "12", caption: "管轄",
    paragraphs: [
      { number: "1", text: "取消訴訟は、被告の普通裁判籍の所在地を管轄する裁判所又は処分若しくは裁決をした行政庁の所在地を管轄する裁判所の管轄に属する。" },
      { number: "2", text: "土地の収用、鉱業権の設定その他不動産又は特定の場所に係る処分又は裁決についての取消訴訟は、その不動産又は場所の所在地の裁判所にも、提起することができる。" },
      { number: "3", text: "取消訴訟は、当該処分又は裁決に関し事案の処理に当たつた下級行政機関の所在地の裁判所にも、提起することができる。" },
      { number: "4", text: "国又は独立行政法人通則法（平成十一年法律第百三号）第二条第一項に規定する独立行政法人若しくは別表に掲げる法人を被告とする取消訴訟は、原告の普通裁判籍の所在地を管轄する高等裁判所の所在地を管轄する地方裁判所（次項において「特定管轄裁判所」という。）にも、提起することができる。" },
      { number: "5", text: "前項の規定により特定管轄裁判所に同項の取消訴訟が提起された場合であつて、他の裁判所に事実上及び法律上同一の原因に基づいてされた処分又は裁決に係る抗告訴訟が係属している場合においては、当該特定管轄裁判所は、当事者の住所又は所在地、尋問を受けるべき証人の住所、争点又は証拠の共通性その他の事情を考慮して、相当と認めるときは、申立てにより又は職権で、訴訟の全部又は一部について、当該他の裁判所又は第一項から第三項までに定める裁判所に移送することができる。" },
    ],
    findings: amendedAdministrative,
    sources: [administrativeSources.original, administrativeSources.annotated, administrativeSources.amendment, administrativeSources.commencement],
  },
  {
    ...administrative, article: "14", caption: "出訴期間",
    paragraphs: [
      { number: "1", text: "取消訴訟は、処分又は裁決があつたことを知つた日から六箇月を経過したときは、提起することができない。ただし、正当な理由があるときは、この限りでない。" },
      { number: "2", text: "取消訴訟は、処分又は裁決の日から一年を経過したときは、提起することができない。ただし、正当な理由があるときは、この限りでない。" },
      { number: "3", text: "処分又は裁決につき審査請求をすることができる場合又は行政庁が誤つて審査請求をすることができる旨を教示した場合において、審査請求があつたときは、処分又は裁決に係る取消訴訟は、その審査請求をした者については、前二項の規定にかかわらず、これに対する裁決があつたことを知つた日から六箇月を経過したとき又は当該裁決の日から一年を経過したときは、提起することができない。ただし、正当な理由があるときは、この限りでない。" },
    ],
    findings: amendedAdministrative,
    sources: [administrativeSources.annotated, administrativeSources.amendment, administrativeSources.commencement],
  },
  {
    ...administrative, article: "30", caption: "裁量処分の取消し",
    paragraphs: [{ number: "1", text: "行政庁の裁量処分については、裁量権の範囲をこえ又はその濫用があつた場合に限り、裁判所は、その処分を取り消すことができる。" }],
    findings: unchangedAdministrative,
    sources: [administrativeSources.original, administrativeSources.annotated],
  },
  {
    ...labor, article: "19", caption: "解雇制限",
    paragraphs: [
      { number: "1", text: "使用者は、労働者が業務上負傷し、又は疾病にかかり療養のために休業する期間及びその後三十日間並びに産前産後の女性が第六十五条の規定によつて休業する期間及びその後三十日間は、解雇してはならない。ただし、使用者が、第八十一条の規定によつて打切補償を支払う場合又は天災事変その他やむを得ない事由のために事業の継続が不可能となつた場合においては、この限りでない。" },
      { number: "2", text: "前項但書後段の場合においては、その事由について行政官庁の認定を受けなければならない。" },
    ],
    findings: ["厚生労働省の第19条の改正注記は「平九法九二・一部改正」です。平成9年法律第92号第3条による「女子」から「女性」、「但し」から「ただし」への変更を照合しました。同法附則第1条第1号の施行規定も確認し、その改正後である2001年以降を今回の確認対象としています。"],
    sources: [laborSources.annotated, laborSources.genderAmendment],
  },
  {
    ...labor, article: "20", caption: "解雇の予告",
    paragraphs: [
      { number: "1", text: "使用者は、労働者を解雇しようとする場合においては、少くとも三十日前にその予告をしなければならない。三十日前に予告をしない使用者は、三十日分以上の平均賃金を支払わなければならない。但し、天災事変その他やむを得ない事由のために事業の継続が不可能となつた場合又は労働者の責に帰すべき事由に基いて解雇する場合においては、この限りでない。" },
      { number: "2", text: "前項の予告の日数は、一日について平均賃金を支払つた場合においては、その日数を短縮することができる。" },
      { number: "3", text: "前条第二項の規定は、第一項但書の場合にこれを準用する。" },
    ],
    findings: ["厚生労働省掲載の第20条には改正注記がありません。同省掲載の全3項を転記し、今回の確認対象期間に限定して補完しています。"],
    sources: [laborSources.annotated],
  },
  {
    ...labor, article: "26", caption: "休業手当",
    paragraphs: [{ number: "1", text: "使用者の責に帰すべき事由による休業の場合においては、使用者は、休業期間中当該労働者に、その平均賃金の百分の六十以上の手当を支払わなければならない。" }],
    findings: ["厚生労働省掲載の第26条には改正注記がありません。同省掲載本文を転記し、今回の確認対象期間に限定して補完しています。"],
    sources: [laborSources.annotated],
  },
  {
    ...labor, article: "75", caption: "療養補償",
    paragraphs: [
      { number: "1", text: "労働者が業務上負傷し、又は疾病にかかつた場合においては、使用者は、その費用で必要な療養を行い、又は必要な療養の費用を負担しなければならない。" },
      { number: "2", text: "前項に規定する業務上の疾病及び療養の範囲は、厚生労働省令で定める。" },
    ],
    findings: ["厚生労働省の第75条の改正注記は「平一一法一六〇・一部改正」です。同法第590条による「命令」から「厚生労働省令」への変更と、附則第1条による2001年1月6日の施行を確認しました。それ以後の改正注記はありません。"],
    sources: [laborSources.annotated, laborSources.ministryAmendment],
  },
  {
    ...labor, article: "81", caption: "打切補償",
    paragraphs: [{ number: "1", text: "第七十五条の規定によつて補償を受ける労働者が、療養開始後三年を経過しても負傷又は疾病がなおらない場合においては、使用者は、平均賃金の千二百日分の打切補償を行い、その後はこの法律の規定による補償を行わなくてもよい。" }],
    findings: ["厚生労働省掲載の第81条には改正注記がありません。同省掲載本文を転記し、今回の確認対象期間に限定して補完しています。"],
    sources: [laborSources.annotated],
  },
];
