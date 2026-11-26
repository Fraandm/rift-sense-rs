import datetime as dt
import json
from pathlib import Path
import tempfile
import unittest

import numpy as np
from extract_timeline_decisions import extract


class ExtractTest(unittest.TestCase):
    def test_excludes_existing_and_never_outputs_future_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cache = root / "cache"
            cache.mkdir()
            dataset = root / "old.npz"
            np.savez(dataset, game_ids=np.array(["old"]))
            for gid in ("old", "new"):
                participants = [
                    {"participantId": i, "championId": i, "teamId": 100 if i <= 5 else 200,
                     "teamPosition": "TOP", "item0": 1056 if i == 1 else 0,
                     "win": True, "goldEarned": 99999}
                    for i in range(1, 11)
                ]
                payload = {
                    "match": {"metadata": {"matchId": gid}, "info": {
                        "queueId": 420, "mapId": 11, "gameCreation": 1794700800000,
                        "gameVersion": "16.23", "participants": participants,
                    }},
                    "timeline": {"metadata": {"matchId": gid}, "info": {"frames": [{
                        "events": [{"type": "ITEM_PURCHASED", "participantId": 1,
                                    "timestamp": 100, "itemId": 1056}]
                    }]}},
                }
                (cache / f"{gid}.json").write_text(json.dumps(payload), encoding="utf-8")
            out = root / "out"
            extract(cache, dataset, out, dt.date(2026, 11, 15), dt.date(2026, 11, 21))
            records = [json.loads(line) for path in out.glob("*.jsonl")
                       for line in path.read_text(encoding="utf-8").splitlines()]
            self.assertEqual(len(records), 1)
            self.assertEqual(records[0]["gameId"], "new")
            self.assertEqual(records[0]["inventoryBefore"], [])
            self.assertNotIn("win", records[0])
            self.assertNotIn("goldEarned", records[0])
            with self.assertRaises(FileExistsError):
                extract(cache, dataset, out, dt.date(2026, 11, 15), dt.date(2026, 11, 21))

    def test_later_undo_with_incorrect_final_is_excluded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            cache = root / "cache"
            cache.mkdir()
            dataset = root / "old.npz"
            np.savez(dataset, game_ids=np.array(["old"]))
            players = [{"participantId": i, "championId": i, "teamId": 100,
                        "teamPosition": "TOP", "item0": 1056 if i == 1 else 0}
                       for i in range(1, 11)]
            payload = {
                "match": {"metadata": {"matchId": "new"}, "info": {
                    "queueId": 420, "mapId": 11, "gameCreation": 1794700800000,
                    "participants": players}},
                "timeline": {"metadata": {"matchId": "new"}, "info": {"frames": [{"events": [
                    {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 100, "itemId": 1056},
                    {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 200, "itemId": 1001},
                    {"type": "ITEM_UNDO", "participantId": 1, "timestamp": 300},
                ]}]}},
            }
            # El deshacer deja el estado sin corregir; el control final rechaza
            # el participante en vez de declarar fiable una reconstrucción falsa.
            (cache / "new.json").write_text(json.dumps(payload), encoding="utf-8")
            out = root / "out"
            extract(cache, dataset, out, dt.date(2026, 11, 15), dt.date(2026, 11, 21))
            self.assertFalse(any(p.read_text(encoding="utf-8") for p in out.glob("*.jsonl")))


if __name__ == "__main__":
    unittest.main()
