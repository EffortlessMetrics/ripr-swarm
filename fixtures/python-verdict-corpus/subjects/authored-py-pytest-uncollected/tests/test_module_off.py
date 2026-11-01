from uncollected.core import module_off

__test__ = False


def test_module_off():
    assert module_off(3, 7) == 21
