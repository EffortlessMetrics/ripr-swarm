"""Dunder methods."""


class Money:
    def __init__(self, cents):
        self.cents = cents

    def __add__(self, other):
        return Money(self.cents + other.cents)

    def __eq__(self, other):
        return isinstance(other, Money) and self.cents == other.cents

    def __lt__(self, other):
        return self.cents < other.cents

    def __repr__(self):
        return f"Money({self.cents})"

    def __str__(self):
        return f"${self.cents / 100:.2f}"

    def __bool__(self):
        return self.cents != 0
