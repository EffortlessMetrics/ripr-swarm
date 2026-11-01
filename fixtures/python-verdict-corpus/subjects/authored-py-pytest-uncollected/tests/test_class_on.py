from uncollected.core import class_on


class TestClassOn:
    __test__ = True

    def test_class_on(self):
        assert class_on(3, 7) == 21
