use strict;
use warnings;
use Test::More;
use Shop::Cart;

my $cart = Shop::Cart->new;
$cart->add(10, 2)->add(5, 1);
is($cart->total, 25, 'total multiplies price by quantity');
is($cart->run('item_count'), 2, 'run dispatches to the named action');

done_testing();
