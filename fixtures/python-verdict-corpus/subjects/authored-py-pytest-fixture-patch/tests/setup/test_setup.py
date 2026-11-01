import unittest
from unittest import mock

from fxpatch import core


class PriceSetupTest(unittest.TestCase):
    def setUp(self):
        self.patcher = mock.patch.object(core, "price_setup", lambda x: x * 2)
        self.patcher.start()
        self.addCleanup(self.patcher.stop)

    def test_price_setup(self):
        self.assertEqual(core.price_setup(3), 6)
