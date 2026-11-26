import datetime as dt
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np
from audit_riot_training import audit


class AuditTest(unittest.TestCase):
    def test_deduplication_temporal_split_and_bad_result(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            np.savez(root / "old.npz", game_ids=np.array(["old"]))
            games = [
                {"GameId": gid, "DateUtc": date, "Winner": 1, "QueueId": 420}
                for gid, date in [("old", "2026-11-01"), ("a", "2026-11-14"),
                                  ("b", "2026-11-15"), ("c", "2026-11-21"),
                                  ("bad", "2026-11-21")]
            ]
            players = [
                {"GameId": game["GameId"], "Side": side, "Role": role, "Champion": "Ahri",
                 "PlayerWin": "Yes" if side == 1 and game["GameId"] != "bad" else "No"}
                for game in games
                for side in (1, 2)
                for role in ("Top", "Jungle", "Mid", "Bot", "Support")
            ]
            for folder in ("one", "two"):
                source = root / folder
                source.mkdir()
                (source / "games.jsonl").write_text(
                    "".join(json.dumps(g) + "\n" for g in games), encoding="utf-8"
                )
                (source / "players.jsonl").write_text(
                    "".join(json.dumps(p) + "\n" for p in players), encoding="utf-8"
                )
            report = audit(root, root / "old.npz", dt.date(2026, 11, 15), dt.date(2026, 11, 21))
            self.assertEqual(report["partitions"], {"train": 1, "validation": 1, "test": 1})
            self.assertEqual(report["counts"]["existing_dataset"], 1)
            self.assertEqual(report["counts"]["invalid_match"], 1)
            self.assertEqual(report["counts"]["duplicate_source_rows"], 5)


if __name__ == "__main__":
    unittest.main()
