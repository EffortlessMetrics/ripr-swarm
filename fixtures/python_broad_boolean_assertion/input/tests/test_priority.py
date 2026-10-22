from src.priority import is_priority


def test_priority_amount():
    amount = 100
    assert is_priority(amount)
