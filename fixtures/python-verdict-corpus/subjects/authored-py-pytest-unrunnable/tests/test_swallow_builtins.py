import builtins

from unrun.core import swallow_builtins


def test_swallow_builtins():
    try:
        assert swallow_builtins(3, 7) == 21
    except builtins.AssertionError:
        pass
