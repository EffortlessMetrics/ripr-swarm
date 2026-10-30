import app
from settings import over_limit

def test_it():
    assert over_limit(11)
