import unittest

from derived.core import scale_literal


class LiteralTest(unittest.TestCase):
    def setUp(self):
        self.expected = 4

    def test_scale(self):
        self.assertEqual(scale_literal(2), self.expected)
