use strict;
use warnings;
use Test::More;
use Test::Deep;
use Inventory;

my $stock = { sku => 'A1', count => 5 };

cmp_deeply(Inventory::restock($stock, 2), superhashof({ sku => 'A1' }), 'restock keeps the sku');

cmp_deeply(Inventory::adjust($stock, -3), { sku => 'A1', count => 2 }, 'adjust changes the count exactly');

cmp_deeply(Inventory::tag_list(qw(b a c)), bag(qw(a b c)), 'tags are the same set');

cmp_deeply(Inventory::sku_for(7), re(qr/^A-\d+$/), 'sku has the A- prefix and a number');

cmp_deeply(Inventory::make_record(41, 'WIDGET'), { id => ignore(), name => 'widget' }, 'record name is lower-cased');

cmp_deeply(Inventory::label_pair('A1', 3), [ 'A1', code(sub { $_[0] > 0 }) ], 'pair has the sku and a positive count');

done_testing;
