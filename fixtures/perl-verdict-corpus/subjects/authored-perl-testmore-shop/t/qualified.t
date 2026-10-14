use strict;
use warnings;
use Test::More;
use Shop::Pricing;

ok(Shop::Pricing::shipping_fee(25), 'heavy parcels pay shipping');

is(Shop::Pricing::tier_label(2000), 'gold', 'big spenders are gold');
is(Shop::Pricing::tier_label(10), 'bronze', 'small spenders are bronze');

cmp_ok(Shop::Pricing::loyalty_points(100), '>', 0, 'spending earns points');

is_deeply(
    Shop::Pricing::quote(100, 5),
    { subtotal => 90, shipping => 5, total => 95 },
    'quote adds shipping to the discounted subtotal',
);

like(Shop::Pricing::round_cents(2), qr/^\d+\.\d\d$/, 'money has two decimals');

isnt(Shop::Pricing::clamp_qty(150), 150, 'huge orders are clamped');

done_testing();
