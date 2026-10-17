"""学習した分類器を、gen_data.py の形の評価用データで採点する（train.py の評価と同じ指標）。

    python evaluate.py ~/.cache/kanpo-lm/breaks-30m data/dev.jsonl [data2/dev.jsonl …]
"""

import json
import sys
from pathlib import Path

import torch
from transformers import AutoModelForTokenClassification

sys.path.insert(0, str(Path(__file__).parent))
from train import encode, evaluate, load_tokenizer  # noqa: E402


def main():
    model_dir = Path(sys.argv[1]).expanduser()
    device = torch.device("mps" if torch.backends.mps.is_available() else "cpu")
    tok = load_tokenizer(model_dir)
    model = AutoModelForTokenClassification.from_pretrained(model_dir, attn_implementation="sdpa").to(device)
    for path in sys.argv[2:]:
        rows = [encode(tok, d["text"], d["breaks"], 512) for d in map(json.loads, open(path))]
        m = evaluate(model, rows, tok.pad_token_id, device)
        print(f"{path}\t" + "\t".join(f"{k}={v:.4f}" for k, v in m.items()))


if __name__ == "__main__":
    main()
