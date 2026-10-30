from shapes.models import Invoice


def test_total_applies_default_tax():
    assert Invoice(100).total() == 120


def test_total_with_explicit_rate():
    assert Invoice(200, 0.5).total() == 300


def test_large_invoice_flag():
    assert Invoice(5000).is_large
    assert not Invoice(10).is_large


def test_free_invoice_costs_nothing():
    assert Invoice.free().total() == 0


def test_parse_amount_strips_whitespace():
    assert Invoice.parse_amount(" 42 ") == 42
