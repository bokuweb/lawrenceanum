"""補正（kanpo-lm の repair / correct）の評価用のページを作り、補正の結果を採点する。

    # dev.jsonl の崩していない例から、1 行だけを崩したページと崩さないページを作る
    python make_repair_eval.py make data/dev.jsonl eval_pages/ --n 300
    # check の出力（TSV）を採点する
    python make_repair_eval.py score eval_pages/ fixes.tsv
"""

import json
import random
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from gen_data import index, rotate, unit_shift, word_move  # noqa: E402

OPS = {"word_move": word_move, "rotate": rotate, "unit_shift": unit_shift}


def make(dev: str, out: str, n: int, seed: int = 7):
    rng = random.Random(seed)
    rows = [json.loads(l) for l in open(dev)]
    clean = [r for r in rows if not r["ops"] and len(r["text"].split("\n")) >= 3]
    rng.shuffle(clean)
    out_dir = Path(out)
    out_dir.mkdir(parents=True, exist_ok=True)
    truth = []
    for k, r in enumerate(clean[:n]):
        lines = r["text"].split("\n")
        name = f"p{k:04d}.txt"
        entry = {"file": name, "line": None, "original": None, "op": None}
        if rng.random() < 0.6:
            targets = [i for i, l in enumerate(lines) if len(l) >= 15]
            has_unit = [i for i in targets if any(u in lines[i] for u in ("MHz", "kHz", "GHz", "ＭＨｚ", "ｋＨｚ", "ＧＨｚ"))]
            if targets:
                op = "unit_shift" if has_unit and rng.random() < 0.5 else rng.choice(["word_move", "rotate"])
                i = rng.choice(has_unit if op == "unit_shift" else targets)
                one = index([lines[i]])
                if OPS[op](one, rng):
                    entry = {"file": name, "line": i, "original": lines[i], "op": op}
                    lines[i] = "".join(c for c, _ in one[0])
        (out_dir / name).write_text("\n".join(lines) + "\n")
        truth.append(entry)
    Path(str(out_dir) + ".truth.jsonl").write_text("".join(json.dumps(t, ensure_ascii=False) + "\n" for t in truth))
    print(f"{len(truth)} pages, {sum(t['line'] is not None for t in truth)} corrupted")


def score(pages: str, fixes: str):
    truth = {}
    for l in open(str(Path(pages)) + ".truth.jsonl"):
        t = json.loads(l)
        truth[t["file"]] = t
    fixed = {}
    for l in open(fixes):
        f = l.rstrip("\n").split("\t")
        if len(f) >= 6:
            fixed.setdefault(f[0], []).append((int(f[1]), f[2], f[5]))
    by_op = {}
    wrong_clean = 0
    for name, t in truth.items():
        got = fixed.get(name, [])
        if t["line"] is None:
            wrong_clean += len(got)
            continue
        s = by_op.setdefault(t["op"], {"n": 0, "exact": 0, "wrong": 0, "missed": 0, "other_lines": 0})
        s["n"] += 1
        mine = [g for g in got if g[0] == t["line"]]
        s["other_lines"] += len(got) - len(mine)
        if not mine:
            s["missed"] += 1
        elif mine[0][2] == t["original"]:
            s["exact"] += 1
        else:
            s["wrong"] += 1
    clean_pages = sum(t["line"] is None for t in truth.values())
    print(f"崩していないページ {clean_pages}: 書き換え {wrong_clean} 行")
    for op, s in sorted(by_op.items()):
        print(f"{op}: {s['n']} 行 正しく直した {s['exact']} 誤って書き換え {s['wrong']} 直さず {s['missed']} 別の行の書き換え {s['other_lines']}")


if __name__ == "__main__":
    if sys.argv[1] == "make":
        n = int(sys.argv[sys.argv.index("--n") + 1]) if "--n" in sys.argv else 300
        make(sys.argv[2], sys.argv[3], n)
    else:
        score(sys.argv[2], sys.argv[3])
