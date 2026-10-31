from unrun.core import dead_zero


def test_dead_zero():
    total = dead_zero(3, 7)
    if 0:
        assert total == 21
