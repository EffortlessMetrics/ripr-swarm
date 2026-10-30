from models import Invoice


def test_total():
    assert Invoice(100).total() == 120
