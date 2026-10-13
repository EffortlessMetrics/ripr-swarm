"""Accounts and wallets whose readings return the stored amount."""


class Account:
    def __init__(self, balance):
        self._balance = balance

    def balance(self):
        # the amount on record
        return self._balance


class Wallet:
    def __init__(self, held):
        self._held = held

    def amount(self):
        # what the wallet holds right now
        return self._held
