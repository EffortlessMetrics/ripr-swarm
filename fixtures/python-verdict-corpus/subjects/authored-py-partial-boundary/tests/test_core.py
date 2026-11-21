import unittest
from partial.core import value_of

class TestPartial(unittest.TestCase):
    def test_partial(self):
        value = value_of(80)
        self.assertTrue(value)
