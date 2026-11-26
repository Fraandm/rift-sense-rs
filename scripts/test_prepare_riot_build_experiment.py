import datetime as dt
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np

from prepare_riot_build_experiment import prepare


class PrepareTest(unittest.TestCase):
    def test_excludes_unknown_and_keeps_teams_and_splits(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            raw, processed = root / "raw", root / "processed"
            raw.mkdir()
            processed.mkdir()
            np.savez(processed / "dataset.npz", game_ids=np.array(["old"]))
            vocab = {
                "champions": {"Ahri": 1}, "items": {"1001": 1},
                "roleIdx": {"TOP": 1, "JUNGLE": 2, "MID": 3, "BOT": 4, "SUPPORT": 5},
            }
            (processed / "vocab.json").write_text(json.dumps(vocab), encoding="utf-8")
            meta = root / "meta.json"
            meta.write_text(json.dumps({"items": [{"id": 1001, "name": "Boots"}]}), encoding="utf-8")
            games = [
                {"GameId": gid, "DateUtc": date, "RiotPatch": "16.23", "Winner": 1, "QueueId": 420}
                for gid, date in (("old", "2026-11-01"), ("train", "2026-11-14"),
                                  ("val", "2026-11-15"), ("test", "2026-11-21"))
            ]
            players = [
                {"GameId": game["GameId"], "Side": side, "Role": role, "Champion": "Ahri",
                 "PlayerWin": "Yes" if side == 1 else "No",
                 "Items": ["unknown"] if side == 1 and role == "Top" else ["Boots"]}
                for game in games for side in (1, 2)
                for role in ("Top", "Jungle", "Mid", "Bot", "Support")
            ]
            (raw / "games.jsonl").write_text("".join(json.dumps(g) + "\n" for g in games), encoding="utf-8")
            (raw / "players.jsonl").write_text("".join(json.dumps(p) + "\n" for p in players), encoding="utf-8")
            out = root / "output"
            prepare(raw, processed, meta, out, dt.date(2026, 11, 15), dt.date(2026, 11, 21))
            with np.load(out / "riot-build-rows.npz", allow_pickle=False) as data:
                self.assertEqual(len(data["champ"]), 27)
                self.assertEqual(set(data["split"].tolist()), {0, 1, 2})
                self.assertEqual(data["ally"].shape, (27, 5))
                self.assertEqual(data["items"].shape, (27, 7))
                self.assertFalse(any(data["game_id"] == "old"))
            with self.assertRaises(FileExistsError):
                prepare(raw, processed, meta, out, dt.date(2026, 11, 15), dt.date(2026, 11, 21))


if __name__ == "__main__":
    unittest.main()
