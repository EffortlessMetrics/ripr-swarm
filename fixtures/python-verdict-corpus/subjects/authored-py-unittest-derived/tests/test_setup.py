import unittest

from derived.core import scale_setup


class SetupTest(unittest.TestCase):
    def setUp(self):
        self.expected = scale_setup(2)

    def test_scale(self):
        self.assertEqual(scale_setup(2), self.expected)
