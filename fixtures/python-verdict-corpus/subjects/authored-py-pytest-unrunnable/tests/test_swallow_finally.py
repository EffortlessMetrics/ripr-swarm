from unrun.core import swallow_finally


def test_swallow_finally():
    total = swallow_finally(3, 7)
    try:
        assert isinstance(total, int)
    except AssertionError:
        pass
    finally:
        assert total == 21
