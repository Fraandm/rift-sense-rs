import unittest
import json
import tempfile
from pathlib import Path

from audit_timeline_inventory import audit
from timeline_inventory import audit_final_inventory, reconstruct, summarize


class InventoryTest(unittest.TestCase):
    def timeline(self, events):
        return {"metadata": {"matchId": "LA1_test"}, "info": {"frames": [{"events": events}]}}

    def test_purchase_sale_and_component(self):
        events = [
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 1, "itemId": 1056},
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 2, "itemId": 1001},
            {"type": "ITEM_DESTROYED", "participantId": 1, "timestamp": 3, "itemId": 1001},
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 4, "itemId": 3020},
            {"type": "ITEM_SOLD", "participantId": 1, "timestamp": 5, "itemId": 1056},
        ]
        rows = reconstruct(self.timeline(events))
        self.assertEqual(rows[2]["inventoryBefore"], [1056])
        self.assertEqual(rows[3]["inventoryBefore"], [1056, 3020])
        self.assertTrue(all(row["reliable"] for row in rows))
        self.assertEqual(summarize(rows)["reliableStarterSales"], 1)

    def test_undo_and_missing_item_fail_closed(self):
        events = [
            {"type": "ITEM_UNDO", "participantId": 1, "timestamp": 1},
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 2, "itemId": 1056},
            {"type": "ITEM_SOLD", "participantId": 2, "timestamp": 3, "itemId": 1001},
            {"type": "ITEM_PURCHASED", "participantId": 2, "timestamp": 4, "itemId": 3020},
        ]
        rows = reconstruct(self.timeline(events))
        self.assertTrue(all(not row["reliable"] for row in rows))

    def test_reject_empty_timeline(self):
        with self.assertRaises(ValueError):
            reconstruct({"info": {"frames": []}})

    def test_simultaneous_component_consumption_is_ambiguous(self):
        events = [
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 1, "itemId": 1001},
            {"type": "ITEM_DESTROYED", "participantId": 1, "timestamp": 2, "itemId": 1001},
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 2, "itemId": 3020},
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 3, "itemId": 1056},
        ]
        rows = reconstruct(self.timeline(events))
        self.assertFalse(rows[1]["reliable"])
        self.assertTrue(rows[2]["reliable"])

    def test_final_inventory_comparison_and_match_identity(self):
        match = {
            "metadata": {"matchId": "LA1_test"},
            "info": {"participants": [
                {"participantId": pid, "item0": 1056 if pid == 1 else 0}
                for pid in range(1, 11)
            ]},
        }
        timeline = self.timeline([
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 1, "itemId": 1056}
        ])
        report = audit_final_inventory(match, timeline)
        self.assertEqual(report["matchingFinal"], 10)
        match["metadata"]["matchId"] = "other"
        with self.assertRaises(ValueError):
            audit_final_inventory(match, timeline)

    def test_undo_excludes_preceding_decision_only_for_same_player(self):
        rows = reconstruct(self.timeline([
            {"type": "ITEM_PURCHASED", "participantId": 1, "timestamp": 1, "itemId": 1056},
            {"type": "ITEM_PURCHASED", "participantId": 2, "timestamp": 2, "itemId": 1001},
            {"type": "ITEM_SOLD", "participantId": 1, "timestamp": 3, "itemId": 1056},
            {"type": "ITEM_UNDO", "participantId": 1, "timestamp": 4},
        ]))
        self.assertTrue(rows[0]["outcomeKnown"])
        self.assertTrue(rows[1]["outcomeKnown"])
        self.assertFalse(rows[2]["outcomeKnown"])
        self.assertEqual(summarize(rows)["reliableStarterSales"], 0)

    def test_batch_audit_skips_duplicate_match(self):
        payload = {
            "match": {
                "metadata": {"matchId": "LA1_test"},
                "info": {"participants": [{"participantId": i} for i in range(1, 11)]},
            },
            "timeline": self.timeline([]),
        }
        with tempfile.TemporaryDirectory() as temp:
            first = Path(temp) / "one.json"
            second = Path(temp) / "two.json"
            first.write_text(json.dumps(payload), encoding="utf-8")
            second.write_text(json.dumps(payload), encoding="utf-8")
            report = audit([first, second])
        self.assertEqual(report["counts"]["matches"], 1)
        self.assertEqual(report["counts"]["duplicateMatchIds"], 1)
        self.assertEqual(report["counts"]["participants"], 10)


if __name__ == "__main__":
    unittest.main()
