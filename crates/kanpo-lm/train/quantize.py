"""学習した重み（fp32、約 147MB）を、2 次元の重み（埋め込み表と線形層）を行ごとの int8 にして小さくする（約 37MB）。

    python quantize.py ~/.cache/kanpo-lm/breaks-30m ~/.cache/kanpo-lm/breaks-30m-int8
    python quantize.py --embeddings-only ~/.cache/kanpo-lm/breaks-30m ~/.cache/kanpo-lm/breaks-30m-emb8

int8 にした重み `X` は `X`（int8 に 128 を足した U8。candle は I8 を読めない）と `X.__scale`（行ごとの倍率、F32）の
2 つで持つ。kanpo-lm は読むときに fp32 に戻して計算する（計算は fp32 のまま）。1 次元の重み（LayerNorm・bias）はそのまま。
"""

import shutil
import sys
from pathlib import Path

import torch
from safetensors.torch import load_file, save_file


def main():
    # --embeddings-only: 埋め込み表（重みの 7 割）だけを int8 にし、線形層は fp32 のまま（約 49MB、誤差がさらに小さい）
    only_emb = "--embeddings-only" in sys.argv
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    src, dst = Path(args[0]).expanduser(), Path(args[1]).expanduser()
    dst.mkdir(parents=True, exist_ok=True)
    tensors = load_file(str(src / "model.safetensors"))
    out = {}
    for name, t in tensors.items():
        t = t.float()
        if t.dim() == 2 and (not only_emb or "embeddings" in name):
            scale = t.abs().amax(dim=1).clamp(min=1e-12) / 127.0
            q = torch.round(t / scale[:, None]).clamp(-127, 127)
            out[name] = (q + 128).to(torch.uint8)
            out[name + ".__scale"] = scale.contiguous()
        else:
            out[name] = t.contiguous()
    save_file(out, str(dst / "model.safetensors"))
    for f in ("config.json", "tokenizer.json"):
        shutil.copy(src / f, dst / f)
    size = (dst / "model.safetensors").stat().st_size
    print(f"{dst / 'model.safetensors'}: {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
