import pytest

pytest.importorskip("no_such_module_for_unrunnable")

from unrun.core import skip_import


def test_skip_import():
    assert skip_import(3, 7) == 21
