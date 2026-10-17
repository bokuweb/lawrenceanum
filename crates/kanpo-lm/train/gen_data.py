"""読み順の崩れを当てる学習データを作る。

正しい本文（e-Gov の法令の段落を縦書きの列の幅で折り返した行、蓄積した官報の今の本文の行）に、パーサの失敗に似た崩しを
入れる。各字に元の本文の中の位置を持たせておき、直前の字（空白・改行は飛ばす）と元の位置がつながらない字を「切れ目」と
する。崩し方ごとにラベルの付け方を決めなくてよい。

    python gen_data.py --laws laws.jsonl [--kanpo <textdump の出力> --exclude 除くページの一覧] --out data/ --n 120000

出力: data/{train,dev}.jsonl（1 行 1 例: {"text": "…", "breaks": [字の位置, …], "ops": [崩し方, …], "src": "law:…|kanpo:…"}）。
dev は法令・ページ単位で分ける（同じ法令の段落が train と dev に入らない）。
"""

import argparse
import json
import random
import re
from pathlib import Path

UNITS = ["MHz", "kHz", "GHz", "Hz", "mW", "kW", "dB", "mm", "cm", "km", "kg", "mg", "mL",
         "ＭＨｚ", "ｋＨｚ", "ＧＨｚ", "Ｈｚ", "ｍＷ", "ｋＷ", "ｄＢ", "ｍｍ", "ｃｍ", "ｋｍ", "ｋｇ", "ｍｇ"]
WS = {" ", "　", "\n", "\t"}

# 1 字 = (字, 元の位置 or None)。空白は位置を持たない。
Line = list[tuple[str, int | None]]


def to_half(text: str) -> str:
    """全角の英数字を半角に（官報の出力は英字・数字が半角のことが多い）。"""
    return "".join(chr(ord(c) - 0xFEE0) if "Ａ" <= c <= "Ｚ" or "ａ" <= c <= "ｚ" or "０" <= c <= "９" else c for c in text)


def wrap(paragraphs: list[str], width: int) -> list[str]:
    """段落を縦書きの列のように width 字で折り返す（句読点・括弧閉じの行頭は前の行にぶら下げる）。"""
    lines = []
    for p in paragraphs:
        while len(p) > width:
            cut = width
            while cut < len(p) and p[cut] in "、。）」』，．":
                cut += 1
            lines.append(p[:cut])
            p = p[cut:]
        if p:
            lines.append(p)
    return lines


def index(lines: list[str]) -> list[Line]:
    out, k = [], 0
    for line in lines:
        cur: Line = []
        for c in line:
            if c in WS:
                cur.append((c, None))
            else:
                cur.append((c, k))
                k += 1
        out.append(cur)
    return out


def text_of(line: Line) -> str:
    return "".join(c for c, _ in line)


# ---- 崩し方 ----

def reverse_merge(lines: list[Line], rng: random.Random) -> bool:
    """縦組みの列を左から順に 1 行につなぐ（横組みのページの中の縦組みの公告）。"""
    if len(lines) < 3:
        return False
    k = rng.randint(2, min(8, len(lines)))
    i = rng.randrange(0, len(lines) - k + 1)
    merged: Line = [ch for line in reversed(lines[i:i + k]) for ch in line]
    lines[i:i + k] = [merged]
    return True


def swap_lines(lines: list[Line], rng: random.Random) -> bool:
    """隣の列の順を取り違える。"""
    if len(lines) < 2:
        return False
    i = rng.randrange(0, len(lines) - 1)
    lines[i], lines[i + 1] = lines[i + 1], lines[i]
    return True


def fragment_move(lines: list[Line], rng: random.Random) -> bool:
    """行の断片（2〜8 字）を近くの別の行へ移す（他の行の断片の混入）。"""
    if len(lines) < 2:
        return False
    a = rng.randrange(len(lines))
    if len(lines[a]) < 6:
        return False
    n = rng.randint(2, min(8, len(lines[a]) - 2))
    p = rng.randrange(0, len(lines[a]) - n + 1)
    frag = lines[a][p:p + n]
    del lines[a][p:p + n]
    b = min(len(lines) - 1, max(0, a + rng.choice([-3, -2, -1, 1, 2, 3])))
    if b == a:
        b = (a + 1) % len(lines)
    q = rng.randint(0, len(lines[b]))
    lines[b][q:q] = frag
    return True


def word_move(lines: list[Line], rng: random.Random) -> bool:
    """行の中の 1〜4 字を 2〜8 字ずらす。"""
    a = rng.randrange(len(lines))
    line = lines[a]
    if len(line) < 10:
        return False
    n = rng.randint(1, 4)
    p = rng.randrange(0, len(line) - n)
    frag = line[p:p + n]
    rest = line[:p] + line[p + n:]
    q = min(len(rest), max(0, p + rng.choice([-1, 1]) * rng.randint(2, 8)))
    if q == p:
        return False
    lines[a] = rest[:q] + frag + rest[q:]
    return True


def rotate(lines: list[Line], rng: random.Random) -> bool:
    """行の前後を入れ替える。"""
    a = rng.randrange(len(lines))
    line = lines[a]
    if len(line) < 6:
        return False
    p = rng.randint(2, len(line) - 2)
    lines[a] = line[p:] + line[:p]
    return True


def unit_shift(lines: list[Line], rng: random.Random) -> bool:
    """縦中横の単位が 1 字以上右へずれ、元の位置に空白が残る（「一・四　のものMHz」）。"""
    found = []
    for a, line in enumerate(lines):
        t = text_of(line)
        for u in UNITS:
            for m in re.finditer(re.escape(u), t):
                found.append((a, m.start(), len(u)))
    if not found:
        return False
    a, p, n = rng.choice(found)
    line = lines[a]
    unit = line[p:p + n]
    rest = line[:p] + [("　", None)] + line[p + n:]
    q = len(rest) if rng.random() < 0.5 else min(len(rest), p + 1 + rng.randint(1, 3))
    lines[a] = rest[:q] + unit + rest[q:]
    return True


def detach(lines: list[Line], rng: random.Random) -> bool:
    """行の中の 1〜3 字（上付きの数など）が抜けて、別の行になる。"""
    a = rng.randrange(len(lines))
    line = lines[a]
    if len(line) < 8:
        return False
    digits = [i for i, (c, _) in enumerate(line) if c.isdigit() or c in "－−+＋"]
    p = rng.choice(digits) if digits and rng.random() < 0.7 else rng.randrange(1, len(line) - 3)
    n = rng.randint(1, 3)
    frag = line[p:p + n]
    del line[p:p + n]
    lines.insert(a + rng.choice([0, 1]), frag)
    return True


OPS = [(reverse_merge, 3), (swap_lines, 2), (fragment_move, 3), (word_move, 2), (rotate, 2), (unit_shift, 2), (detach, 1)]


def corrupt(lines: list[Line], rng: random.Random) -> list[str]:
    applied = []
    # 単位を含むまとまりは少ないので、あれば半分はずらす。
    if rng.random() < 0.5 and unit_shift(lines, rng):
        applied.append("unit_shift")
    for _ in range(rng.choice([1, 1, 1, 2, 2, 3])):
        for _ in range(5):
            op = rng.choices([o for o, _ in OPS], weights=[w for _, w in OPS])[0]
            if op(lines, rng):
                applied.append(op.__name__)
                break
    return applied


def labels(lines: list[Line]) -> tuple[str, list[int]]:
    text, breaks, prev = [], [], None
    pos = 0
    for i, line in enumerate(lines):
        if i > 0:
            text.append("\n")
            pos += 1
        for c, src in line:
            if src is not None:
                if prev is not None and src != prev + 1:
                    breaks.append(pos)
                prev = src
            text.append(c)
            pos += len(c)
    return "".join(text), breaks


def examples(lines: list[str], max_chars: int, rng: random.Random):
    """行を max_chars 字ほどのまとまりに分ける。"""
    i = 0
    while i < len(lines):
        size, j = 0, i
        limit = rng.randint(max_chars // 2, max_chars)
        while j < len(lines) and (j == i or size + len(lines[j]) <= limit):
            size += len(lines[j])
            j += 1
        yield lines[i:j]
        i = j


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--laws", required=True)
    ap.add_argument("--kanpo")
    ap.add_argument("--exclude", help="kanpo から除くページ（評価に使うもの）の一覧")
    ap.add_argument("--out", required=True)
    ap.add_argument("--max-chars", type=int, default=360)
    ap.add_argument("--clean", type=float, default=0.3, help="崩さない例の割合")
    ap.add_argument("--kanpo-rate", type=float, default=0.4, help="官報のページを使う割合（今の出力にも崩れが残るので減らす）")
    ap.add_argument("--kanpo-flags", help="breaks の出力（TSV）。確率 0.9 以上の行を含む官報のまとまりは使わない（今の出力に残る崩れを正しい本文として教えない）")
    ap.add_argument("--extra", nargs="*", default=[], help="real_pairs.py の出力（本物の崩れの例）。学習に混ぜる")
    ap.add_argument("--extra-repeat", type=int, default=2, help="本物の崩れの例を何回入れるか")
    ap.add_argument("--dev", type=float, default=0.05)
    ap.add_argument("--seed", type=int, default=1)
    args = ap.parse_args()
    rng = random.Random(args.seed)
    sources: list[tuple[str, list[str]]] = []
    for line in open(args.laws):
        law = json.loads(line)
        paras = law["paragraphs"]
        if rng.random() < 0.6:
            paras = [to_half(p) for p in paras]
        sources.append((f"law:{law['law_id']}", wrap(paras, rng.randint(18, 45))))
    flagged: set[tuple[str, int]] = set()
    if args.kanpo_flags:
        for line in open(args.kanpo_flags, errors="replace"):
            f = line.rstrip("\n").split("\t")
            if len(f) >= 3 and float(f[2]) >= 0.9:
                flagged.add((f[0], int(f[1])))
    if args.kanpo:
        exclude = set(Path(args.exclude).read_text().split()) if args.exclude else set()
        for path in sorted(Path(args.kanpo).iterdir()):
            if path.name in exclude or rng.random() >= args.kanpo_rate:
                continue
            lines = [l for l in path.read_text().splitlines() if l.strip()]
            if flagged:
                # 疑いの強い行の前後 3 行を落とし、残りを続きの行ごとに別の本文にする。
                bad = {i for i in range(len(lines)) if any((path.name, k) in flagged for k in range(i - 3, i + 4))}
                run: list[str] = []
                for i, l in enumerate(lines):
                    if i in bad:
                        if run:
                            sources.append((f"kanpo:{path.name}", run))
                        run = []
                    else:
                        run.append(l)
                if run:
                    sources.append((f"kanpo:{path.name}", run))
            else:
                sources.append((f"kanpo:{path.name}", lines))
    rng.shuffle(sources)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    n_dev = max(1, int(len(sources) * args.dev))
    counts = {}
    for split, group in (("dev", sources[:n_dev]), ("train", sources[n_dev:])):
        with (out / f"{split}.jsonl").open("w") as f:
            n = 0
            for name, lines in group:
                for chunk in examples(lines, args.max_chars, rng):
                    idx = index(chunk)
                    ops = [] if rng.random() < args.clean else corrupt(idx, rng)
                    text, breaks = labels(idx)
                    f.write(json.dumps({"text": text, "breaks": breaks, "ops": ops, "src": name}, ensure_ascii=False) + "\n")
                    n += 1
            if split == "train":
                for path in args.extra:
                    rows = [l for l in open(path)]
                    for _ in range(args.extra_repeat):
                        for l in rows:
                            f.write(l if l.endswith("\n") else l + "\n")
                            n += 1
            counts[split] = n
    print(counts)


if __name__ == "__main__":
    main()
