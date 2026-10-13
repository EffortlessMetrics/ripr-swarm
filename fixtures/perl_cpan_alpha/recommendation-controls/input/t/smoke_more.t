use strict;
use warnings;
use Test::More;
use Checkout ();

ok(Checkout::total_cents(125, 3), 'has a total');
done_testing;
