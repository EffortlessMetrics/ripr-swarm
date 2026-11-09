import unittest

from coarse.fees import exact_fee, length_fee, residue_fee, sign_fee


class TestFees(unittest.TestCase):
    def test_sign_fee(self):
        self.assertEqual(sign_fee(2) > 0, True)

    def test_residue_fee(self):
        assert residue_fee(3) % 3 == 0

    def test_length_fee(self):
        assert len(str(length_fee(3))) == 1

    def test_exact_fee(self):
        assert exact_fee(3) == 6
