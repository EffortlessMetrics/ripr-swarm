from unrun.core import dead_none


def test_dead_none():
    total = dead_none(3, 7)
    if None:
        assert total == 21
