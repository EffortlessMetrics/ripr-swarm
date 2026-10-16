from src.shop import fee
import unittest

class FeeTest(unittest.TestCase):
    def test_fee(self):
        with self.assertRaisesRegex(ValueError, 'negative'):
            fee(-1)
