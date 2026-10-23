import pytest

from grid_returns.core import banner, full_name, greeting, initial, normalize, shout


def test_greeting_names_the_person():
    assert greeting("Ada") == "Hello, Ada!"


@pytest.mark.parametrize("name,want", [("alice", "a"), ("", ""), ("Bob", "B")])
def test_initial_table(name, want):
    assert initial(name) == want


def test_normalize_is_idempotent():
    for text in ("  a ", "b", ""):
        assert normalize(normalize(text)) == normalize(text)


def check_shout(text, want):
    assert shout(text) == want


def test_shout():
    check_shout("hey", "HEY")


def test_banner_smoke():
    banner("x")


def test_full_name_starts_with_the_first_name():
    assert full_name("Ada", "Lovelace").startswith("Ada")
