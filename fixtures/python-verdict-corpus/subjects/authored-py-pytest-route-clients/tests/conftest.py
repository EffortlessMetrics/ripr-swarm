import pytest

from routeclients.mini import TestClient
from routeclients.orders import app


@pytest.fixture
def client():
    return TestClient(app)
