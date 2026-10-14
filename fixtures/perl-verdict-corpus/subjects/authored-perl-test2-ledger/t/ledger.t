use strict;
use warnings;
use Test2::V0;
use Ledger qw(net_change);

is(Ledger::summary(10, 20), { total => 30, count => 2 }, 'summary totals and counts');

like(Ledger::format_entry('2026-10-04', 5), qr/^2026/, 'entries start with the date');

ok(Ledger::is_overdrawn(-1), 'negative balances are overdrawn');

like(dies { Ledger::withdraw(10, 20) }, qr/insufficient funds/, 'overdrawing names the reason');

ok(lives { Ledger::deposit(5, 5) }, 'deposits never die');

isnt(Ledger::fee(100), 0, 'fees are charged');

is(net_change(5, -3), 2, 'net change sums signed amounts');

subtest 'month close' => sub {
    is(Ledger::close_month(5, -5), 'balanced', 'zero is balanced');
    is(Ledger::close_month(5, -6), 'short', 'negative is short');
};

done_testing;
