"""ModernBERT-Ja-30m を、トークンごとに「直前から本文の流れが途切れているか」を当てる分類器に fine-tune する。

    python train.py --model ~/.cache/kanpo-lm/modernbert-ja-30m --data data/ --out ~/.cache/kanpo-lm/breaks-30m

学習データは gen_data.py で作る。出力は transformers の形（config.json・model.safetensors）に tokenizer.json を添えたもの。
kanpo-lm の `tagger`（candle）が読む。
"""

import argparse
import json
import math
import random
import shutil
import time
from pathlib import Path

import numpy as np
import torch
from sklearn.metrics import average_precision_score, roc_auc_score
from transformers import AutoModelForTokenClassification, PreTrainedTokenizerFast

WS = set(" 　\n\t")


def load_tokenizer(model_dir: Path) -> PreTrainedTokenizerFast:
    return PreTrainedTokenizerFast(
        tokenizer_file=str(model_dir / "tokenizer.json"),
        bos_token="<s>", eos_token="</s>", unk_token="<unk>", pad_token="<pad>", mask_token="<mask>", cls_token="<cls>", sep_token="<sep>",
    )


def encode(tok, text: str, breaks: list[int], max_len: int):
    enc = tok(text, truncation=True, max_length=max_len, return_offsets_mapping=True)
    marks = set(breaks)
    labels = []
    for s, e in enc["offset_mapping"]:
        if s == e or all(c in WS for c in text[s:e]):
            labels.append(-100)
        else:
            labels.append(1 if any(i in marks for i in range(s, e)) else 0)
    return enc["input_ids"], labels


def batches(rows, size, pad_id, shuffle, rng):
    order = list(range(len(rows)))
    if shuffle:
        rng.shuffle(order)
    for i in range(0, len(order), size):
        chunk = [rows[j] for j in order[i:i + size]]
        n = max(len(r[0]) for r in chunk)
        ids = torch.full((len(chunk), n), pad_id)
        mask = torch.zeros((len(chunk), n), dtype=torch.long)
        lab = torch.full((len(chunk), n), -100)
        for k, (x, y) in enumerate(chunk):
            ids[k, :len(x)] = torch.tensor(x)
            mask[k, :len(x)] = 1
            lab[k, :len(y)] = torch.tensor(y)
        yield ids, mask, lab


@torch.no_grad()
def evaluate(model, rows, pad_id, device, size=64):
    model.eval()
    probs, golds, ex_score, ex_gold = [], [], [], []
    for ids, mask, lab in batches(rows, size, pad_id, False, None):
        logits = model(input_ids=ids.to(device), attention_mask=mask.to(device)).logits.float()
        p = torch.softmax(logits, -1)[..., 1].cpu()
        for k in range(ids.shape[0]):
            keep = lab[k] != -100
            pk, gk = p[k][keep].numpy(), lab[k][keep].numpy()
            probs.append(pk)
            golds.append(gk)
            ex_score.append(float(pk.max()) if len(pk) else 0.0)
            ex_gold.append(int(gk.max()) if len(gk) else 0)
    model.train()
    p, g = np.concatenate(probs), np.concatenate(golds)
    pred = p >= 0.5
    tp = int((pred & (g == 1)).sum())
    prec = tp / max(1, int(pred.sum()))
    rec = tp / max(1, int((g == 1).sum()))
    return {
        "token_ap": float(average_precision_score(g, p)),
        "token_p": prec,
        "token_r": rec,
        "token_f1": 2 * prec * rec / max(1e-9, prec + rec),
        "example_auc": float(roc_auc_score(ex_gold, ex_score)),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--max-len", type=int, default=512)
    ap.add_argument("--batch", type=int, default=32)
    ap.add_argument("--lr", type=float, default=5e-5)
    ap.add_argument("--epochs", type=float, default=2.0)
    ap.add_argument("--pos-weight", type=float, default=3.0)
    ap.add_argument("--eval-every", type=int, default=500)
    ap.add_argument("--limit", type=int, default=0, help="学習に使う例の数（0 は全部）")
    ap.add_argument("--seed", type=int, default=1)
    args = ap.parse_args()
    rng = random.Random(args.seed)
    torch.manual_seed(args.seed)
    model_dir, out = Path(args.model).expanduser(), Path(args.out).expanduser()
    device = torch.device("mps" if torch.backends.mps.is_available() else "cpu")
    tok = load_tokenizer(model_dir)

    def load(split):
        rows = []
        for line in open(Path(args.data) / f"{split}.jsonl"):
            d = json.loads(line)
            rows.append(encode(tok, d["text"], d["breaks"], args.max_len))
        return rows

    train, dev = load("train"), load("dev")
    if args.limit:
        rng.shuffle(train)
        train = train[: args.limit]
    print(f"train {len(train)} dev {len(dev)} device {device}", flush=True)
    model = AutoModelForTokenClassification.from_pretrained(model_dir, num_labels=2, attn_implementation="sdpa").to(device)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=0.01)
    steps = math.ceil(len(train) / args.batch * args.epochs)
    warmup = max(1, steps // 20)
    sched = torch.optim.lr_scheduler.LambdaLR(opt, lambda s: min(1.0, (s + 1) / warmup) * max(0.0, (steps - s) / max(1, steps - warmup)))
    loss_fn = torch.nn.CrossEntropyLoss(weight=torch.tensor([1.0, args.pos_weight], device=device), ignore_index=-100)
    pad_id = tok.pad_token_id
    step, best, t0 = 0, -1.0, time.time()
    model.train()
    while step < steps:
        for ids, mask, lab in batches(train, args.batch, pad_id, True, rng):
            logits = model(input_ids=ids.to(device), attention_mask=mask.to(device)).logits
            loss = loss_fn(logits.view(-1, 2), lab.to(device).view(-1))
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            sched.step()
            opt.zero_grad()
            step += 1
            if step % 50 == 0:
                print(f"step {step}/{steps} loss {loss.item():.4f} {time.time() - t0:.0f}s", flush=True)
            if step % args.eval_every == 0 or step == steps:
                m = evaluate(model, dev, pad_id, device)
                print(f"eval step {step} {json.dumps(m)}", flush=True)
                if m["token_ap"] > best:
                    best = m["token_ap"]
                    model.save_pretrained(out, safe_serialization=True)
                    shutil.copy(model_dir / "tokenizer.json", out / "tokenizer.json")
                    (out / "eval.json").write_text(json.dumps({"step": step, **m}, indent=2))
            if step >= steps:
                break
    print(f"best token_ap {best:.4f} -> {out}", flush=True)


if __name__ == "__main__":
    main()
