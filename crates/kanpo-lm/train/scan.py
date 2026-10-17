"""切れ目の分類器で、本文のテキスト（textdump の出力）の各行の切れ目の確率を出す（GPU でまとめて流す分析用）。

    python scan.py ~/.cache/kanpo-lm/breaks-30m /tmp/after > breaks.tsv

出力は kanpo-lm の examples/breaks と同じ TSV（ファイル, 行, いちばん高い切れ目の確率, 切れ目（0.5 以上）の前に「|」を入れた行）。
行は 360 字ほどのまとまりにして流す（examples/breaks と同じ）。空行はまとまりの区切り（表の欄ごとに書いた textdump の
出力で、欄をまたいで文脈にしない）。
"""

import sys
from pathlib import Path

import torch
from transformers import AutoModelForTokenClassification

sys.path.insert(0, str(Path(__file__).parent))
from train import WS, load_tokenizer  # noqa: E402


def chunks(lines, limit=360):
    i = 0
    while i < len(lines):
        j, size = i, 0
        while j < len(lines) and (j == i or size + len(lines[j]) <= limit):
            size += len(lines[j])
            j += 1
        yield i, j
        i = j


@torch.no_grad()
def main():
    model_dir, src = Path(sys.argv[1]).expanduser(), Path(sys.argv[2])
    device = torch.device("mps" if torch.backends.mps.is_available() else "cpu")
    tok = load_tokenizer(model_dir)
    model = AutoModelForTokenClassification.from_pretrained(model_dir, attn_implementation="sdpa").to(device).eval()
    files = [src] if src.is_file() else sorted(p for p in src.iterdir() if p.is_file())
    jobs = []  # (file, lines, i, j, text, line_starts)
    for path in files:
        raw = path.read_text().splitlines()
        lines = [l for l in raw if l.strip()]
        # 空行で区切られたまとまり（表の欄など）は、まとまりをまたいで文脈にしない。
        blocks, cur, start = [], 0, 0
        for l in raw:
            if l.strip():
                cur += 1
            elif cur > start:
                blocks.append((start, cur))
                start = cur
        if cur > start:
            blocks.append((start, cur))
        spans = [(b0 + i, b0 + j) for b0, b1 in blocks for i, j in chunks(lines[b0:b1])]
        for i, j in spans:
            starts, at = [], 0
            for l in lines[i:j]:
                starts.append(at)
                at += len(l) + 1
            jobs.append((path.name, lines, i, j, "\n".join(lines[i:j]), starts))
    jobs.sort(key=lambda x: len(x[4]))
    out = []
    for b in range(0, len(jobs), 64):
        batch = jobs[b:b + 64]
        enc = tok([x[4] for x in batch], truncation=True, max_length=512, padding=True, return_offsets_mapping=True, return_tensors="pt")
        offsets = enc.pop("offset_mapping")
        probs = torch.softmax(model(input_ids=enc["input_ids"].to(device), attention_mask=enc["attention_mask"].to(device)).logits.float(), -1)[..., 1].cpu()
        for k, (name, lines, i, j, text, starts) in enumerate(batch):
            per_line = {n: (0.0, []) for n in range(i, j)}
            for t in range(offsets.shape[1]):
                s, e = int(offsets[k, t, 0]), int(offsets[k, t, 1])
                if s == e or all(c in WS for c in text[s:e]):
                    continue
                n = max(m for m in range(len(starts)) if starts[m] <= s)
                mx, cuts = per_line[i + n]
                p = float(probs[k, t])
                if p >= 0.5:
                    cuts.append(s - starts[n])
                per_line[i + n] = (max(mx, p), cuts)
            for n, (mx, cuts) in per_line.items():
                line = lines[n]
                marked = "".join(("|" if c in cuts else "") + ch for c, ch in enumerate(line))
                out.append((name, n, mx, marked))
    out.sort(key=lambda r: (r[0], r[1]))
    for name, n, mx, marked in out:
        print(f"{name}\t{n}\t{mx:.3f}\t{marked}")


if __name__ == "__main__":
    main()
