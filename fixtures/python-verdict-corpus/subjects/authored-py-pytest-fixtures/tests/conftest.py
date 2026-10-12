import pytest

from shop import Account


@pytest.fixture
def account():
    return Account(100)


@pytest.fixture(params=[(99, 5), (100, 0)], ids=["below", "at"])
def fee_case(request):
    return request.param
