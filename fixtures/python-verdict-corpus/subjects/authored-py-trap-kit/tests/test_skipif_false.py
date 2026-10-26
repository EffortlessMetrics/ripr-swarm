import pytest

from trapkit.core import skipif_false


@pytest.mark.skipif(False, reason="never skipped")
def test_skipif_false_total():
    assert skipif_false(3, 7) == 21
