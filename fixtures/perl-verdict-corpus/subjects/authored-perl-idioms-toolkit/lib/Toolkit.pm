package Toolkit;

use strict;
use warnings;

sub make_counter {
    my ($start) = @_;
    my $n = $start;
    return sub { return $n++ };
}

sub ctx_value {
    my @r = (1, 2, 3);
    return wantarray ? @r : scalar(@r);
}

sub ctx_count {
    my @r = (4, 5, 6);
    return wantarray ? @r : @r + 0;
}

sub join_local {
    my (@items) = @_;
    local $" = '-';
    return "@items";
}

sub parse_version {
    my ($s) = @_;
    return $s =~ /^v?(\d+)\.(\d+)$/ ? [$1, $2] : undef;
}

sub safe_div {
    my ($x, $y) = @_;
    my $r = eval { $x / $y };
    return defined $r ? $r : 'NaN';
}

sub slugify {
    my ($s) = @_;
    $s = lc $s;
    $s =~ s/[^a-z0-9]+/-/g;
    return $s;
}

sub banner {
    my ($width) = @_;
    return '=' x $width;
}

1;
