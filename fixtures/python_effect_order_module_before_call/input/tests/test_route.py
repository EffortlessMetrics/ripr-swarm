from shop.route import checkout, response


def test_checkout():
    assert response.status_code == 422; checkout()
