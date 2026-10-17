import unittest
from src.labels import labels
class Checks(unittest.TestCase):
    def test_labels(self):
        self.assertListEqual(labels().fixed_list(), [1, 2])
