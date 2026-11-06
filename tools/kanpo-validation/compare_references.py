#!/usr/bin/env python3
"""Diagnostic comparison of reference RTF table columns with parser output.

Uses macOS textutil to expose the original RTF table structure as HTML. This is
an independent reference reader, not a gold generator: differences remain in the
report, and a matching character stream does not certify table rows or images.
"""
import argparse
import difflib
import hashlib
from html.parser import HTMLParser
import json
import pathlib
import re
import subprocess
import unicodedata


class Node:
    def __init__(self, tag="", children=None):
        self.tag = tag
        self.children = children or []

    def text(self):
        return "".join(x.text() if isinstance(x, Node) else x for x in self.children)

    def descendants(self, tag):
        for child in self.children:
            if isinstance(child, Node):
                if child.tag == tag:
                    yield child
                yield from child.descendants(tag)


class Reader(HTMLParser):
    def __init__(self, html):
        super().__init__(convert_charrefs=True)
        self.root = Node()
        self.stack = [self.root]
        self.feed(html)

    def handle_starttag(self, tag, attrs):
        node = Node(tag)
        self.stack[-1].children.append(node)
        if tag not in {"meta", "br", "hr", "img", "link", "input"}:
            self.stack.append(node)

    def handle_endtag(self, tag):
        for i in range(len(self.stack) - 1, 0, -1):
            if self.stack[i].tag == tag:
                self.stack = self.stack[:i]
                return

    def handle_data(self, data):
        self.stack[-1].children.append(data)


def compact(text):
    # Only explicit composition controls and underline delimiters are omitted.
    # Other PUA glyphs may be significant numbers or operators: keep them visible
    # as differences until the reference encoding has been independently decoded.
    text = re.sub(r"〈組方向コード[^〉]*〉[^\n]*\n", "", text)
    text = re.sub(r"〈継続〉|〈表継続〉", "", text)
    return "".join(c for c in unicodedata.normalize("NFKC", text)
                   if not c.isspace() and c not in "\ue1de\ue1e1")


def publication_headings(lines):
    """Explicit publication identifiers only; no title/context similarity."""
    found = set()
    for line in lines:
        match = re.match(r"^[〇○]([^。\n]{1,100}?第[〇零一二三四五六七八九十百千万0-9]+号)", compact(line).strip())
        if match:
            found.add(match[1])
    return found


def publication_identity(reference, actual):
    """Opaque reference glyphs cannot prove that two publications differ."""
    opaque = lambda heading: any(unicodedata.category(c) == 'Co' or c == '\ufffd' for c in heading)
    decoded_reference = {h for h in reference if not opaque(h)}
    decoded_actual = {h for h in actual if not opaque(h)}
    if decoded_reference & decoded_actual:
        return 'common_heading'
    if any(opaque(h) for h in reference | actual):
        return 'unverified'
    return 'mismatch' if reference and actual else 'unverified'


def reference_fields(path, continuation=False, headings=None):
    result = subprocess.run(["textutil", "-convert", "html", "-stdout", str(path)],
                            check=True, capture_output=True)
    tree = Reader(result.stdout.decode("utf-8")).root
    if headings is not None:
        headings.update(publication_headings(p.text() for p in tree.descendants("p")))
    fields = [[], []]
    warnings = []
    def top_blocks(node):
        for child in node.children:
            if isinstance(child, Node):
                if child.tag in {"table", "p"}:
                    yield child
                else:
                    yield from top_blocks(child)

    def table_rows(node):
        for child in node.children:
            if isinstance(child, Node):
                if child.tag == "tr":
                    yield child
                elif child.tag != "table":
                    yield from table_rows(child)

    for table in top_blocks(tree):
        if table.tag == 'p':
            # A paragraph inside a cell is not a top-level block. Outside a
            # table, real prose ends its comparison context; composition-only
            # or continuation controls do not. Never carry the prior document's
            # header through prose into an ordinary two-column table.
            if compact(table.text() + '\n'):
                continuation = False
            continue
        rows = list(table_rows(table))
        if not rows:
            continue
        first = [c for c in rows[0].children if isinstance(c, Node) and c.tag == "td"]
        header = len(first) == 2 and tuple(compact(c.text()) for c in first) in (
                ("改正後", "改正前"), ("変更後", "変更前"))
        if not header and not continuation:
            warnings.append("Table without a proven comparison header")
            continue
        continuation = True
        for row in rows[1:] if header else rows:
            cells = [c for c in row.children if isinstance(c, Node) and c.tag == "td"]
            if len(cells) != 2:
                # A spanning note belongs outside the two comparison fields.
                # Its content is retained in the original reference, not guessed
                # into either side of this column-only diagnostic.
                warnings.append(f"Row with {len(cells)} physical cells requires merge/field review")
                continue
            for i, cell in enumerate(cells):
                fields[i].append(cell.text())
    return [compact("".join(parts)) for parts in fields], continuation, warnings


def run_text(run):
    if "table" in run:
        return "".join(run_text(r) for row in run["table"]["rows"] for cell in row for r in cell)
    return run["text"]


def document_fields(analysis):
    result = [[], []]
    for article in analysis["articles"]:
        for block in article["document"]["blocks"]:
            if block["kind"] == "shinkyu":
                for row in block["rows"]:
                    for i, field in enumerate(("after", "before")):
                        result[i].extend(run_text(r) for r in row[field])
                        if field + "_table" in row:
                            result[i].append(run_text({"table": row[field + "_table"]}))
    return [compact("".join(parts)) for parts in result]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("root", type=pathlib.Path)
    parser.add_argument("--analysis", default="candidate")
    parser.add_argument("--case", type=int, action="append", dest="cases",
                        help="Compare only the explicitly selected workbook case(s)")
    parser.add_argument("--article", type=int, action="append", dest="articles",
                        help="Zero-based article index; requires one --case and explicit --analysis")
    parser.add_argument("--output", type=pathlib.Path)
    args = parser.parse_args()
    if args.articles is not None and (not args.cases or len(args.cases) != 1 or args.analysis == "candidate"):
        parser.error("--article requires one --case and an explicit --analysis")
    report = []
    for case in json.loads((args.root / "index.json").read_text())["cases"]:
        if args.cases is not None and case["number"] not in args.cases:
            continue
        directory = args.root / f"case-{case['number']:02}"
        refs = [[], []]
        continuation = False
        warnings = []
        reference_headings = set()
        for source in case["sources"]:
            if source["kind"] == "rtf":
                fields, continuation, issues = reference_fields(directory / (source["cell"] + ".rtf"), continuation, reference_headings)
                warnings.extend({"source_cell": source["cell"], "issue": issue} for issue in issues)
                for i in range(2):
                    refs[i].append(fields[i])
        expected = ["".join(f) for f in refs]
        analysis_path=directory / args.analysis / "analysis.json"
        analysis_bytes=analysis_path.read_bytes()
        analysis=json.loads(analysis_bytes)
        selected=list(range(len(analysis["articles"]))) if args.articles is None else args.articles
        if len(set(selected)) != len(selected) or any(i < 0 or i >= len(analysis["articles"]) for i in selected):
            parser.error("article indices must be unique and inside the analysis inventory")
        actual = document_fields({"articles":[analysis["articles"][i] for i in selected]})
        entry = {"case": case["number"], "analysis_path":str(analysis_path),
                 "analysis_sha256":hashlib.sha256(analysis_bytes).hexdigest(),
                 "article_indices":selected, "columns": [], "reference_reader_warnings": warnings}
        actual_headings=publication_headings(line for i in selected for line in analysis["articles"][i]["text"].splitlines())
        entry["reference_headings"]=sorted(reference_headings)
        entry["actual_headings"]=sorted(actual_headings)
        entry["source_identity_status"]=publication_identity(reference_headings, actual_headings)
        if entry["source_identity_status"] == 'mismatch':
            entry["skipped_reason"]="Reference and selected PDF articles have different explicit publication identifiers"
            report.append(entry)
            print(case["number"],"skipped: different reference/PDF publication identifiers",flush=True)
            continue
        for e, a in zip(expected, actual):
            diff = [{"kind": kind, "reference": e[i:j], "actual": a[k:l]}
                    for kind, i, j, k, l in difflib.SequenceMatcher(None, e, a, autojunk=False).get_opcodes()
                    if kind != "equal"]
            entry["columns"].append({"reference_chars": len(e), "actual_chars": len(a),
                                     "exact_stream_match": bool(e) and e == a and not warnings, "differences": diff})
        report.append(entry)
        print(case["number"], [(x["reference_chars"], x["actual_chars"], len(x["differences"])) for x in entry["columns"]], flush=True)
    (args.output or args.root / (args.analysis + "-references.json")).write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
