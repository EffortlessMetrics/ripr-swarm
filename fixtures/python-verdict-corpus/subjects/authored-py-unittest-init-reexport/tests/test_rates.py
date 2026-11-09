import unittest
import shapes.rates.table as t


class TestRates(unittest.TestCase):
    def test_gold(self):
        assert t.lookup("gold") == 10
