from unrun.core import dead_else


def test_dead_else():
    total = dead_else(3, 7)
    if False:
        pass
    else:
        assert total == 21
