"""Normaliza decisiones temporales experimentales; nunca lee test ni producción."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

# Consumibles y trinkets no modelados por BuildNet. No asumir que otros IDs
# desconocidos son irrelevantes: una pieza real podría ocupar una ranura.
IGNORED_NON_BUILD_IDS = {2003, 2031, 2055, 3340, 3363, 3364}


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def convert(row, items):
    target = row["purchasedItemId"]
    if target in IGNORED_NON_BUILD_IDS:
        return None, "ignored_target"
    if str(target) not in items:
        return None, "unknown_target"
    inventory = []
    ignored = 0
    for item in row["inventoryBefore"]:
        if item in IGNORED_NON_BUILD_IDS:
            ignored += 1
        elif str(item) not in items:
            return None, "unknown_inventory"
        else:
            inventory.append(items[str(item)])
    if len(inventory) > 7:
        return None, "inventory_over_capacity"
    result = {
        "gameId": row["gameId"],
        "participantId": row["participantId"],
        "championId": row["championId"],
        "teamId": row["teamId"],
        "role": row["role"],
        "queueId": row["queueId"],
        "dateUtc": row["dateUtc"],
        "timestampMs": row["timestampMs"],
        "inventoryIndices": inventory,
        "targetIndex": items[str(target)],
        "ignoredNonBuildCount": ignored,
    }
    return result, "accepted"


def prepare(source, vocab_path, output):
    vocab = json.loads(vocab_path.read_text(encoding="utf-8"))
    output.mkdir(parents=True, exist_ok=False)
    report = {"vocabSha256": digest(vocab_path), "testRead": False, "splits": {},
              "limitations": [
                  "Filtrado retrospectivo por coincidencia final: posible sesgo de selección.",
                  "No se incluyen estados posteriores a ambigüedades ni compras deshechas.",
                  "Los consumibles y trinkets ignorados no se usan como características.",
                  "Se conservan firmas repetidas: pueden representar compras múltiples legítimas.",
              ]}
    seen_games = {}
    for split in ("train", "validation"):
        path = source / f"{split}.jsonl"
        counts, games = Counter(), set()
        times = Counter()
        with path.open(encoding="utf-8") as input_file, \
                (output / f"{split}.jsonl").open("x", encoding="utf-8") as result_file:
            for line in input_file:
                row = json.loads(line)
                games.add(row["gameId"])
                result, reason = convert(row, vocab["items"])
                counts[reason] += 1
                if result is not None:
                    minute = row["timestampMs"] // 60000
                    times[f"{minute // 10 * 10:02d}-{minute // 10 * 10 + 9:02d}"] += 1
                    result_file.write(json.dumps(result, ensure_ascii=False) + "\n")
        seen_games[split] = games
        report["splits"][split] = {
            "counts": dict(counts), "sourceGames": len(games),
            "acceptedByMinute": dict(sorted(times.items())), "sourceSha256": digest(path),
        }
    overlap = seen_games["train"] & seen_games["validation"]
    if overlap:
        raise ValueError(f"Hay {len(overlap)} partidas en más de una partición")
    (output / "manifest.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({k: v["counts"] for k, v in report["splits"].items()}))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=Path)
    parser.add_argument("--vocab", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    prepare(args.input, args.vocab, args.output)
