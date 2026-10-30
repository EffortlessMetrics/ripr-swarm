import pytest

pytest.importorskip("no_such_module_for_trap")

from trapkit.core import module_skipped


def test_module_skipped_total():
    assert module_skipped(3, 7) == 21
