from src.subject import Eligible, Other


def test_check():
    assert Eligible().check(5) == True
    assert Other().check(3) == True
