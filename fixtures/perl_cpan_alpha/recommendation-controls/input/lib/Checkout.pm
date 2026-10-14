package Checkout;
use strict;
use warnings;

sub total_cents {
    my ($price_cents, $quantity) = @_;
    return $price_cents * $quantity;
}

1;
