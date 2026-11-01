from uncollected.core import class_inherited_off


class Base:
    __test__ = False


class TestInheritedOff(Base):
    def test_class_inherited_off(self):
        assert class_inherited_off(3, 7) == 21
