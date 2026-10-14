use strict;
use warnings;
use Test2::V0;
use Checkout ();

ok(Checkout::total_cents(125, 3), 'has a total');
done_testing;
