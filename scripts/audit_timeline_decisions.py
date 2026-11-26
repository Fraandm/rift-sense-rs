"""Audita exclusivamente train/validation sin leer el test reservado."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path


def sha(path):
    h = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def inspect(path, vocabulary):
    counts = Counter()
    times = Counter()
    unknown_targets = Counter()
    game_ids = set()
    signatures = set()
    with path.open(encoding="utf-8") as source:
        for line in source:
            row = json.loads(line)
            counts["decisions"] += 1
            game_ids.add(row["gameId"])
            signature = (row["gameId"], row["participantId"],
                         row["timestampMs"], row["purchasedItemId"])
            if signature in signatures:
                # Varias unidades pueden comprarse en el mismo milisegundo;
                # no asumir duplicación ni eliminar el evento.
                counts["repeatedSignatures"] += 1
            signatures.add(signature)
            target = row["purchasedItemId"]
            if str(target) not in vocabulary:
                counts["targetOutsideVocabulary"] += 1
                unknown_targets[target] += 1
            if any(str(item) not in vocabulary for item in row["inventoryBefore"]):
                counts["inventoryOutsideVocabulary"] += 1
            minute = row["timestampMs"] // 60000
            times[f"{(minute // 10) * 10}-{(minute // 10) * 10 + 9}"] += 1
    counts["games"] = len(game_ids)
    return {
        "counts": dict(counts),
        "minutes": dict(sorted(times.items())),
        "unknownTargetIds": dict(unknown_targets.most_common()),
        "sha256": sha(path),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--vocab", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    vocab = json.loads(args.vocab.read_text(encoding="utf-8"))["items"]
    report = {
        "vocabSha256": sha(args.vocab),
        "train": inspect(args.input / "train.jsonl", vocab),
        "validation": inspect(args.input / "validation.jsonl", vocab),
        "testRead": False,
        "limitations": [
            "Firmas repetidas pueden ser compras múltiples legítimas.",
            "El filtro de inventario final introduce sesgo de selección.",
            "Coincidencia final no demuestra la exactitud de cada estado previo.",
        ],
    }
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps({key: report[key]["counts"] for key in ("train", "validation")}))


if __name__ == "__main__":
    main()
