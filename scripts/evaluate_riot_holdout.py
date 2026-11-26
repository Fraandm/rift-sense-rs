"""Comparación pareada en test por GameId; salida nueva y sin promoción."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import numpy as np
import torch


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def predict_hits(model, tensors, indices):
    lengths = tensors["length"].numpy()
    outcomes, row_ids = [], []
    for slot in range(7):
        selected = indices[lengths[indices] > slot]
        for start in range(0, len(selected), 1024):
            ids = selected[start:start + 1024]
            sel = torch.from_numpy(ids.astype(np.int64))
            with torch.no_grad():
                logits = model(
                    *(tensors[key][sel] for key in ("champ", "role", "side", "ally", "enemy", "items")),
                    torch.full_like(sel, slot),
                )
                target = tensors["items"][sel, slot]
                target_score = logits.gather(1, target[:, None])
                item_ids = torch.arange(logits.shape[-1])[None, :]
                ranks = ((logits > target_score) |
                         ((logits == target_score) & (item_ids < target[:, None]))).sum(-1)
            outcomes.extend((ranks < 3).numpy().tolist())
            row_ids.extend(ids.tolist())
    return np.asarray(outcomes, dtype=np.float64), np.asarray(row_ids, dtype=np.int64)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--project", required=True, type=Path)
    p.add_argument("--rows", required=True, type=Path)
    p.add_argument("--base", required=True, type=Path)
    p.add_argument("--candidate", required=True, type=Path)
    p.add_argument("--output", required=True, type=Path)
    a = p.parse_args()
    if a.output.exists():
        raise FileExistsError("No se sobrescribe una evaluación existente")
    sys.path.insert(0, str(a.project))
    from lolanalyzer.models.build_net import load_build_net
    from lolanalyzer.models.train_build import make_tensors

    torch.set_num_threads(4)
    with np.load(a.rows, allow_pickle=False) as data:
        keep = data["split"] == 2
        rows = {name: data[name][keep] for name in data.files}
    if not len(rows["champ"]):
        raise ValueError("Test vacío")
    tensors = make_tensors(rows, "cpu")
    idx = np.arange(len(rows["champ"]), dtype=np.int64)
    base, _ = load_build_net(a.base, "cpu")
    candidate, ckpt = load_build_net(a.candidate, "cpu")
    experiment = ckpt.get("experiment", {})
    if experiment.get("rowsSha256") != digest(a.rows) or experiment.get("baseSha256") != digest(a.base):
        raise ValueError("Hashes del candidato no coinciden")
    first, id1 = predict_hits(base, tensors, idx)
    second, id2 = predict_hits(candidate, tensors, idx)
    if not np.array_equal(id1, id2):
        raise ValueError("Los objetivos comparados no coinciden")
    delta = second - first
    _, group = np.unique(rows["game_id"][id1], return_inverse=True)
    sums = np.bincount(group, weights=delta)
    ns = np.bincount(group)
    rng = np.random.default_rng(20261126)
    boot = []
    for _ in range(2000):
        sample = rng.integers(0, len(sums), len(sums))
        boot.append(float(sums[sample].sum() / ns[sample].sum()))
    roles = {}
    for role, label in ((1, "TOP"), (2, "JUNGLE"), (3, "MID"), (4, "BOT"), (5, "SUPPORT")):
        mask = rows["role"][id1] == role
        if mask.any():
            roles[label] = {"targets": int(mask.sum()), "base": float(first[mask].mean()),
                            "candidate": float(second[mask].mean())}
    report = {
        "metric": "top3", "targets": len(first), "independentGames": len(sums),
        "baseline": float(first.mean()), "candidate": float(second.mean()),
        "difference": float(delta.mean()), "ci95GameBootstrap": np.quantile(boot, [0.025, 0.975]).tolist(),
        "perRole": roles, "bootstrapReplicates": 2000, "bootstrapSeed": 20261126,
        "rowsSha256": digest(a.rows), "baseSha256": digest(a.base), "candidateSha256": digest(a.candidate),
        "limitations": ["Inventarios finales ordenados, no recomendaciones observadas en directo.",
                        "Muestreo de jugadores semilla de alto elo; no representa toda la población.",
                        "Test temporal nuevo; no es el test histórico del modelo publicado."],
    }
    with a.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, indent=2, ensure_ascii=False)
        output.write("\n")
    print(json.dumps(report, ensure_ascii=True), flush=True)


if __name__ == "__main__":
    main()
