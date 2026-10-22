import pytest

from idioms.legacy import beta_enabled, legacy_rate


@pytest.mark.skip(reason="rate under review")
def test_legacy_rate():
    assert legacy_rate() == 7


@pytest.mark.xfail(reason="beta flag not ready")
def test_beta_flag_is_on():
    assert beta_enabled()
