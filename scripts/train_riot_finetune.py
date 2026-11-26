"""Ajuste experimental BuildNet; no lee test ni sobrescribe producción."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import torch
import torch.nn.functional as F


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--project", type=Path, required=True)
    parser.add_argument("--rows", type=Path, required=True)
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--seed", type=int, default=1337)
    parser.add_argument("--epochs", type=int, default=12)
    parser.add_argument("--patience", type=int, default=3)
    parser.add_argument("--lr", type=float, default=0.0001)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    sys.path.insert(0, str(args.project))
    from lolanalyzer.models.build_net import load_build_net
    from lolanalyzer.models.train_build import evaluate, make_tensors

    torch.set_num_threads(4)
    torch.manual_seed(args.seed)
    rng = np.random.default_rng(args.seed)
    with np.load(args.rows, allow_pickle=False) as source:
        rows = {name: source[name] for name in source.files}
    train = np.flatnonzero(rows["split"] == 0)
    val = np.flatnonzero(rows["split"] == 1)
    if not len(train) or not len(val):
        raise ValueError("Faltan filas de entrenamiento o validación")
    if set(rows["game_id"][train]) & set(rows["game_id"][val]):
        raise ValueError("Partidas compartidas entre entrenamiento y validación")
    # Se descarta el test de memoria antes de crear tensores.
    keep = rows["split"] != 2
    rows = {key: value[keep] for key, value in rows.items()}
    train = np.flatnonzero(rows["split"] == 0)
    val = np.flatnonzero(rows["split"] == 1)
    tensors = make_tensors(rows, "cpu")
    model, checkpoint = load_build_net(args.base, "cpu")
    baseline = evaluate(model, tensors, val, model.n_items)
    print("baseline", json.dumps(baseline), flush=True)
    opt = torch.optim.AdamW(model.parameters(), lr=args.lr, weight_decay=1e-4)
    best_score = baseline["top3"]
    best_epoch = 0
    best_state = copy.deepcopy(model.state_dict())
    history = []
    lengths = rows["length"]
    for epoch in range(1, args.epochs + 1):
        model.train()
        permutation = rng.permutation(train)
        total_loss = 0.0
        for start in range(0, len(permutation), 1024):
            selection = permutation[start:start + 1024]
            sel = torch.from_numpy(selection.astype(np.int64))
            lens = torch.from_numpy(lengths[selection].astype(np.int64))
            slot = (torch.rand(len(selection)) * lens).long()
            logits = model(*(tensors[key][sel] for key in ("champ", "role", "side", "ally", "enemy", "items")), slot)
            loss = F.cross_entropy(logits, tensors["items"][sel, slot], label_smoothing=0.02)
            opt.zero_grad(set_to_none=True)
            loss.backward()
            torch.nn.utils.clip_grad_norm_(model.parameters(), 1.0)
            opt.step()
            total_loss += float(loss.detach()) * len(selection)
        result = evaluate(model, tensors, val, model.n_items)
        history.append({"epoch": epoch, "loss": total_loss / len(train), "validation": result})
        print(f"epoch {epoch} val top1={result['top1']} top3={result['top3']}", flush=True)
        if result["top3"] > best_score:
            best_score = result["top3"]
            best_epoch = epoch
            best_state = copy.deepcopy(model.state_dict())
        if epoch - best_epoch >= args.patience:
            break
    checkpoint["state_dict"] = best_state
    checkpoint["experiment"] = {"baseSha256": digest(args.base), "rowsSha256": digest(args.rows),
                                "seed": args.seed, "epoch": best_epoch}
    torch.save(checkpoint, args.output / "candidate.pt")
    report = {
        "baseline": baseline, "bestEpoch": best_epoch, "history": history,
        "baseSha256": digest(args.base), "rowsSha256": digest(args.rows),
        "trainGames": len(set(rows["game_id"][train])),
        "validationGames": len(set(rows["game_id"][val])),
        "testUsed": False,
        "limitations": ["Inventarios finales ordenados, no decisiones completas en vivo.",
                        "Comparación observacional; la selección usa validación repetida."],
    }
    (args.output / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(f"best_epoch={best_epoch} best_top3={best_score}", flush=True)


if __name__ == "__main__":
    main()
