import json
from pathlib import Path
import sqlite3
import tempfile
import unittest
from contextlib import closing

from build_match_database import build


class MatchDatabaseTest(unittest.TestCase):
    def test_import_deduplicates_and_excludes_mismatched_outcomes(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            players = [
                {"GameId": "ok", "Side": side, "Role": role, "Champion": "Ahri",
                 "PlayerWin": "Yes" if side == 1 else "No", "Items": ["Boots"]}
                for side in (1, 2)
                for role in ("Top", "Jungle", "Mid", "Bot", "Support")
            ]
            for folder in ("a", "b"):
                source = root / folder
                source.mkdir()
                games = [{"GameId": "ok", "Winner": 1, "QueueId": 420}]
                rows = list(players)
                if folder == "a":
                    games.append({"GameId": "bad", "Winner": 1, "QueueId": 420})
                    rows.extend({**p, "GameId": "bad", "PlayerWin": "No"} for p in players)
                (source / "games.jsonl").write_text(
                    "".join(json.dumps(g) + "\n" for g in games), encoding="utf-8"
                )
                (source / "players.jsonl").write_text(
                    "".join(json.dumps(p) + "\n" for p in rows), encoding="utf-8"
                )
            dest = root / "test.sqlite"
            build(root, dest)
            with closing(sqlite3.connect(dest)) as db:
                self.assertEqual(db.execute("select count(*) from matches").fetchone()[0], 1)
                self.assertEqual(db.execute("select count(*) from participants").fetchone()[0], 10)
                self.assertEqual(db.execute("select count(*) from match_sources").fetchone()[0], 2)
                self.assertEqual(db.execute("select count(*) from exclusions").fetchone()[0], 1)
                self.assertEqual(db.execute("pragma foreign_key_check").fetchall(), [])
            with self.assertRaises(FileExistsError):
                build(root, dest)


if __name__ == "__main__":
    unittest.main()
