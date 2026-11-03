import unittest

from derived.core import scale_reference, reference_double


class ReferenceTest(unittest.TestCase):
    def test_scale(self):
        self.assertEqual(scale_reference(2), reference_double(2))
