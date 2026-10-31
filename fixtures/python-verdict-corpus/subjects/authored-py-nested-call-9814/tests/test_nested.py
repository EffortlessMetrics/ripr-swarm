import unittest

from nested.core import f, g, normalize


class NestedCallTests(unittest.TestCase):
    def test_independent_helper_repeat(self):
        assert f(g(5)) == g(5)

    def test_idempotence(self):
        assert normalize(normalize(" x ")) == normalize(" x ")
