from uncollected.core import class_off


class TestClassOff:
    __test__ = False

    def test_class_off(self):
        assert class_off(3, 7) == 21
