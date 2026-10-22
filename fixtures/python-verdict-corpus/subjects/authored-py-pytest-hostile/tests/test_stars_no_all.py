from hostile.stars_no_all import *  # noqa: F403


def test_public_star_total():
    assert public_star_total(2, 3) == 5


def test_private_star_name_is_not_imported():
    assert "_private_star_total" not in globals()
