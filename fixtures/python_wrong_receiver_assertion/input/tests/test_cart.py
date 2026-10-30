from src.cart import Cart, Other


def test_total_on_another_receiver():
    cart = Cart()
    cart.total(10)
    other = Other()
    assert other.total(10) == 30
