"""パーサを直す前と後の本文（textdump の出力）の組から、本物の崩れのラベル付きの例を作る。

    python real_pairs.py <直す前> <直した後> out.jsonl [--exclude 除くページの一覧]

字の集合が前後で同じページ（読み順だけが変わったページ）だけを使う。直す前の各字を直した後の本文の位置に突き合わせ
（一致する並びを先に取り、残りの字は同じ字のうち使っていないものを前から当てる）、gen_data.py と同じく直前の字の
位置の次でない字を切れ目とする。直す前のページを 180〜360 字のまとまりに分け、切れ目を含むまとまりと、同じページの
崩れの無いまとまりを出す（出力の形は gen_data.py と同じ）。
"""

import argparse
import collections
import difflib
import json
import random
from pathlib import Path

WS = set(" 　\n\t")


def align(before: str, after: str) -> list[int | None]:
    """直す前の各字（空白は None）の、直した後の本文（空白を除いて数えた）の中の位置。"""
    xs = [c for c in before if c not in WS]
    ys = [c for c in after if c not in WS]
    src: list[int | None] = [None] * len(xs)
    used = [False] * len(ys)
    for a, b, n in difflib.SequenceMatcher(None, xs, ys, autojunk=False).get_matching_blocks():
        for k in range(n):
            src[a + k] = b + k
            used[b + k] = True
    free = collections.defaultdict(collections.deque)
    for j, c in enumerate(ys):
        if not used[j]:
            free[c].append(j)
    for i, c in enumerate(xs):
        if src[i] is None and free[c]:
            src[i] = free[c].popleft()
    out, k = [], 0
    for c in before:
        if c in WS:
            out.append(None)
        else:
            out.append(src[k])
            k += 1
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("before")
    ap.add_argument("after")
    ap.add_argument("out")
    ap.add_argument("--exclude")
    ap.add_argument("--max-chars", type=int, default=360)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--max-examples", type=int, default=0, help="切れ目を含む例の上限（0 は全部）")
    args = ap.parse_args()
    rng = random.Random(args.seed)
    exclude = set(Path(args.exclude).read_text().split()) if args.exclude else set()
    n_pos = n_neg = pages = 0
    rows = []
    with open(args.out, "w") as f:
        for path in sorted(Path(args.before).iterdir()):
            other = Path(args.after) / path.name
            if path.name in exclude or not other.exists():
                continue
            before, after = path.read_text(), other.read_text()
            if before == after:
                continue
            if collections.Counter(c for c in before if c not in WS) != collections.Counter(c for c in after if c not in WS):
                continue
            pages += 1
            lines = [l for l in before.splitlines() if l.strip()]
            text = "\n".join(lines)
            src = align(text, "\n".join(l for l in after.splitlines() if l.strip()))
            # 行ごとの (開始, 終わり) の字の位置。
            spans, at = [], 0
            for l in lines:
                spans.append((at, at + len(l)))
                at += len(l) + 1
            i = 0
            prev = None
            while i < len(lines):
                limit = rng.randint(args.max_chars // 2, args.max_chars)
                j, size = i, 0
                while j < len(lines) and (j == i or size + len(lines[j]) <= limit):
                    size += len(lines[j])
                    j += 1
                lo, hi = spans[i][0], spans[j - 1][1]
                breaks = []
                # まとまりの頭の字は、前のまとまりからの続きかを見ない（gen_data と同じく、まとまりの中だけで決める）。
                prev = None
                for p in range(lo, hi):
                    s = src[p]
                    if s is None:
                        continue
                    if prev is not None and s != prev + 1:
                        breaks.append(p - lo)
                    prev = s
                chunk = text[lo:hi]
                # 数字の多いまとまり（表の金額）は同じ字が多く、突き合わせが当てにならない（「8|94,361,200」のような
                # 偽の切れ目ができる）ので使わない。
                digits = sum(c.isdigit() for c in chunk)
                if digits > len(chunk) * 0.25:
                    i = j
                    continue
                if breaks:
                    rows.append({"text": chunk, "breaks": breaks, "ops": ["real"], "src": f"real:{path.name}"})
                elif rng.random() < 0.3:
                    rows.append({"text": chunk, "breaks": [], "ops": [], "src": f"real:{path.name}"})
                i = j
        pos = [r for r in rows if r["breaks"]]
        neg = [r for r in rows if not r["breaks"]]
        if args.max_examples and len(pos) > args.max_examples:
            rng.shuffle(pos)
            neg = neg[: int(len(neg) * args.max_examples / len(pos))]
            pos = pos[: args.max_examples]
        for r in pos + neg:
            f.write(json.dumps(r, ensure_ascii=False) + "\n")
        n_pos, n_neg = len(pos), len(neg)
    print(f"{pages} pages, {n_pos} examples with breaks, {n_neg} without")


if __name__ == "__main__":
    main()
