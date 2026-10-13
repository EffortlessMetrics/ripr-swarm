package Account;

use strict;
use warnings;

package Account::Error;

sub new {
    my ($class, %args) = @_;
    return bless {%args}, $class;
}

sub message { $_[0]{message} }

package Account;

sub withdraw {
    my ($balance, $amount) = @_;
    die "insufficient funds: need $amount\n" if $amount > $balance;
    return $balance - $amount;
}

sub transfer {
    my ($from, $amount) = @_;
    if ($amount > $from) {
        die "transfer exceeds balance\n";
    }
    return $from - $amount;
}

sub close_out {
    my ($balance) = @_;
    die "balance must be zero\n" if $balance > 100;
    return 'closed';
}

sub interest {
    my ($balance) = @_;
    return $balance * 0.05;
}

sub freeze {
    my ($reason) = @_;
    die Account::Error->new(message => $reason) if defined $reason;
    return 1;
}

sub balance_after {
    my ($balance, $amount) = @_;
    return $balance - $amount - 1;
}

1;
