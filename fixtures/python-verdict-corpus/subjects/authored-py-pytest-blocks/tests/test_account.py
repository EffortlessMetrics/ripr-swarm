from ledger.account import Account, Wallet


def test_balance_starts_at_the_opening_amount():
    account = Account(100)
    account.balance()
    assert account._balance == 100


def test_an_overdrawn_wallet_reads_its_debt():
    assert Wallet(-50).amount() == -50
