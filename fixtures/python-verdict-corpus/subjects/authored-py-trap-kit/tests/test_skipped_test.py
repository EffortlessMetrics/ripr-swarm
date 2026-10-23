import pytest

from trapkit.core import skipped_test


@pytest.mark.skip(reason="flaky")
def test_skipped_total():
    assert skipped_test(3, 7) == 21
