import unittest

from derived.core import scale_cancel


class CancelTest(unittest.TestCase):
    def test_scale(self):
        self.assertEqual(scale_cancel(2) - scale_cancel(2), 0)
