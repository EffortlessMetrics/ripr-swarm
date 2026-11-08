import unittest

from derived.core import scale_classattr


class ClassAttrTest(unittest.TestCase):
    expected = scale_classattr(2)

    def test_scale(self):
        self.assertEqual(scale_classattr(2), self.expected)
