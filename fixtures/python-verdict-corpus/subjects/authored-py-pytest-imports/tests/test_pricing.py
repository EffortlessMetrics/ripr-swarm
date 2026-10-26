from shop.pricing import subtotal


def test_subtotal_adds_tax():
    assert subtotal(100, 5) == 105
