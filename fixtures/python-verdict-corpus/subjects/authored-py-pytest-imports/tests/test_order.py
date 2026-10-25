from shop import rates
from shop.order import checkout, rush


def test_checkout_with_patched_rate(mocker):
    mocker.patch("shop.rates.tax", return_value=5)
    assert checkout(100) == 105


def test_rush_with_patched_fee(mocker):
    mocker.patch.object(rates, "fee", return_value=7)
    assert rush(100) == 107
