import pytest

pytest.importorskip("json")

from trapkit.core import module_loaded


def test_module_loaded_total():
    assert module_loaded(3, 7) == 21
