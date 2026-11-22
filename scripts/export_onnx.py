"""Exporta los checkpoints de RiftSense a ONNX para el runtime Rust.

El proyecto Python guarda `artifacts/build_net.pt` y `artifacts/draft_net.pt`.
Este script vive en el repo Rust para no modificar el pipeline original y
genera los ficheros que consume `src/model.rs`.

Uso:
    python scripts/export_onnx.py --root "C:/ruta/Analizador LOL"
    python scripts/export_onnx.py --root "..." --only build
"""
from __future__ import annotations

import argparse
import datetime as dt
import json
import sys
from pathlib import Path

import torch


def load_classes(root: Path):
    sys.path.insert(0, str(root))
    from lolanalyzer.models.build_net import BuildNet  # noqa: PLC0415
    from lolanalyzer.models.draft_net import DraftNet  # noqa: PLC0415

    return BuildNet, DraftNet


def export_build(root: Path, out_dir: Path) -> dict:
    BuildNet, _ = load_classes(root)
    checkpoint = root / "artifacts" / "build_net.pt"
    ckpt = torch.load(checkpoint, map_location="cpu", weights_only=False)
    model = BuildNet(
        n_champions=int(ckpt["n_champions"]),
        n_items=int(ckpt["n_items"]),
        d_model=int(ckpt.get("d_model", 96)),
        hidden=int(ckpt.get("hidden", 192)),
        dropout=float(ckpt.get("dropout", 0.15)),
    )
    model.load_state_dict(ckpt["state_dict"])
    model.eval()

    batch = 2
    champ = torch.zeros(batch, dtype=torch.long)
    role = torch.ones(batch, dtype=torch.long)
    side = torch.zeros(batch, dtype=torch.long)
    ally = torch.zeros(batch, 5, dtype=torch.long)
    enemy = torch.zeros(batch, 5, dtype=torch.long)
    items = torch.zeros(batch, 7, dtype=torch.long)
    slot = torch.zeros(batch, dtype=torch.long)

    out = out_dir / "build_net.onnx"
    with torch.no_grad():
        torch.onnx.export(
            model,
            (champ, role, side, ally, enemy, items, slot),
            out,
            input_names=["champ", "role", "side", "ally", "enemy", "items", "slot"],
            output_names=["logits"],
            dynamic_axes={
                "champ": {0: "batch"},
                "role": {0: "batch"},
                "side": {0: "batch"},
                "ally": {0: "batch"},
                "enemy": {0: "batch"},
                "items": {0: "batch"},
                "slot": {0: "batch"},
                "logits": {0: "batch"},
            },
            opset_version=17,
            do_constant_folding=True,
            dynamo=False,
            external_data=False,
        )
    return {
        "build_n_items": int(ckpt["n_items"]),
        "build_n_champions": int(ckpt["n_champions"]),
    }


def export_draft(root: Path, out_dir: Path) -> dict:
    _, DraftNet = load_classes(root)
    checkpoint = root / "artifacts" / "draft_net.pt"
    ckpt = torch.load(checkpoint, map_location="cpu", weights_only=False)
    model = DraftNet(
        n_champions=int(ckpt["n_champions"]),
        d_model=int(ckpt.get("d_model", 96)),
        n_heads=int(ckpt.get("n_heads", 4)),
        n_layers=int(ckpt.get("n_layers", 3)),
        dropout=float(ckpt.get("dropout", 0.15)),
    )
    model.load_state_dict(ckpt["state_dict"])
    model.eval()

    batch = 2
    champ = torch.zeros(batch, 20, dtype=torch.long)
    role = torch.ones(batch, 20, dtype=torch.long)
    side = torch.zeros(batch, 20, dtype=torch.long)
    kind = torch.ones(batch, 20, dtype=torch.long)
    mask = torch.ones(batch, 20, dtype=torch.bool)

    out = out_dir / "draft_net.onnx"
    with torch.no_grad():
        torch.onnx.export(
            model,
            (champ, role, side, kind, mask),
            out,
            input_names=["champ", "role", "side", "kind", "mask"],
            output_names=["logits"],
            dynamic_axes={
                "champ": {0: "batch"},
                "role": {0: "batch"},
                "side": {0: "batch"},
                "kind": {0: "batch"},
                "mask": {0: "batch"},
                "logits": {0: "batch"},
            },
            opset_version=17,
            do_constant_folding=True,
            dynamo=False,
            external_data=False,
        )
    return {
        "draft_n_champions": int(ckpt["n_champions"]),
    }


def main() -> None:
    parser = argparse.ArgumentParser(description="Exporta BuildNet/DraftNet a ONNX")
    parser.add_argument(
        "--root",
        default=str(Path(__file__).resolve().parents[2] / "Analizador LOL"),
        help="Raiz del proyecto Python (contiene artifacts/ y lolanalyzer/)",
    )
    parser.add_argument(
        "--out",
        default=str(Path(__file__).resolve().parents[1] / "models"),
        help="Carpeta de salida para los .onnx",
    )
    parser.add_argument("--only", choices=["build", "draft"], default=None)
    args = parser.parse_args()

    root = Path(args.root).resolve()
    out_dir = Path(args.out).resolve()
    if not (root / "artifacts" / "build_net.pt").exists():
        raise SystemExit(f"No existe {root / 'artifacts' / 'build_net.pt'}")
    out_dir.mkdir(parents=True, exist_ok=True)

    manifest_path = out_dir / "manifest.json"
    manifest: dict = {}
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest.update(
        {
            "draft": "draft_net.onnx",
            "build": "build_net.onnx",
            "exported_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        }
    )

    if args.only in (None, "build"):
        print(f"Exportando BuildNet -> {out_dir / 'build_net.onnx'}")
        manifest.update(export_build(root, out_dir))
    if args.only in (None, "draft"):
        print(f"Exportando DraftNet -> {out_dir / 'draft_net.onnx'}")
        manifest.update(export_draft(root, out_dir))

    manifest_path.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )
    print(f"Manifiesto -> {manifest_path}")


if __name__ == "__main__":
    main()
