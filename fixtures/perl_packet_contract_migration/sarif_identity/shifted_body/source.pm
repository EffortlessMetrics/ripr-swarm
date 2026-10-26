package App::Discount;

sub discount {
# movement

    my ($self, $amount) = @_;
    return 5 if $amount < 25;

    return 7 if $amount > 100;
    return 10;
}

1;
