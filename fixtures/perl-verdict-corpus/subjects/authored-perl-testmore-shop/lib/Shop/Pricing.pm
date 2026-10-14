package Shop::Pricing;

use strict;
use warnings;

use Exporter 'import';
our @EXPORT_OK = qw(
    discount shipping_fee tier_label loyalty_points quote
    round_cents is_bulk tax_for bundle_price clamp_qty
);

sub discount {
    my ($amount) = @_;
    if ($amount >= 100) {
        return $amount * 0.9;
    }
    return $amount;
}

sub shipping_fee {
    my ($weight) = @_;
    return 0 if $weight <= 0;
    if ($weight > 20) {
        return 15;
    }
    return 5;
}

sub tier_label {
    my ($points) = @_;
    return 'gold' if $points >= 1000;
    return 'silver' if $points >= 500;
    return 'bronze';
}

sub loyalty_points {
    my ($spent) = @_;
    my $points = int($spent / 10);
    return $points;
}

sub quote {
    my ($amount, $weight) = @_;
    my %quote = (
        subtotal => discount($amount),
        shipping => shipping_fee($weight),
    );
    $quote{total} = $quote{subtotal} + $quote{shipping};
    return \%quote;
}

sub round_cents {
    my ($value) = @_;
    return sprintf('%.2f', $value);
}

sub is_bulk {
    my ($qty) = @_;
    return $qty >= 50 ? 1 : 0;
}

sub tax_for {
    my ($region, $amount) = @_;
    my $rate = $region eq 'north' ? 0.2 : 0.1;
    return $amount * $rate;
}

sub bundle_price {
    my ($items) = @_;
    my $sum = 0;
    $sum += $_ for @$items;
    return $sum - (@$items >= 3 ? 5 : 0);
}

sub clamp_qty {
    my ($qty) = @_;
    return 1 if $qty < 1;
    return 99 if $qty > 99;
    return $qty;
}

sub gift_wrap_fee {
    my ($items) = @_;
    return $items * 3;
}

1;
