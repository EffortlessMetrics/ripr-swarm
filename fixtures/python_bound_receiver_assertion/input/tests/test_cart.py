from src.cart import Cart, Other


def test_total_on_the_owner_receiver():
    cart = Cart()
    other = Other()
    assert cart.total(10) == 30
    assert other.total(10) == 30
