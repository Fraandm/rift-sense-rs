"""Audita partidas Riot candidatas sin modificar datasets ni entrenar.

La unidad de partición es GameId. El corte prospectivo es posterior al del
dataset anterior; un duplicado del dataset publicado nunca es independiente.
"""
import argparse
from collections import Counter, defaultdict
import datetime as dt
import hashlib
import json
from pathlib import Path

import numpy as np


def read_lines(path):
    with path.open(encoding="utf-8") as stream:
        for line in stream:
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                yield None


def audit(raw, dataset, val_start, test_start):
    if not val_start < test_start:
        raise ValueError("La fecha de validación debe preceder al test")
    with np.load(dataset, allow_pickle=False) as archive:
        old_ids = set(archive["game_ids"].tolist())
    counts = Counter()
    partitions = Counter()
    by_region_queue = defaultdict(Counter)
    seen = set()
    for file in sorted(raw.rglob("games.jsonl")):
        players_file = file.with_name("players.jsonl")
        if not players_file.exists():
            counts["missing_player_files"] += 1
            continue
        players = defaultdict(list)
        for player in read_lines(players_file):
            if not isinstance(player, dict):
                counts["invalid_player_json"] += 1
                continue
            players[player.get("GameId")].append(player)
        for game in read_lines(file):
            if not isinstance(game, dict):
                counts["invalid_game_json"] += 1
                continue
            gid = game.get("GameId")
            if not isinstance(gid, str) or not gid:
                counts["missing_id"] += 1
                continue
            if gid in seen:
                counts["duplicate_source_rows"] += 1
                continue
            seen.add(gid)
            if gid in old_ids:
                counts["existing_dataset"] += 1
                continue
            try:
                date = dt.date.fromisoformat(game.get("DateUtc", "")[:10])
            except (ValueError, TypeError):
                counts["invalid_date"] += 1
                continue
            rows = players.get(gid, [])
            winner = game.get("Winner")
            role_set = {(p.get("Side"), p.get("Role")) for p in rows}
            valid = (
                game.get("QueueId") in (420, 440)
                and winner in (1, 2)
                and len(rows) == 10
                and len(role_set) == 10
                and all(
                    p.get("Side") in (1, 2)
                    and p.get("Role") in ("Top", "Jungle", "Mid", "Bot", "Support")
                    and p.get("Champion")
                    and p.get("PlayerWin") in ("Yes", "No")
                    and (p.get("PlayerWin") == "Yes") == (p.get("Side") == winner)
                    for p in rows
                )
            )
            if not valid:
                counts["invalid_match"] += 1
                continue
            partition = "train" if date < val_start else "validation" if date < test_start else "test"
            partitions[partition] += 1
            key = f"{game.get('Region', 'unknown')}:{game.get('QueueId')}"
            by_region_queue[key][partition] += 1
            counts["eligible_matches"] += 1
    return {
        "existingDatasetSha256": hashlib.sha256(dataset.read_bytes()).hexdigest(),
        "validationStartUtc": val_start.isoformat(),
        "testStartUtc": test_start.isoformat(),
        "counts": dict(counts),
        "partitions": dict(partitions),
        "regionQueue": {key: dict(value) for key, value in sorted(by_region_queue.items())},
        "limitations": [
            "Auditoría preliminar: no verifica vocabulario, cronología de objetos ni timelines.",
            "El conjunto anterior usó fechas posteriores como test; esta partición es un nuevo experimento.",
            "El muestreo parte de jugadores apex y no representa todas las partidas ranked.",
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", required=True, type=Path)
    parser.add_argument("--dataset", required=True, type=Path)
    parser.add_argument("--validation-start", required=True, type=dt.date.fromisoformat)
    parser.add_argument("--test-start", required=True, type=dt.date.fromisoformat)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    report = audit(args.raw, args.dataset, args.validation_start, args.test_start)
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps({"counts": report["counts"], "partitions": report["partitions"]}))


if __name__ == "__main__":
    main()
