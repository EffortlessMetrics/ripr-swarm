"""A shop account, fees, reports and greetings, tested through pytest fixtures."""

import os
import sys


class Account:
    def __init__(self, balance):
        self.balance = balance

    def deposit(self, amount):
        self.balance += amount


def handling_fee(amount):
    """Orders of 100 or more pay no handling fee."""
    return 0 if amount >= 100 else 5


def save_report(path, total):
    """Write the order total to a report file."""
    path.write_text(f"total={total}\n")


def greeting():
    """Greet the user named in the environment."""
    return f"hello {os.environ.get('USER_NAME', 'guest')}"


def warn(message):
    """Print a warning to standard error."""
    print(f"warning: {message}", file=sys.stderr)
