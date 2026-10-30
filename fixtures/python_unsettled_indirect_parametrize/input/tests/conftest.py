import pytest


@pytest.fixture
def quantity(request):
    return request.param * 2
