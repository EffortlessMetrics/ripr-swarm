class Account:
    def __init__(self, balance):
        self._balance = balance

    def balance(self):
        # the amount on record, never below zero
        return max(0, self._balance)
