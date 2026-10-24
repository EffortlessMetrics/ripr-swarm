use strict;
use warnings;
use Test::More;
use Toolkit;

my $c = Toolkit::make_counter(5);
is($c->(), 5, 'counter starts at its start');
is($c->(), 6, 'counter advances');

is(scalar Toolkit::ctx_value(), 3, 'scalar context counts');
is_deeply([Toolkit::ctx_value()], [1, 2, 3], 'list context lists');

ok(scalar Toolkit::ctx_count(), 'scalar context is true');

is(Toolkit::join_local(1, 2), '1-2', 'local separator joins');

is_deeply(Toolkit::parse_version('v1.2'), [1, 2], 'parses a version');
is(Toolkit::parse_version('nonsense'), undef, 'rejects junk');

is(Toolkit::safe_div(4, 2), 2, 'divides');
like(Toolkit::safe_div(1, 0), qr/N/, 'division by zero is not a number');

is(Toolkit::slugify('Hello, World'), 'hello-world', 'slugifies');

ok(length(Toolkit::banner(10)), 'banner has content');

done_testing;
