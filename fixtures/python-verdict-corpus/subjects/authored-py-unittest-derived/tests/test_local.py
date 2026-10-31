import unittest

from derived.core import scale_local


class LocalTest(unittest.TestCase):
    def test_scale(self):
        expected = scale_local(2)
        self.assertEqual(scale_local(2), expected)
