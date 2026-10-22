from src.kind import kind


def test_kind_of_unknown_value():
    assert kind("x") == "misc"
