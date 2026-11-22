from src.lib import humanize


def test_just_now():
    assert humanize(1, 0) == "just now"
