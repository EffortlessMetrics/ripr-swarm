from src.account import Account


def test_balance_starts_at_the_opening_amount():
    account = Account(100)
    account.balance()
    assert account._balance == 100
