"""Auditoría descriptiva de cachés Riot; nunca modifica archivos de entrada."""
import argparse
from collections import Counter
import json
from pathlib import Path

from timeline_inventory import audit_final_inventory, reconstruct, summarize


def audit(paths):
    counts = Counter()
    only_final = Counter()
    only_reconstructed = Counter()
    seen = set()
    for path in paths:
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
            match = payload["match"]
            timeline = payload["timeline"]
            match_id = match.get("metadata", {}).get("matchId")
            if not match_id:
                raise ValueError("Partida sin identificador")
            if match_id in seen:
                counts["duplicateMatchIds"] += 1
                continue
            seen.add(match_id)
            report = audit_final_inventory(match, timeline)
            summary = summarize(reconstruct(timeline))
        except (OSError, ValueError, KeyError, TypeError, AttributeError):
            counts["invalidFiles"] += 1
            continue
        counts["matches"] += 1
        counts["events"] += summary["events"]
        counts["apparentlyReliableEvents"] += summary["reliableEvents"]
        counts["possiblyUndoneEvents"] += summary["potentiallyUndoneEvents"]
        counts["starterSales"] += summary["starterSales"]
        counts["apparentlyReliableStarterSales"] += summary["reliableStarterSales"]
        counts["exactFinalInventories"] += report["matchingFinal"]
        for player in report["participants"]:
            counts["participants"] += 1
            only_final.update(player["onlyFinal"])
            only_reconstructed.update(player["onlyReconstructed"])
    return {
        "counts": dict(counts),
        "onlyInFinal": dict(only_final.most_common(20)),
        "onlyInReconstruction": dict(only_reconstructed.most_common(20)),
        "limitations": (
            "Coincidencias finales son controles retrospectivos, no entradas del modelo. "
            "Los eventos aparentemente fiables no están verificados individualmente."
        ),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cache", type=Path)
    parser.add_argument("--region-prefix", required=True, help="Región sin guion bajo, p. ej. EUW1")
    parser.add_argument("--limit", type=int, default=200)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    if not args.region_prefix.isalnum() or args.limit < 1:
        parser.error("Usa un prefijo alfanumérico sin guion bajo y un límite positivo")
    paths = sorted(args.cache.glob(args.region_prefix.upper() + "_*.json"))[:args.limit]
    report = audit(paths)
    report["sample"] = {
        "regionPrefix": args.region_prefix.upper(),
        "selection": "primeros nombres ordenados; no aleatoria",
        "selectedFiles": len(paths),
    }
    with args.output.open("x", encoding="utf-8") as output:
        json.dump(report, output, ensure_ascii=False, indent=2)
        output.write("\n")
    print(json.dumps(report["counts"], ensure_ascii=False))


if __name__ == "__main__":
    main()
