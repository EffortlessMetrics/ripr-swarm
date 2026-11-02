from src.steps import countdown

def test_steps():
    assert list(countdown(-2)) == []
