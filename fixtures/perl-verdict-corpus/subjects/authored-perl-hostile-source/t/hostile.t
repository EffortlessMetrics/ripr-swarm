use strict;
use warnings;
use utf8;
use Test::More;
use Hostile;

binmode(Test::More->builder->output, ':utf8');

is(Hostile::tax(100), 10, 'tax at the flat rate');

like(Hostile::greet('Ann'), qr/Ann/, 'greeting names the person');

my $o = Hostile->new;
is($o->width, 8, 'autoloaded field is doubled');

is(Hostile::double(4), 8, 'double doubles');

is(Hostile::quad(2), 8, 'string-eval sub quadruples');

is(Hostile::café_price(1), 6, 'non-ASCII sub name works');

is(Hostile::pad('ab'), 'ab    |', 'pad fills to six columns');

done_testing;
