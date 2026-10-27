"""Reconstruct complete gold from the frozen baseline and reviewed source cuts.

This script never calls the parser or its new boundary detector. Paragraph runs
are split literally; all other AST objects, format labels and page data survive.
"""
from pathlib import Path
import copy
import hashlib
import json


def canonical(value):
    data = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(data.encode()).hexdigest()


def split_document(document, anchor):
    blocks = document["blocks"]
    for bi, block in enumerate(blocks):
        if block["kind"] != "paragraph":
            continue
        for ri, run in enumerate(block["runs"]):
            if "table" in run or anchor not in run["text"]:
                continue
            offset = run["text"].index(anchor)
            left_runs = copy.deepcopy(block["runs"][:ri])
            right_runs = copy.deepcopy(block["runs"][ri + 1:])
            if offset:
                prefix = copy.deepcopy(run)
                prefix["text"] = run["text"][:offset]
                left_runs.append(prefix)
            suffix = copy.deepcopy(run)
            suffix["text"] = run["text"][offset:]
            right_runs.insert(0, suffix)
            left_blocks = copy.deepcopy(blocks[:bi])
            right_blocks = copy.deepcopy(blocks[bi + 1:])
            if left_runs:
                prefix = copy.deepcopy(block)
                prefix["runs"] = left_runs
                left_blocks.append(prefix)
            suffix = copy.deepcopy(block)
            suffix["runs"] = right_runs
            right_blocks.insert(0, suffix)
            left, right = copy.deepcopy(document), copy.deepcopy(document)
            left["blocks"], right["blocks"] = left_blocks, right_blocks
            return left, right
    raise AssertionError(("source heading absent from old plain runs", anchor))


def construct(before, cuts):
    for cut in cuts:
        page = before["pages"][cut["page"]]
        region = page["regions"][cut["region"]]
        line = region["lines"][cut["line"]]
        assert region["role"] == "body" and "table" not in line
        assert line["words"] == cut["words"] and line["text"] == cut["source_line"]
        assert all(page["words"][i]["id"] == i for i in cut["words"])
        text = before["articles"][cut["old_article"]]["text"]
        offset = cut["old_article_char_offset"]
        assert offset > 0 and text[offset - 1] == "\n"
        assert text[offset:offset + len(line["text"])] == line["text"]
        end = offset + len(line["text"])
        assert end == len(text) or text[end] == "\n"
        context = cut.get("source_context", [line["text"]])
        assert context == [row["text"] for row in region["lines"][
            cut["line"]:cut["line"] + len(context)]]
        context_text = "\n".join(context)
        assert text[offset:offset + len(context_text)] == context_text
    expected = copy.deepcopy(before)
    articles = []
    for ai, article in enumerate(before["articles"]):
        selected = sorted((c for c in cuts if c["old_article"] == ai),
                          key=lambda c: c["old_article_char_offset"])
        if not selected:
            articles.append(copy.deepcopy(article))
            continue
        assert set(article) == {"text", "document"}
        remaining = copy.deepcopy(article["document"])
        start = 0
        for cut in selected:
            offset = cut["old_article_char_offset"]
            text = article["text"][start:offset].rstrip("\n")
            left, remaining = split_document(remaining, cut["document_anchor"])
            articles.append({"text": text, "document": left})
            start = offset
        articles.append({"text": article["text"][start:], "document": remaining})
    expected["articles"] = articles
    assert "\n".join(a["text"] for a in articles) == "\n".join(
        a["text"] for a in before["articles"])
    return expected


if __name__ == "__main__":
    root = Path(__file__).resolve().parent
    crate = root.parents[2]
    index = json.loads((root / "index.json").read_text())
    for case in index["cases"]:
        for pdf in case["pdfs"]:
            assert hashlib.sha256((crate / pdf["path"]).read_bytes()).hexdigest() == pdf["sha256"]
        before = json.loads((root / case["baseline"]).read_text())
        assert canonical(before) == case["baseline_Analysis_sha256"]
        expected = construct(before, case["cuts"])
        assert canonical(expected) == case["expected_Analysis_sha256"]
        assert expected == json.loads((root / case["expected"]).read_text()), case["key"]
    print(f"{len(index['cases'])} complete source-constructed Analysis fixtures verified")
