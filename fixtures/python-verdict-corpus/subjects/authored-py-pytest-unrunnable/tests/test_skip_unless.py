import unittest

from unrun.core import skip_unless


class Total(unittest.TestCase):
    @unittest.skipUnless(True, "always runs")
    def test_skip_unless(self):
        self.assertEqual(skip_unless(3, 7), 21)
