"""Prepara filas experimentales de inventarios finales Riot; no son timelines.

No modifica fuentes ni producción. Excluye partidas del dataset publicado y
filas con objetos ambiguos. Particiones temporales agrupadas por GameId.
"""
import argparse
from collections import Counter, defaultdict
import datetime as dt
import hashlib
import json
from pathlib import Path

import numpy as np

ROLES = {"Top": "TOP", "Jungle": "JUNGLE", "Mid": "MID", "Bot": "BOT", "Support": "SUPPORT"}


def read_jsonl(path):
    with path.open(encoding="utf-8") as source:
        for line_number, line in enumerate(source, 1):
            try:
                yield json.loads(line)
            except json.JSONDecodeError as error:
                raise ValueError(f"JSON inválido en {path}:{line_number}") from error


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(raw, processed, metadata, output, validation_start, test_start):
    if validation_start >= test_start:
        raise ValueError("Cortes temporales inválidos")
    output.mkdir(parents=True, exist_ok=False)
    vocab_path = processed / "vocab.json"
    dataset_path = processed / "dataset.npz"
    vocab = json.loads(vocab_path.read_text(encoding="utf-8"))
    meta = json.loads(metadata.read_text(encoding="utf-8"))
    name_to_indices = defaultdict(set)
    for item in meta["items"]:
        idx = vocab["items"].get(str(item["id"]))
        if idx:
            name_to_indices[item["name"]].add(idx)
    with np.load(dataset_path, allow_pickle=False) as original:
        old_ids = set(original["game_ids"].tolist())
    rows = defaultdict(list)
    counts = Counter()
    seen = set()
    epoch = dt.date(2024, 1, 1)
    for game_path in sorted(raw.rglob("games.jsonl")):
        player_path = game_path.with_name("players.jsonl")
        if not player_path.exists():
            counts["missing_player_files"] += 1
            continue
        players = defaultdict(list)
        for player in read_jsonl(player_path):
            players[player.get("GameId")].append(player)
        for game in read_jsonl(game_path):
            gid = game.get("GameId")
            if not isinstance(gid, str) or not gid:
                counts["invalid_id"] += 1
                continue
            if gid in seen:
                counts["duplicates"] += 1
                continue
            seen.add(gid)
            if gid in old_ids:
                counts["existing_dataset"] += 1
                continue
            try:
                date = dt.date.fromisoformat(game.get("DateUtc", "")[:10])
                patch_parts = game["RiotPatch"].split(".")
                major, minor = int(patch_parts[0]), int(patch_parts[1])
                patch = (major + 10 if major >= 15 else major) * 100 + minor
            except (ValueError, TypeError, KeyError, IndexError):
                counts["invalid_date_or_patch"] += 1
                continue
            group = players.get(gid, [])
            winner = game.get("Winner")
            slots = {(p.get("Side"), p.get("Role")) for p in group}
            valid = (
                game.get("QueueId") in (420, 440) and winner in (1, 2)
                and len(group) == 10 and len(slots) == 10
                and all(
                    p.get("Side") in (1, 2) and p.get("Role") in ROLES
                    and p.get("Champion") in vocab["champions"]
                    and p.get("PlayerWin") in ("Yes", "No")
                    and (p["PlayerWin"] == "Yes") == (p["Side"] == winner)
                    for p in group
                )
            )
            if not valid:
                counts["invalid_game"] += 1
                continue
            split = 0 if date < validation_start else 1 if date < test_start else 2
            teams = {
                side: [
                    vocab["champions"][p["Champion"]]
                    for p in sorted((p for p in group if p["Side"] == side),
                                    key=lambda p: vocab["roleIdx"][ROLES[p["Role"]]])
                ]
                for side in (1, 2)
            }
            counts["games_" + ("train", "validation", "test")[split]] += 1
            for p in group:
                items = p.get("Items")
                if not isinstance(items, list) or not items or len(items) > 7:
                    counts["invalid_item_sequence"] += 1
                    continue
                indices = []
                for name in items:
                    candidates = name_to_indices.get(name, set())
                    if len(candidates) != 1:
                        break
                    indices.append(next(iter(candidates)))
                if len(indices) != len(items):
                    counts["unknown_or_ambiguous_items"] += 1
                    continue
                side = p["Side"]
                rows["champ"].append(vocab["champions"][p["Champion"]])
                rows["role"].append(vocab["roleIdx"][ROLES[p["Role"]]])
                rows["side"].append(side - 1)
                rows["ally"].append(teams[side])
                rows["enemy"].append(teams[3 - side])
                rows["items"].append(indices + [0] * (7 - len(indices)))
                rows["length"].append(len(indices))
                rows["split"].append(split)
                rows["date"].append((date - epoch).days)
                rows["patch"].append(patch)
                rows["game_id"].append(gid)
                rows["queue"].append(game["QueueId"])
                counts["rows_" + ("train", "validation", "test")[split]] += 1
    if not rows:
        raise ValueError("No se encontraron filas válidas")
    arrays = {
        key: np.asarray(value, dtype="U100" if key == "game_id" else np.int32)
        for key, value in rows.items()
    }
    game_sets = [set(arrays["game_id"][arrays["split"] == i]) for i in range(3)]
    if any(game_sets[i] & game_sets[j] for i in range(3) for j in range(i)):
        raise ValueError("Una partida aparece en varias particiones")
    np.savez_compressed(output / "riot-build-rows.npz", **arrays)
    report = {
        "counts": dict(counts), "validationStart": validation_start.isoformat(),
        "testStart": test_start.isoformat(), "vocabSha256": sha(vocab_path),
        "baselineDatasetSha256": sha(dataset_path), "metadataSha256": sha(metadata),
        "limitations": [
            "Inventario final ordenado por primera compra; no representa todas las compras ni estados en vivo.",
            "Fechas posteriores al test histórico: nuevo experimento, no comparación del test anterior.",
            "Jugadores semilla apex; no representa toda la población ranked.",
        ],
    }
    (output / "manifest.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(report["counts"], ensure_ascii=True))


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--raw", type=Path, required=True)
    p.add_argument("--processed", type=Path, required=True)
    p.add_argument("--metadata", type=Path, required=True)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--validation-start", type=dt.date.fromisoformat, required=True)
    p.add_argument("--test-start", type=dt.date.fromisoformat, required=True)
    a = p.parse_args()
    prepare(a.raw, a.processed, a.metadata, a.output, a.validation_start, a.test_start)
