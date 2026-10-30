import pytest

from grid_equality.core import is_admin, is_default_port, is_https, is_saturday, same_pair


def test_admin_role_is_admin():
    assert is_admin("admin")
    assert not is_admin("guest")


@pytest.mark.parametrize("day,want", [(0, False), (5, False), (6, True), (13, True)])
def test_saturday_table(day, want):
    assert is_saturday(day) == want


def test_a_value_matches_itself():
    for x in range(5):
        assert same_pair(x, x)


def check_https(port, want):
    assert is_https(port) == want, port


def test_https_port():
    check_https(443, True)
    check_https(80, False)


def test_default_port_smoke():
    is_default_port(8080)
