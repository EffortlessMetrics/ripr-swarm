use strict;
use warnings;
use Test::More;
use Checkout ();

subtest 'line item total' => sub {
    is(Checkout::total_cents(125, 3), 375, 'price times quantity');
    is(Checkout::total_cents(125, 0), 0, 'empty quantity');
};
done_testing;
