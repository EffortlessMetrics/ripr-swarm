import unittest
from unittest import mock

from accounts.ledger import (
    Account,
    InsufficientFunds,
    monthly_fee,
    transfer,
)


class TestDeposits(unittest.TestCase):
    def test_deposit_adds_to_balance(self):
        account = Account("ann", 100)
        account.deposit(50)
        self.assertEqual(account.balance_cents, 150)

    def test_owner_is_kept(self):
        account = Account("ann")
        self.assertEqual(account.owner, "ann")


class TestWithdrawals(unittest.TestCase):
    def test_small_withdrawal_is_allowed(self):
        account = Account("ann", 100)
        self.assertTrue(account.can_withdraw(50))

    def test_overdraft_is_refused(self):
        account = Account("ann", 100)
        with self.assertRaises(InsufficientFunds):
            account.withdraw(500)


class TestTransfers(unittest.TestCase):
    def test_receiver_is_notified_of_the_amount(self):
        notifier = mock.Mock()
        transfer(Account("ann", 100), Account("bob"), 25, notifier)
        notifier.send.assert_called_once_with("bob", 25)


class FeeChecks:
    def test_fee_waived_exactly_at_waiver_balance(self):
        self.assertEqual(monthly_fee(10000), 0)
        self.assertEqual(monthly_fee(9999), 300)


class TestFees(FeeChecks, unittest.TestCase):
    pass


if __name__ == "__main__":
    unittest.main()
