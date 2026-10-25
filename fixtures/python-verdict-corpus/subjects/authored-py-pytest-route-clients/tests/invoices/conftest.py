import pytest

from routeclients.invoices import app
from routeclients.mini import TestClient


@pytest.fixture
def invoice_app():
    return app


@pytest.fixture
def client(invoice_app):
    return TestClient(invoice_app)
