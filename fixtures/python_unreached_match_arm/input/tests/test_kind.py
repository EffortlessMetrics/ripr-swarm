from src.kind import kind


def test_kind_of_known_values():
    assert kind(0) == "zero"
    assert kind(5) == "int"
