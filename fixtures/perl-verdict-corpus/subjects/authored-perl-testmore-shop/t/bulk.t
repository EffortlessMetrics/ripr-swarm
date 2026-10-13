use strict;
use warnings;
use Test::More;
use Shop::Pricing qw(is_bulk tax_for bundle_price);

subtest 'bulk threshold' => sub {
    is(is_bulk(50), 1, 'fifty is bulk');
    is(is_bulk(49), 0, 'forty-nine is not');
};

my @tax_cases = (
    { in => ['north', 100], want => 20 },
    { in => ['south', 100], want => 10 },
);
for my $case (@tax_cases) {
    is(tax_for(@{ $case->{in} }), $case->{want}, "tax for $case->{in}[0]");
}

sub check_bundle {
    my ($items, $want, $name) = @_;
    is(bundle_price($items), $want, $name);
}

check_bundle([10, 10, 10], 25, 'three items earn the bundle discount');
check_bundle([10, 10], 20, 'two items do not');

done_testing();
