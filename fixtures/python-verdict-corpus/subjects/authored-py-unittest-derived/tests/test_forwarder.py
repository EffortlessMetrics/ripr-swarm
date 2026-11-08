import unittest

from derived.core import scale_forward, relay_forward


class ForwardTest(unittest.TestCase):
    def test_scale(self):
        self.assertEqual(scale_forward(2), relay_forward(2))
