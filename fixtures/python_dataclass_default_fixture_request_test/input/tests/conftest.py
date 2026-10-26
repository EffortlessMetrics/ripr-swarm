import pytest
import models


def _swap(target, name, value):
    setattr(target, name, value)


@pytest.fixture
def rate():
    _swap(models.Invoice, "total", lambda self: 120)
    return 0.25
