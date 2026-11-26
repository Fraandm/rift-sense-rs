"""Crea casos de paridad PyTorch/tract a partir de validación, sin leer test."""
import argparse
import json
from pathlib import Path
import sys

import numpy as np
import torch

parser = argparse.ArgumentParser()
parser.add_argument("--project", type=Path, required=True)
parser.add_argument("--rows", type=Path, required=True)
parser.add_argument("--checkpoint", type=Path, required=True)
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
sys.path.insert(0, str(args.project))
from lolanalyzer.models.build_net import load_build_net

model, _ = load_build_net(args.checkpoint, "cpu")
model.eval()
with np.load(args.rows, allow_pickle=False) as rows:
    indices = np.flatnonzero(rows["split"] == 1)
    indices = np.random.default_rng(812).choice(indices, size=min(64, len(indices)), replace=False)
    cases = []
    for index in indices:
        slot = int(index % int(rows["length"][index]))
        inputs = {key: np.asarray(rows[key][index:index + 1], dtype=np.int64).tolist()
                  for key in ("champ", "role", "side", "ally", "enemy", "items")}
        inputs["slot"] = [slot]
        tensors = [torch.tensor(inputs[key], dtype=torch.long)
                   for key in ("champ", "role", "side", "ally", "enemy", "items", "slot")]
        with torch.no_grad():
            expected = torch.softmax(model(*tensors), dim=-1).flatten().tolist()
        cases.append({"inputs": inputs, "expected": expected})
args.output.write_text(json.dumps({"build": cases, "draft": []}), encoding="utf-8")
print(f"{len(cases)} casos de validación; test no utilizado")
