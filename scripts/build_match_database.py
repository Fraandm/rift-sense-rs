"""Importación conservadora de JSONL Riot normalizados a SQLite nuevo.

No modifica datos fuente ni integra automáticamente datos al entrenamiento.
"""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import sqlite3


def lines(path):
    with path.open(encoding="utf-8") as stream:
        for n, line in enumerate(stream, 1):
            try:
                yield json.loads(line)
            except ValueError as exc:
                raise ValueError(f"JSON inválido en {path}:{n}") from exc


def build(raw, destination):
    if destination.exists():
        raise FileExistsError(f"No se sobrescribe {destination}")
    db = sqlite3.connect(destination)
    counts = Counter()
    try:
        db.executescript("""
        PRAGMA foreign_keys=ON;
        CREATE TABLE matches (
          game_id TEXT PRIMARY KEY, region TEXT, queue_id INTEGER,
          date_utc TEXT, patch TEXT, winner INTEGER CHECK(winner IN (1,2))
        );
        CREATE TABLE participants (
          game_id TEXT NOT NULL REFERENCES matches(game_id), side INTEGER NOT NULL,
          role TEXT NOT NULL, champion TEXT NOT NULL, won INTEGER NOT NULL,
          PRIMARY KEY(game_id,side,role)
        );
        CREATE TABLE participant_items (
          game_id TEXT NOT NULL, side INTEGER NOT NULL, role TEXT NOT NULL,
          position INTEGER NOT NULL, name TEXT NOT NULL,
          PRIMARY KEY(game_id,side,role,position),
          FOREIGN KEY(game_id,side,role) REFERENCES participants(game_id,side,role)
        );
        CREATE TABLE match_sources (
          game_id TEXT NOT NULL REFERENCES matches(game_id), source TEXT NOT NULL,
          PRIMARY KEY(game_id,source)
        );
        CREATE TABLE exclusions (
          game_id TEXT, source TEXT NOT NULL, reason TEXT NOT NULL
        );
        """)
        for game_file in sorted(raw.rglob("games.jsonl")):
            player_file = game_file.with_name("players.jsonl")
            source = str(game_file.relative_to(raw))
            if not player_file.exists():
                counts["missing_player_file"] += 1
                continue
            grouped = defaultdict(list)
            for player in lines(player_file):
                grouped[player.get("GameId")].append(player)
            for game in lines(game_file):
                gid = game.get("GameId")
                players = grouped.get(gid, [])
                roles = {(p.get("Side"), p.get("Role")) for p in players}
                valid = (
                    isinstance(gid, str) and bool(gid)
                    and game.get("Winner") in (1, 2)
                    and game.get("QueueId") in (420, 440)
                    and len(players) == 10 and len(roles) == 10
                    and all(p.get("Side") in (1, 2) and p.get("Role") in
                            ("Top", "Jungle", "Mid", "Bot", "Support")
                            and p.get("Champion")
                            and p.get("PlayerWin") in ("Yes", "No")
                            and (p.get("PlayerWin") == "Yes") == (p.get("Side") == game.get("Winner"))
                            for p in players)
                )
                if not valid:
                    db.execute("INSERT INTO exclusions VALUES (?,?,?)", (gid, source, "registro incompleto o inconsistente"))
                    counts["excluded"] += 1
                    continue
                exists = db.execute("SELECT 1 FROM matches WHERE game_id=?", (gid,)).fetchone()
                if not exists:
                    db.execute("INSERT INTO matches VALUES (?,?,?,?,?,?)", (
                        gid, game.get("Region"), game.get("QueueId"),
                        game.get("DateUtc"), game.get("RiotPatch"), game["Winner"],
                    ))
                    for p in players:
                        side, role = p["Side"], p["Role"]
                        db.execute("INSERT INTO participants VALUES (?,?,?,?,?)",
                                   (gid, side, role, p["Champion"], int(side == game["Winner"])))
                        for pos, name in enumerate(p.get("Items") or []):
                            db.execute("INSERT INTO participant_items VALUES (?,?,?,?,?)",
                                       (gid, side, role, pos, name))
                    counts["matches"] += 1
                else:
                    counts["duplicate_source_rows"] += 1
                db.execute("INSERT OR IGNORE INTO match_sources VALUES (?,?)", (gid, source))
            db.commit()
        print(json.dumps(dict(counts), ensure_ascii=False))
    finally:
        db.close()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--raw", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    build(args.raw, args.output)
