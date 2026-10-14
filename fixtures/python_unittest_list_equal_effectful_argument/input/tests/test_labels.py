import unittest
from src.labels import labels
from src.inputs import abort


class Checks(unittest.TestCase):
    def test_labels(self):
        self.assertListEqual(labels(abort()), [1, 2])
