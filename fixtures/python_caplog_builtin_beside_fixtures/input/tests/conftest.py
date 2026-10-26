import pytest


@pytest.fixture(params=['name="caplog"'])
def sample(request):
    return request.param


@pytest.fixture(name="recorder")
def make_recorder():
    return object()
