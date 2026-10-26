import pytest

pytest.skip("disabled", allow_module_level=True)

from unrun.core import skip_call


def test_skip_call():
    assert skip_call(3, 7) == 21
