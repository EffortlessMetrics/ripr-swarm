import pytest

from routeclients.mini import TestClient
from routeclients.stock import app


@pytest.fixture
def api_client():
    return TestClient(app)
