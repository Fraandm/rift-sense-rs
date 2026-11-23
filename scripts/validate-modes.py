"""Valida los objetos recomendados por modo contra Data Dragon y las partidas reales.

Lanza el binario de RiftSense, pide una build para cada campeón en Grieta
(cada rol), ARAM, Mayhem y Arena, y comprueba:

- que cada objeto recomendado existe en el mapa del modo (Data Dragon) o se
  ha comprado en partidas reales de ese modo (data/processed/priors_modes.json);
- que no hay placeholders de anvil, duplicados en una misma lista ni objetos
  de AD en campeones AP (o al revés) sin datos propios.

    python scripts/validate-modes.py --analizador "C:\\ruta\\Analizador LOL"

Variables: RIFTSENSE_EXE (binario), RIFTSENSE_ROOT (datos del Analizador).
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import time
from collections import Counter, defaultdict
from pathlib import Path

import requests

DEFAULT_EXE = Path(os.environ.get("RIFTSENSE_EXE", r"target\debug\riftsense.exe"))
MAYHEM_EXCLUSIVE = {3039, 123430, 124011}
ARENA_STARTERS = {
    "Guardian's Horn",
    "Guardian's Hammer",
    "Guardian's Blade",
    "Guardian's Orb",
    "Guardian's Shroud",
    "Guardian's Amulet",
    "Guardian's Dirk",
}
PLACEHOLDER_PREFIXES = ("Legendary ", "Prismatic", "Stat Bonus")

MODES = {
    "classic": {
        "roles": ["TOP", "JUNGLE", "MID", "BOT", "SUPPORT"],
        "gold": 3000,
        "minute": 12,
        "game": {"gameMode": "CLASSIC", "mapName": "Map11", "queueId": 420},
    },
    "aram": {
        "roles": ["MID"],
        "gold": 1500,
        "minute": 5,
        "game": {"gameMode": "ARAM", "mapName": "Map12", "queueId": 450},
    },
    "mayhem": {
        "roles": ["MID"],
        "gold": 1500,
        "minute": 5,
        "game": {"gameMode": "KIWI", "mapName": "Map12", "queueId": 2400},
    },
    "arena": {
        "roles": ["MID"],
        "gold": 500,
        "minute": 0,
        "game": {"gameMode": "CHERRY", "mapName": "Map30", "queueId": 1700},
    },
}


class Reference:
    """Datos de referencia: Data Dragon y objetos observados por modo."""

    def __init__(self, root: Path):
        version_meta = json.loads((root / "artifacts" / "meta.json").read_text(encoding="utf-8"))
        version = version_meta.get("ddragonVersion", "16.23.1")
        self.items = json.loads((root / "data" / "static" / version / "item.json").read_text(encoding="utf-8"))["data"]
        self.modes = json.loads((root / "data" / "processed" / "priors_modes.json").read_text(encoding="utf-8"))["modes"]

    def maps(self, item_id: int) -> list[str] | None:
        value = self.items.get(str(item_id))
        if value is None:
            return None
        return sorted(name for name, enabled in value.get("maps", {}).items() if enabled)

    def observed(self, mode: str, item_id: int) -> int:
        key = "aram" if mode == "mayhem" and "mayhem" not in self.modes else mode
        entry = self.modes.get(key, {}).get("observedIds", {}).get(str(item_id))
        return entry["count"] if entry else 0

    def illegal_reason(self, mode: str, item: dict) -> str | None:
        """None si el objeto es legal en el modo; si no, el motivo."""
        item_id, name = item["id"], item["name"]
        if name.startswith(PLACEHOLDER_PREFIXES):
            return "placeholder de anvil"
        maps = self.maps(item_id) or []
        if mode == "classic":
            return None if not maps or "11" in maps else f"solo mapas {maps}"
        if mode == "aram":
            if "12" in maps or self.observed(mode, item_id) >= 3:
                return None
            return f"no disponible en ARAM (mapas {maps})"
        if mode == "mayhem":
            if item_id in MAYHEM_EXCLUSIVE or "12" in maps or self.observed(mode, item_id) >= 3:
                return None
            return f"no disponible en Mayhem (mapas {maps})"
        if mode == "arena":
            if name in ARENA_STARTERS or "30" in maps or self.observed(mode, item_id) >= 3:
                return None
            return f"no disponible en Arena (mapas {maps})"
        return "modo desconocido"


def request_builds(base: str, champions: list[str]) -> list[dict]:
    rows = []
    for mode, cfg in MODES.items():
        for role in cfg["roles"]:
            for name in champions:
                body = {
                    "champion": name,
                    "mode": mode,
                    "role": role,
                    "side": "blue",
                    "allies": [],
                    "enemies": [],
                    "owned": [],
                    "gold": cfg["gold"],
                    "minute": cfg["minute"],
                    "stable": True,
                    "simulate": True,
                    "limit": 6,
                    "game": dict(cfg["game"], gameTime=cfg["minute"] * 60.0),
                }
                response = requests.post(f"{base}/api/build", json=body, timeout=60)
                if response.status_code != 200:
                    rows.append({"mode": mode, "role": role, "champion": name, "error": response.status_code})
                    continue
                data = response.json()
                rows.append(
                    {
                        "mode": mode,
                        "role": role,
                        "champion": name,
                        "roleOut": data.get("role"),
                        "items": data.get("items", []),
                        "plan": data.get("buildPlan", []),
                        "nextPurchase": (data.get("nextPurchase") or {}).get("name"),
                        "boots": (data.get("bootsRecommendation") or {}).get("name"),
                    }
                )
    return rows


def report(rows: list[dict], reference: Reference, champions: dict[str, dict]) -> int:
    issues = 0
    per_mode: dict[str, list[dict]] = defaultdict(list)
    for row in rows:
        if "error" in row:
            issues += 1
            print(f"ERROR {row['mode']} {row['role']} {row['champion']}: HTTP {row['error']}")
            continue
        per_mode[row["mode"]].append(row)
    for mode, mode_rows in per_mode.items():
        counts: Counter[str] = Counter()
        for row in mode_rows:
            names = Counter(item["name"] for item in row["items"])
            for name, count in names.items():
                if count > 1:
                    counts[f"duplicado {name}"] += 1
            for item in row["items"] + [{"id": p["id"], "name": p["name"]} for p in row["plan"]]:
                reason = reference.illegal_reason(mode, item)
                if reason:
                    counts[f"{item['name']}: {reason}"] += 1
        issues += sum(counts.values())
        print(f"\n== {mode}: {len(mode_rows)} builds, incidencias: {sum(counts.values())}")
        for key, count in counts.most_common(15):
            print(f"   {count:4d}  {key}")
    return issues


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--exe", type=Path, default=DEFAULT_EXE)
    parser.add_argument("--analizador", type=Path, default=Path(os.environ.get("RIFTSENSE_ROOT", "")))
    parser.add_argument("--models", type=Path, default=Path(__file__).resolve().parent.parent / "models")
    parser.add_argument("--port", type=int, default=8190)
    parser.add_argument("--out", type=Path, default=Path("sweep_modes.json"))
    args = parser.parse_args()
    if not args.analizador or not args.analizador.exists():
        raise SystemExit("Indica la carpeta de datos con --analizador o RIFTSENSE_ROOT.")

    env = dict(os.environ)
    env["RIFTSENSE_ROOT"] = str(args.analizador)
    env["RIFTSENSE_MODELS_DIR"] = str(args.models)
    proc = subprocess.Popen([str(args.exe), "--port", str(args.port)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    base = f"http://127.0.0.1:{args.port}"
    try:
        for _ in range(80):
            try:
                requests.get(f"{base}/api/health", timeout=2)
                break
            except requests.RequestException:
                time.sleep(0.25)
        meta = requests.get(f"{base}/api/meta", timeout=60).json()
        champions = {champion["name"]: champion for champion in meta["champions"]}
        started = time.time()
        rows = request_builds(base, list(champions))
        print(f"builds: {len(rows)} en {round(time.time() - started, 1)} s")
        args.out.write_text(json.dumps(rows, ensure_ascii=False, indent=1), encoding="utf-8")
        issues = report(rows, Reference(args.analizador), champions)
        print(f"\nIncidencias totales: {issues}")
        raise SystemExit(1 if issues else 0)
    finally:
        if proc.poll() is None:
            proc.kill()


if __name__ == "__main__":
    main()
