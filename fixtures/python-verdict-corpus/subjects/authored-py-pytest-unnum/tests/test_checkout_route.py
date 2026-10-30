from unnum.checkout_route import checkout, response


def test_checkout_sets_unprocessable_status():
    checkout()
    assert response.status_code == 422
