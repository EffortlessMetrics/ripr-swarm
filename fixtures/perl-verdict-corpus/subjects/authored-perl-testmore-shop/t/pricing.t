use strict;
use warnings;
use Test::More;
use Shop::Pricing qw(discount);

is(discount(100), 90, 'discount starts at exactly 100');
is(discount(99), 99, 'no discount just below the threshold');

done_testing();
