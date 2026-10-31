import pytest

from unrun.core import swallow_fail


def test_swallow_fail():
    try:
        assert swallow_fail(3, 7) == 21
    except AssertionError:
        pytest.fail("wrong total")
