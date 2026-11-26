import unittest
from prepare_timeline_examples import convert


class ConvertTest(unittest.TestCase):
    def setUp(self):
        self.vocab = {"1001": 1, "1056": 2}
        self.row = {
            "gameId": "LA1_1", "participantId": 1, "championId": 103,
            "teamId": 100, "role": "MIDDLE", "queueId": 420, "dateUtc": "2026-11-01",
            "timestampMs": 123, "inventoryBefore": [2003, 1001], "purchasedItemId": 1056,
        }

    def test_known_consumable_is_ignored(self):
        result, reason = convert(self.row, self.vocab)
        self.assertEqual(reason, "accepted")
        self.assertEqual(result["inventoryIndices"], [1])
        self.assertEqual(result["ignoredNonBuildCount"], 1)

    def test_unknown_item_is_not_silently_dropped(self):
        self.row["inventoryBefore"] = [1042]
        result, reason = convert(self.row, self.vocab)
        self.assertIsNone(result)
        self.assertEqual(reason, "unknown_inventory")

    def test_unknown_or_consumable_target_is_excluded(self):
        self.row["purchasedItemId"] = 2003
        self.assertEqual(convert(self.row, self.vocab)[1], "ignored_target")
        self.row["purchasedItemId"] = 1042
        self.assertEqual(convert(self.row, self.vocab)[1], "unknown_target")


if __name__ == "__main__":
    unittest.main()
