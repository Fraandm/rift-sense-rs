"""Extrae decisiones experimentales conservadoras desde cachés Riot.

La coincidencia final es solo un filtro retrospectivo de calidad, nunca una
característica. No se modifica ninguna fuente ni dataset de producción.
"""
import argparse
from collections import Counter
import datetime as dt
import json
from pathlib import Path

import numpy as np
from timeline_inventory import audit_final_inventory, reconstruct


def extract(cache, dataset, output, val_start, test_start):
    if val_start >= test_start:
        raise ValueError("Cortes temporales inválidos")
    with np.load(dataset, allow_pickle=False) as archive:
        old = set(archive["game_ids"].tolist())
    output.mkdir(parents=True, exist_ok=False)
    counts = Counter()
    files = {name: (output / f"{name}.jsonl").open("x", encoding="utf-8")
             for name in ("train", "validation", "test")}
    try:
        for path in sorted(cache.glob("*.json")):
            try:
                payload = json.loads(path.read_text(encoding="utf-8"))
                match, timeline = payload["match"], payload["timeline"]
                info = match["info"]
                gid = match["metadata"]["matchId"]
                if gid in old:
                    counts["existing_dataset"] += 1
                    continue
                if info.get("queueId") not in (420, 440) or info.get("mapId") != 11:
                    counts["excluded_mode"] += 1
                    continue
                creation = int(info["gameCreation"]) / 1000
                date = dt.datetime.fromtimestamp(creation, tz=dt.timezone.utc).date()
                if date < dt.date(2024, 1, 1):
                    counts["invalid_date"] += 1
                    continue
                report = audit_final_inventory(match, timeline)
                # Una ambigüedad tardía no invalida necesariamente los estados
                # anteriores. Cada evento conserva su propio filtro fiable.
                valid_players = {p["participantId"] for p in report["participants"]
                                 if p["matchesFinal"]}
                events = reconstruct(timeline)
                participants = {p["participantId"]: p for p in info["participants"]}
            except (OSError, ValueError, TypeError, KeyError, AttributeError, OverflowError):
                counts["invalid_cache"] += 1
                continue
            split = "train" if date < val_start else "validation" if date < test_start else "test"
            counts["matches_" + split] += 1
            counts["participants_final_exact"] += len(valid_players)
            for event in events:
                if (event["event"] != "ITEM_PURCHASED" or
                    not event["reliable"] or not event["outcomeKnown"] or
                    event["participantId"] not in valid_players):
                    continue
                player = participants[event["participantId"]]
                record = {
                    "gameId": gid,
                    "participantId": event["participantId"],
                    "queueId": info["queueId"],
                    "dateUtc": date.isoformat(),
                    "gameVersion": info.get("gameVersion"),
                    "championId": player.get("championId"),
                    "teamId": player.get("teamId"),
                    "role": player.get("teamPosition"),
                    "timestampMs": event["timestampMs"],
                    "inventoryBefore": event["inventoryBefore"],
                    "purchasedItemId": event["itemId"],
                }
                files[split].write(json.dumps(record, ensure_ascii=False) + "\n")
                counts["decisions_" + split] += 1
    finally:
        for file in files.values():
            file.close()
    report = {
        "counts": dict(counts), "validationStart": val_start.isoformat(),
        "testStart": test_start.isoformat(),
        "limitations": [
            "Coincidencia final usada como filtro retrospectivo: introduce sesgo de selección.",
            "No se reconstruyen deshacer, transformaciones gratuitas ni compras ambiguas.",
            "Inventarios aparentemente fiables no están verificados individualmente.",
            "Entradas de test reservadas: no usar para selección ni ajustes.",
        ],
    }
    (output / "manifest.json").write_text(json.dumps(report, indent=2, ensure_ascii=False), encoding="utf-8")
    print(json.dumps(report["counts"]), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache", type=Path, required=True)
    parser.add_argument("--dataset", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--validation-start", type=dt.date.fromisoformat, required=True)
    parser.add_argument("--test-start", type=dt.date.fromisoformat, required=True)
    args = parser.parse_args()
    extract(args.cache, args.dataset, args.output, args.validation_start, args.test_start)
