import pytest

from unrun.core import skip_mark

pytestmark = pytest.mark.skip(reason="disabled")


def test_skip_mark():
    assert skip_mark(3, 7) == 21
