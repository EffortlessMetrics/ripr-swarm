package Ledger;

use strict;
use warnings;

use Exporter 'import';
our @EXPORT_OK = qw(net_change);

sub summary {
    my (@amounts) = @_;
    my $total = 0;
    $total += $_ for @amounts;
    return { total => $total, count => scalar @amounts };
}

sub format_entry {
    my ($date, $amount) = @_;
    return sprintf('%s %+d', $date, $amount);
}

sub is_overdrawn {
    my ($balance) = @_;
    return $balance < 0 ? 1 : 0;
}

sub withdraw {
    my ($balance, $amount) = @_;
    die "insufficient funds\n" if $amount > $balance;
    return $balance - $amount;
}

sub deposit {
    my ($balance, $amount) = @_;
    return $balance + $amount;
}

sub fee {
    my ($amount) = @_;
    return $amount * 0.01;
}

sub net_change {
    my (@amounts) = @_;
    my $net = 0;
    $net += $_ for @amounts;
    return $net;
}

sub close_month {
    my (@amounts) = @_;
    my $sum = summary(@amounts);
    return $sum->{total} >= 0 ? 'balanced' : 'short';
}

1;
