import pytest


@pytest.fixture(params=[100, 200])
def quantity(request):
    return request.param * 2
