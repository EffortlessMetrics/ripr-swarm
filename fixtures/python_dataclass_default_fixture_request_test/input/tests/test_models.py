from models import Invoice


def test_total(rate):
    assert Invoice(100).total() == 120
