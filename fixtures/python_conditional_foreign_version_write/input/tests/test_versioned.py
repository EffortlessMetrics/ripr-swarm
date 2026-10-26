import app
from versioned import versioned

def test_it():
    assert versioned() == 'new'
