import unittest

from idioms.stock import Stock


class StockTests(unittest.TestCase):
    def setUp(self):
        self.stock = Stock(5)

    def test_take_reduces_the_count(self):
        self.assertEqual(self.stock.take(2), 3)

    def test_overtake_raises(self):
        with self.assertRaises(ValueError):
            self.stock.take(6)
