from src.quote import quote


def test_bulk_order_uses_flat_quote():
    assert quote(2000.0) == 0
