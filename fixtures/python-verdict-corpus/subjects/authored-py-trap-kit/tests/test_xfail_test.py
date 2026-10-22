import pytest

from trapkit.core import xfail_test


@pytest.mark.xfail(reason="known bug")
def test_xfail_total():
    assert xfail_test(3, 7) == 21
