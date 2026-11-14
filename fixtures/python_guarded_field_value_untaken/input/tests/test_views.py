from orders.views import order_view


def test_view_ok():
    assert order_view(None, 100)["status"] == 200
