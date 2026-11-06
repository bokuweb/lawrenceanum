#!/usr/bin/env python3
"""Import the supplied case index and optionally fetch its public source files.

This is validation tooling only. Workbook prose is data, never executable input.
The parser neither reads this index nor uses the reference RTFs at runtime.
"""

import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import re
import urllib.request
import xml.etree.ElementTree as ET
import zipfile

NS = {"s": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
RID = "{http://schemas.openxmlformats.org/officeDocument/2006/relationships}id"


def read_cases(path):
    with zipfile.ZipFile(path) as archive:
        strings = ["".join(node.itertext()) for node in ET.fromstring(
            archive.read("xl/sharedStrings.xml")).findall("s:si", NS)]
        sheet = ET.fromstring(archive.read("xl/worksheets/sheet1.xml"))
        rels = {node.get("Id"): node.get("Target") for node in ET.fromstring(
            archive.read("xl/worksheets/_rels/sheet1.xml.rels"))}
        links = {node.get("ref"): rels[node.get(RID)]
                 for node in sheet.findall(".//s:hyperlink", NS)}
        cases = []
        current = None
        for row in sheet.findall(".//s:sheetData/s:row", NS):
            cells = {}
            for cell in row:
                value = cell.find("s:v", NS)
                if value is not None:
                    cells[re.sub(r"\d+$", "", cell.get("r"))] = (
                        strings[int(value.text)] if cell.get("t") == "s" else value.text)
            if cells.get("A") not in (None, "No."):
                current = {"number": int(float(cells["A"])), "row": int(row.get("r")),
                           "metadata": cells, "sources": []}
                cases.append(current)
            if current is None:
                continue
            for cell in row:
                address = cell.get("r")
                column = re.sub(r"\d+$", "", address)
                if address in links and column in ("N", "O", "Q"):
                    current["sources"].append({
                        "cell": address, "kind": {"N": "pdf", "O": "annotated_pdf", "Q": "rtf"}[column],
                        "name": cells.get(column, ""), "url": links[address]})
        return cases


def fetch_source(job):
    directory, source = job
    # Only follow recognized public document IDs; do not interpret cell contents.
    match = re.search(r"/(?:file/)?d/([\w-]+)", source["url"])
    if not match:
        match = re.search(r"[?&]id=([\w-]+)", source["url"])
    if not match:
        raise ValueError(f"Unsupported source URL: {source['url']}")
    extension = ".rtf" if source["kind"] == "rtf" else ".pdf"
    name = source["cell"] + extension
    path = directory / name
    if not path.exists():
        url = "https://drive.google.com/uc?export=download&id=" + match[1]
        with urllib.request.urlopen(url, timeout=60) as response:
            data = response.read()
        path.write_bytes(data)
    data = path.read_bytes()
    signature = b"{\\rtf" if extension == ".rtf" else b"%PDF-"
    if not data.startswith(signature):
        raise ValueError(f"Unexpected download type at {source['cell']}")
    source.update(path=str(path), sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
    return source["cell"]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("workbook", type=pathlib.Path)
    parser.add_argument("output", type=pathlib.Path)
    parser.add_argument("--fetch", action="store_true")
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    cases = read_cases(args.workbook)
    errors = []
    if args.fetch:
        jobs = []
        for case in cases:
            directory = args.output / f"case-{case['number']:02}"
            directory.mkdir(exist_ok=True)
            jobs.extend((directory, source) for source in case["sources"])
        with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
            futures = {pool.submit(fetch_source, job): job[1] for job in jobs}
            for future in concurrent.futures.as_completed(futures):
                try:
                    future.result()
                except Exception as error:
                    errors.append({"cell": futures[future]["cell"], "error": str(error)})
    result = {"workbook_sha256": hashlib.sha256(args.workbook.read_bytes()).hexdigest(),
              "cases": cases, "download_errors": errors}
    (args.output / "index.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"cases": len(cases), "sources": sum(len(c["sources"]) for c in cases),
                      "errors": errors}, ensure_ascii=False), flush=True)
    if errors:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
