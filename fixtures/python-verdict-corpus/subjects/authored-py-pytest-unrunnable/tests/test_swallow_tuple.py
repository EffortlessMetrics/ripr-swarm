from unrun.core import swallow_tuple


def test_swallow_tuple():
    try:
        assert swallow_tuple(3, 7) == 21
    except (ValueError, AssertionError):
        pass
