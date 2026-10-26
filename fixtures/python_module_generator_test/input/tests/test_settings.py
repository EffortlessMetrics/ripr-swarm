from settings import over_limit

def test_it():
    value = (yield 1)
    assert over_limit(11)
