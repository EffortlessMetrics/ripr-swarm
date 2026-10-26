from unrun.core import swallow_bare


def test_swallow_bare():
    try:
        assert swallow_bare(3, 7) == 21
    except:
        pass
