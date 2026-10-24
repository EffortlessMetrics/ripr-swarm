use strict;
use warnings;
use Test::More;
use Counter;

my $c = Counter->new;
is($c->inc(2), 2, 'first increment');
is($c->inc(3), 5, 'second increment accumulates');
is($c->value, 5, 'value reads the total');

my $s = Counter->new(start => 10);
is($s->value, 10, 'start is honoured');

done_testing;
