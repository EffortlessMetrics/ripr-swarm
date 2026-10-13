use strict;
use warnings;
use Test::More;
use Test::Exception;
use Account;

throws_ok { Account::withdraw(10, 20) } qr/insufficient funds/, 'overdrawing names the reason';

throws_ok { Account::transfer(10, 11) } qr/exceeds balance/, 'one over the balance dies';
lives_ok { Account::transfer(10, 10) } 'the whole balance may move';

dies_ok { Account::close_out(1000) } 'large balances cannot close';

lives_ok { Account::interest(100) } 'interest never dies';

throws_ok { Account::freeze('fraud') } 'Account::Error', 'freezing raises an account error';

lives_and { is(Account::balance_after(10, 4), 5) } 'a withdrawal costs a fee of one';

done_testing();
