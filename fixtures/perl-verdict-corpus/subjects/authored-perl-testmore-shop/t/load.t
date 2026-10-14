use strict;
use warnings;
use Test::More;

use_ok('Shop::Pricing');
can_ok('Shop::Pricing', 'gift_wrap_fee');

done_testing();
