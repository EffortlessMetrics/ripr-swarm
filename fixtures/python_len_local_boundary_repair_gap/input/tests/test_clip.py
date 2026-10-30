from src.clip import truncate_visible


def test_truncates_long_text():
    assert truncate_visible("abcdef", 3) == "abc"


def test_keeps_short_text():
    assert truncate_visible("ab", 5) == "ab"
