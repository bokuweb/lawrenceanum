"""e-Gov 法令 API v2 の全文 JSON（egov_check / egov_rows のキャッシュ）から、条・項・号・表の行ごとの本文を取り出す。

    python extract_laws.py .cache/egov-check laws.jsonl

1 行 1 法令（同じ法令の版は最新の 1 つだけ）: {"law_id": ..., "title": ..., "paragraphs": ["第一条　…", "一　…", …]}
"""

import json
import re
import sys
from pathlib import Path

# 1 つの段落として出す要素（中の同じ種類の要素は別の段落）。
BLOCKS = {
    "LawTitle", "EnactStatement", "PartTitle", "ChapterTitle", "SectionTitle", "SubsectionTitle", "DivisionTitle",
    "ArticleCaption", "Paragraph", "Item", "TableRow", "SupplProvisionLabel", "AppdxTableTitle", "AppdxStyleTitle",
    "Remarks", "NoteStruct", "TableStructTitle",
} | {f"Subitem{i}" for i in range(1, 11)}
# 文に含めない要素（ルビの読み・図・様式）。
SKIP = {"Rt", "Fig", "StyleStruct", "FormatStruct", "QuoteStruct"}
# 前後に全角の空白を置く要素（項番号・号の名称と本文の間、表の欄の間、本文の段の間）。
SPACED = {"ParagraphNum", "ItemTitle", "TableColumn", "Column", "ArticleTitle"} | {f"Subitem{i}Title" for i in range(1, 11)}


def text_of(node, out):
    """要素の文（中の BLOCKS の要素は除く）。"""
    if isinstance(node, str):
        out.append(node)
        return
    tag = node.get("tag", "")
    if tag in SKIP:
        return
    for child in node.get("children", []):
        if isinstance(child, dict) and child.get("tag") in BLOCKS:
            continue
        if isinstance(child, dict) and child.get("tag") in SPACED and out and not out[-1].endswith("　"):
            out.append("　")
        text_of(child, out)
        if isinstance(child, dict) and child.get("tag") in SPACED:
            out.append("　")


def blocks(node, out, prefix=""):
    if isinstance(node, str):
        return
    tag = node.get("tag", "")
    if tag in SKIP:
        return
    if tag == "Article":
        # 条の見出し（「（目的）」）は 1 行、条名は最初の項の頭に付ける。
        title = ""
        for child in node.get("children", []):
            if isinstance(child, dict) and child.get("tag") == "ArticleTitle":
                parts: list[str] = []
                text_of(child, parts)
                title = "".join(parts).strip()
        first = True
        for child in node.get("children", []):
            if not isinstance(child, dict) or child.get("tag") == "ArticleTitle":
                continue
            if child.get("tag") == "Paragraph" and first:
                blocks(child, out, title + "　")
                first = False
            else:
                blocks(child, out)
        return
    if tag in BLOCKS:
        parts = []
        text_of(node, parts)
        text = prefix + "".join(parts)
        text = re.sub("　+", "　", text).strip("　 \n")
        if text:
            out.append(text)
    for child in node.get("children", []):
        if isinstance(child, dict) and (child.get("tag") in BLOCKS or tag not in BLOCKS):
            blocks(child, out)


def main():
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    latest: dict[str, Path] = {}
    for path in sorted(src.glob("_law_data_*full_text*.json")):
        law_id = path.name.split("_")[3]
        latest[law_id] = path  # 名前の順で後ろ（施行日の新しい版）
    with dst.open("w") as f:
        for law_id, path in sorted(latest.items()):
            data = json.loads(path.read_text())
            out: list[str] = []
            blocks(data["law_full_text"], out)
            title = data.get("revision_info", {}).get("law_title", "")
            f.write(json.dumps({"law_id": law_id, "title": title, "paragraphs": out}, ensure_ascii=False) + "\n")
    print(f"{len(latest)} laws", file=sys.stderr)


if __name__ == "__main__":
    main()
