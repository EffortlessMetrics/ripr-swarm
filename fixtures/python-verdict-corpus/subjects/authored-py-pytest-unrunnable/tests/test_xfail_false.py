import pytest

from unrun.core import xfail_false


@pytest.mark.xfail(False, reason="never expected to fail")
def test_xfail_false():
    assert xfail_false(3, 7) == 21
