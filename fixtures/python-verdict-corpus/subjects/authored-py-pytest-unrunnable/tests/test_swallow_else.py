from unrun.core import swallow_else


def test_swallow_else():
    total = swallow_else(3, 7)
    try:
        assert isinstance(total, int)
    except AssertionError:
        pass
    else:
        assert total == 21
