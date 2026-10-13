"""Account balances and the rules that move money between them."""

MONTHLY_FEE_CENTS = 300
FEE_WAIVER_BALANCE_CENTS = 10000


class InsufficientFunds(Exception):
    """A withdrawal asked for more than the balance holds."""


class Account:
    def __init__(self, owner, opening_cents=0):
        self.owner = owner.strip()
        self.balance_cents = opening_cents

    def deposit(self, amount_cents):
        if amount_cents <= 0:
            raise ValueError("deposit must be positive")
        self.balance_cents += amount_cents

    def can_withdraw(self, amount_cents):
        return amount_cents <= self.balance_cents

    def withdraw(self, amount_cents):
        if amount_cents > self.balance_cents:
            raise InsufficientFunds(self.owner)
        self.balance_cents -= amount_cents


def monthly_fee(balance_cents):
    """Accounts holding the waiver balance pay no monthly fee."""
    if balance_cents >= FEE_WAIVER_BALANCE_CENTS:
        return 0
    return MONTHLY_FEE_CENTS


def transfer(source, target, amount_cents, notifier):
    """Move money and tell the receiving owner how much arrived."""
    source.withdraw(amount_cents)
    target.deposit(amount_cents)
    notifier.send(target.owner, amount_cents)


def statement_header(account):
    """First line of a printed statement."""
    return f"Statement for {account.owner}"
