use strict;
use warnings;
use Test::More;
use Profile;

my $p = Profile->new(first_name => 'Ada', last_name => 'Lovelace', age => 30);

is($p->full_name, 'Ada Lovelace', 'builder joins the names');

ok($p->is_adult, 'thirty is an adult');

my $kid = Profile->new(first_name => 'Tim', last_name => 'Small', age => 5);
ok(!$kid->is_adult, 'five is not an adult');

ok($p->birthday, 'birthday returns the new age');

done_testing;
