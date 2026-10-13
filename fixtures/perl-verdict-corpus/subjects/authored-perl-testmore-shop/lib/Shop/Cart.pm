package Shop::Cart;

use strict;
use warnings;

sub new {
    my ($class) = @_;
    return bless { items => [] }, $class;
}

sub add {
    my ($self, $price, $qty) = @_;
    push @{ $self->{items} }, { price => $price, qty => $qty };
    return $self;
}

sub total {
    my ($self) = @_;
    my $total = 0;
    $total += $_->{price} * $_->{qty} for @{ $self->{items} };
    return $total;
}

sub item_count {
    my ($self) = @_;
    return scalar @{ $self->{items} };
}

sub run {
    my ($self, $action) = @_;
    return $self->$action();
}

1;
