"""Estados de inventario de timelines Riot para análisis; no modifica el dataset.

Los estados marcados como no fiables se excluyen de decisiones de entrenamiento.
Las compras se registran antes de aplicar el evento, sin información futura.
"""
from __future__ import annotations

import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path


def reconstruct(timeline: dict, *, include_final: bool = False):
    states: dict[int, Counter] = defaultdict(Counter)
    unreliable: set[int] = set()
    rows: list[dict] = []
    last_decision: dict[int, dict] = {}
    frames = timeline.get("info", {}).get("frames")
    if not isinstance(frames, list) or not frames:
        raise ValueError("Timeline sin frames")

    # Componentes pueden desaparecer antes del evento de compra con la misma
    # marca de tiempo. No presentar ese estado intermedio como decisión real.
    combined = {
        (int(event.get("participantId") or 0), int(event.get("timestamp") or 0))
        for frame in frames
        for event in frame.get("events", [])
        if event.get("type") == "ITEM_DESTROYED"
    }
    last_timestamp = -1
    for frame in frames:
        for event in frame.get("events", []):
            kind = event.get("type")
            if kind not in {"ITEM_PURCHASED", "ITEM_SOLD", "ITEM_DESTROYED", "ITEM_UNDO"}:
                continue
            pid = int(event.get("participantId") or 0)
            timestamp = int(event.get("timestamp") or 0)
            if pid not in range(1, 11):
                continue
            # Una secuencia fuera de orden no permite asegurar el estado previo.
            if timestamp < last_timestamp:
                unreliable.update(range(1, 11))
            last_timestamp = timestamp
            inventory = states[pid]
            if kind == "ITEM_UNDO":
                # Riot puede deshacer compras y ventas, incluidos componentes;
                # no inferir una receta incompleta como si fuera fiable.
                previous = last_decision.get(pid)
                if previous is not None:
                    # Etiqueta retrospectiva: no es una característica de
                    # entrada, sirve solo para excluir decisiones deshechas.
                    previous["outcomeKnown"] = False
                unreliable.add(pid)
                continue
            item_id = int(event.get("itemId") or 0)
            if item_id <= 0:
                unreliable.add(pid)
                continue
            before = sorted(inventory.elements())
            if kind in {"ITEM_PURCHASED", "ITEM_SOLD"}:
                row = {
                    "matchId": timeline.get("metadata", {}).get("matchId"),
                    "participantId": pid,
                    "timestampMs": timestamp,
                    "event": kind,
                    "itemId": item_id,
                    "inventoryBefore": before,
                    "reliable": pid not in unreliable and not (
                        kind == "ITEM_PURCHASED" and (pid, timestamp) in combined
                    ),
                    "outcomeKnown": True,
                }
                rows.append(row)
                last_decision[pid] = row
            if kind == "ITEM_PURCHASED":
                inventory[item_id] += 1
            elif inventory[item_id] > 0:
                inventory[item_id] -= 1
                if inventory[item_id] == 0:
                    del inventory[item_id]
            else:
                # No inventar una compra previa ausente de la timeline.
                unreliable.add(pid)
                if kind == "ITEM_SOLD":
                    rows[-1]["reliable"] = False
    if include_final:
        return rows, states, unreliable
    return rows


def audit_final_inventory(match: dict, timeline: dict) -> dict:
    """Compara multiconjuntos finales sin afirmar que expliquen cada diferencia.

    Es un control a posteriori, jamás una característica disponible al comprar.
    Las discrepancias pueden incluir consumibles, mejoras gratuitas o trinkets.
    """
    match_id = match.get("metadata", {}).get("matchId")
    timeline_id = timeline.get("metadata", {}).get("matchId")
    if not match_id or match_id != timeline_id:
        raise ValueError("Los identificadores de partida y timeline no coinciden")
    participants = match.get("info", {}).get("participants", [])
    if len(participants) != 10 or {p.get("participantId") for p in participants} != set(range(1, 11)):
        raise ValueError("Se esperan diez participantes identificados")
    rows, states, unreliable = reconstruct(timeline, include_final=True)
    details = []
    for player in participants:
        pid = player["participantId"]
        final = Counter(int(player.get(f"item{i}") or 0) for i in range(7))
        final.pop(0, None)
        bound = int(player.get("roleBoundItem") or 0)
        if bound:
            final[bound] += 1
        observed = states[pid]
        details.append({
            "participantId": pid,
            "eventsReliable": pid not in unreliable,
            "matchesFinal": observed == final,
            "onlyReconstructed": sorted((observed - final).elements()),
            "onlyFinal": sorted((final - observed).elements()),
        })
    return {
        "matchId": match_id,
        "events": len(rows),
        "matchingFinal": sum(p["matchesFinal"] for p in details),
        "participants": details,
    }


def summarize(rows: list[dict]) -> dict:
    """Conteos descriptivos; no estiman la rentabilidad de vender."""
    starters = {1054, 1055, 1056}
    sales = [
        row for row in rows
        if row["event"] == "ITEM_SOLD" and row["itemId"] in starters
    ]
    reliable_sales = [
        row for row in sales if row["reliable"] and row["outcomeKnown"]
    ]
    return {
        "events": len(rows),
        "reliableEvents": sum(row["reliable"] for row in rows),
        "potentiallyUndoneEvents": sum(not row["outcomeKnown"] for row in rows),
        "starterSales": len(sales),
        "reliableStarterSales": len(reliable_sales),
        "starterSaleMinutes": [round(row["timestampMs"] / 60000, 2) for row in reliable_sales],
    }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path, help="JSON de timeline o caché {match,timeline}")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    payload = json.loads(args.input.read_text(encoding="utf-8"))
    timeline = payload.get("timeline", payload)
    rows = reconstruct(timeline)
    report = summarize(rows)
    if payload.get("match"):
        report["finalInventoryAudit"] = audit_final_inventory(payload["match"], timeline)
    # Rehusar sobrescribir resultados o datos existentes.
    with args.output.open("x", encoding="utf-8") as output:
        for row in rows:
            output.write(json.dumps(row, ensure_ascii=False) + "\n")
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()
