from src.checker import validate


def test_checker_validate():
    assert validate("ok") is True
