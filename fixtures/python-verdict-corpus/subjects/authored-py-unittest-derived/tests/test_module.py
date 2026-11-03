import unittest

from derived.core import scale_module


EXPECTED = scale_module(2)


class ModuleTest(unittest.TestCase):
    def test_scale(self):
        self.assertEqual(scale_module(2), EXPECTED)
