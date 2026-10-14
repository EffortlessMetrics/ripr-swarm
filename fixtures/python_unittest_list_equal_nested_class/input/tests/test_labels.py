import unittest
from src.labels import labels


class Outer(unittest.TestCase):
    class Inner(unittest.TestCase):
        def test_labels(self):
            self.assertListEqual(labels(), [1, 2])
