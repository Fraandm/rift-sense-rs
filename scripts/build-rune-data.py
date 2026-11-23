#!/usr/bin/env python3
"""Genera el catálogo de runas y las páginas históricas de RiftSense.

Lee las partidas cacheadas por el scraper de Riot, extrae las páginas de runas
de cada participante y construye una página recomendada por campeón y rol.
Las runas que no tengan muestra suficiente las resuelve el motor con reglas.
"""

from __future__ import annotations

import argparse
import collections
import json
import os
import urllib.request
from pathlib import Path

ROLE_MAP = {
    "TOP": "TOP",
    "JUNGLE": "JUNGLE",
    "MIDDLE": "MID",
    "BOTTOM": "BOT",
    "UTILITY": "SUPPORT",
}

STAT_SHARDS = {
    5001: ("Vida por nivel", "perk-images/StatMods/StatModsHealthScalingIcon.png"),
    5002: ("Armadura", "perk-images/StatMods/StatModsArmorIcon.png"),
    5003: ("Resistencia mágica", "perk-images/StatMods/StatModsMagicResIcon.png"),
    5005: ("Velocidad de ataque", "perk-images/StatMods/StatModsAttackSpeedIcon.png"),
    5007: ("Aceleración de habilidades", "perk-images/StatMods/StatModsCDRScalingIcon.png"),
    5008: ("Fuerza adaptable", "perk-images/StatMods/StatModsAdaptiveForceIcon.png"),
    5011: ("Vida", "perk-images/StatMods/StatModsHealthPlusIcon.png"),
    5013: ("Tenacidad y ralentización", "perk-images/StatMods/StatModsTenacityIcon.png"),
}

DEFAULT_TREES = [8000, 8100, 8200, 8300, 8400]


def fetch_json(url: str) -> object:
    request = urllib.request.Request(url, headers={"User-Agent": "RiftSense/rune-builder"})
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def load_catalog(patch: str) -> tuple[list[dict], list[dict], dict[int, dict]]:
    url = f"https://ddragon.leagueoflegends.com/cdn/{patch}/data/es_ES/runesReforged.json"
    raw = fetch_json(url)
    trees: list[dict] = []
    runes: list[dict] = []
    by_id: dict[int, dict] = {}
    for tree in raw:
        tree_entry = {
            "id": int(tree["id"]),
            "key": tree["key"],
            "name": tree["name"],
            "icon": tree["icon"],
        }
        trees.append(tree_entry)
        for slot_index, slot in enumerate(tree.get("slots", [])):
            for rune in slot.get("runes", []):
                entry = {
                    "id": int(rune["id"]),
                    "key": rune["key"],
                    "name": rune["name"],
                    "icon": rune["icon"],
                    "tree": int(tree["id"]),
                    "slot": slot_index,
                }
                runes.append(entry)
                by_id[entry["id"]] = entry
    return trees, runes, by_id


def top(counter: collections.Counter) -> tuple[int | None, int]:
    if not counter:
        return None, 0
    value, count = counter.most_common(1)[0]
    return value, count


def build_page(samples: list[dict], by_id: dict[int, dict]) -> dict | None:
    if not samples:
        return None
    exact: collections.Counter = collections.Counter()
    wins: collections.Counter = collections.Counter()
    keystones: collections.Counter = collections.Counter()
    primary_trees: collections.Counter = collections.Counter()
    secondary_trees: collections.Counter = collections.Counter()
    primary_slots = [collections.Counter() for _ in range(3)]
    secondary_slots = [collections.Counter() for _ in range(3)]
    shard_slots = [collections.Counter() for _ in range(3)]
    tree_of: dict[int, int] = {}

    for sample in samples:
        styles = sample["styles"]
        primary = next((s for s in styles if s.get("description") == "primaryStyle"), styles[0] if styles else None)
        secondary = next((s for s in styles if s.get("description") == "subStyle"), styles[1] if len(styles) > 1 else None)
        if not primary:
            continue
        primary_sel = [int(s["perk"]) for s in primary.get("selections", [])]
        secondary_sel = [int(s["perk"]) for s in (secondary or {}).get("selections", [])]
        if not primary_sel:
            continue
        keystone = primary_sel[0]
        primary_tree = int(primary["style"])
        secondary_tree = int(secondary["style"]) if secondary else None
        shards = sample.get("shards") or []
        key = (
            keystone,
            primary_tree,
            secondary_tree,
            tuple(primary_sel[1:4]),
            tuple(secondary_sel),
            tuple(shards),
        )
        exact[key] += 1
        if sample.get("win"):
            wins[key] += 1
        keystones[keystone] += 1
        primary_trees[primary_tree] += 1
        if secondary_tree:
            secondary_trees[secondary_tree] += 1
        tree_of[keystone] = primary_tree
        for index, rune in enumerate(primary_sel[1:4]):
            primary_slots[index][rune] += 1
        for index, rune in enumerate(secondary_sel[:3]):
            secondary_slots[index][rune] += 1
        for index, shard in enumerate(shards[:3]):
            if shard:
                shard_slots[index][int(shard)] += 1

    chosen = None
    if exact:
        key, count = exact.most_common(1)[0]
        if count >= 2:
            chosen = {
                "keystone": key[0],
                "primaryTree": key[1],
                "secondaryTree": key[2],
                "primaryRunes": list(key[3]),
                "secondaryRunes": list(key[4]),
                "shards": list(key[5]),
                "n": count,
                "wins": wins[key],
            }
    if chosen is None:
        keystone, _ = top(keystones)
        if keystone is None:
            return None
        primary_tree = tree_of.get(keystone)
        if primary_tree is None:
            return None
        secondary_candidates = [
            (tree, count) for tree, count in secondary_trees.items() if tree != primary_tree
        ]
        if secondary_candidates:
            secondary_tree = max(secondary_candidates, key=lambda item: item[1])[0]
        else:
            secondary_tree = next((tree for tree in DEFAULT_TREES if tree != primary_tree), 8000)
        primary_runes: list[int] = []
        for slot in primary_slots:
            rune, _ = top(slot)
            if rune and rune not in primary_runes:
                primary_runes.append(rune)
        secondary_runes: list[int] = []
        secondary_pairs: list[tuple[int, int, int]] = []
        for index, slot in enumerate(secondary_slots[:3]):
            for rune, count in slot.items():
                if by_id.get(rune, {}).get("tree") == secondary_tree:
                    secondary_pairs.append((count, index, rune))
        used_slots: set[int] = set()
        for count, index, rune in sorted(secondary_pairs, reverse=True):
            if index in used_slots or rune in secondary_runes:
                continue
            used_slots.add(index)
            secondary_runes.append(rune)
            if len(secondary_runes) >= 2:
                break
        shards = []
        for slot in shard_slots:
            shard, _ = top(slot)
            shards.append(shard if shard else 5008)
        chosen = {
            "keystone": keystone,
            "primaryTree": primary_tree,
            "secondaryTree": secondary_tree,
            "primaryRunes": primary_runes[:3],
            "secondaryRunes": secondary_runes[:2],
            "shards": shards,
            "n": len(samples),
            "wins": sum(1 for sample in samples if sample.get("win")),
        }

    if chosen["keystone"] not in by_id:
        return None
    if chosen["primaryTree"] not in {rune["tree"] for rune in by_id.values()}:
        return None
    chosen["primaryRunes"] = [rune for rune in chosen["primaryRunes"] if rune in by_id][:3]
    chosen["secondaryRunes"] = [rune for rune in chosen["secondaryRunes"] if rune in by_id][:2]
    chosen["shards"] = [shard for shard in chosen["shards"] if shard in STAT_SHARDS][:3]
    while len(chosen["shards"]) < 3:
        chosen["shards"].append([5008, 5008, 5001][len(chosen["shards"])])
    return chosen


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", default=os.environ.get("RIFTSENSE_DATA_ROOT", ""))
    parser.add_argument("--patch", default="16.23.1")
    parser.add_argument(
        "--output",
        default=str(Path(__file__).resolve().parents[1] / "web" / "assets" / "data" / "runes.json"),
    )
    args = parser.parse_args()

    if not args.root:
        candidate = Path(__file__).resolve().parents[2] / "Analizador LOL"
        args.root = str(candidate)
    root = Path(args.root)
    cache = root / "data" / "cache" / "riot"
    meta_path = root / "artifacts" / "meta.json"
    if not cache.is_dir() or not meta_path.is_file():
        raise SystemExit(f"No encuentro cache/meta en {root}")

    meta = json.loads(meta_path.read_text(encoding="utf-8"))
    champion_ids = {str(champ["id"]) for champ in meta.get("champions", [])}
    trees, runes, by_id = load_catalog(args.patch)

    pages: dict[str, dict[str, dict]] = {}
    unmatched: collections.Counter = collections.Counter()
    files = sorted(cache.glob("*.json"))
    for path in files:
        try:
            payload = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, ValueError):
            continue
        match = payload.get("match") or {}
        info = match.get("info") or {}
        for participant in info.get("participants", []):
            champion = str(participant.get("championName") or "")
            role = ROLE_MAP.get(str(participant.get("teamPosition") or "").upper())
            perks = participant.get("perks") or {}
            if not champion or not role or not perks.get("styles"):
                continue
            if champion not in champion_ids:
                unmatched[champion] += 1
                continue
            stat_perks = perks.get("statPerks") or {}
            sample = {
                "styles": perks.get("styles", []),
                "shards": [
                    stat_perks.get("offense"),
                    stat_perks.get("flex"),
                    stat_perks.get("defense"),
                ],
                "win": bool(participant.get("win")),
            }
            pages.setdefault(champion, {}).setdefault(role, []).append(sample)

    champion_pages: dict[str, dict[str, dict]] = {}
    for champion, roles in sorted(pages.items()):
        for role, samples in sorted(roles.items()):
            page = build_page(samples, by_id)
            if page and page["n"] >= 2:
                champion_pages.setdefault(champion, {})[role] = page

    output = {
        "patch": args.patch,
        "source": "Riot match cache",
        "matches": len(files),
        "trees": trees,
        "runes": runes,
        "statShards": [
            {"id": shard, "name": name, "icon": icon}
            for shard, (name, icon) in sorted(STAT_SHARDS.items())
        ],
        "championPages": champion_pages,
    }
    output_path = Path(args.output)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(json.dumps(output, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"runas: {output_path}")
    print(f"partidas: {len(files)} | campeones: {len(champion_pages)} | combinaciones: {sum(len(v) for v in champion_pages.values())}")
    if unmatched:
        print("sin mapear:", ", ".join(f"{name}({count})" for name, count in unmatched.most_common(10)))


if __name__ == "__main__":
    main()
