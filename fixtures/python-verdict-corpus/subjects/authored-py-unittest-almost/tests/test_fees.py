import unittest

from shop.fees import (
    default_fee,
    delta_fee,
    helper_fee,
    none_fee,
    places_fee,
    pos_fee,
    spread_fee,
)
from tests import checks

LOOSE = {"places": 0}


class TestFees(unittest.TestCase):
    def test_default_fee(self):
        self.assertAlmostEqual(default_fee(10), 30.0)

    def test_none_fee(self):
        self.assertAlmostEqual(none_fee(10), 30.0, places=None)

    def test_places_fee(self):
        self.assertAlmostEqual(places_fee(10), 30.0, places=0)

    def test_delta_fee(self):
        self.assertAlmostEqual(delta_fee(10), 30.0, delta=1)

    def test_pos_fee(self):
        self.assertAlmostEqual(pos_fee(10), 30.0, None, "msg", 1)

    def test_spread_fee(self):
        self.assertAlmostEqual(spread_fee(10), 30.0, **LOOSE)

    def test_helper_fee(self):
        checks.assertAlmostEqual(helper_fee(10), 30.0)
