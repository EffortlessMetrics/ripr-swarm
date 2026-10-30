package Inventory;

use strict;
use warnings;

sub restock {
    my ($stock, $qty) = @_;
    my %next = %$stock;
    $next{count} = $stock->{count} + $qty;
    return \%next;
}

sub adjust {
    my ($stock, $delta) = @_;
    my %next = %$stock;
    $next{count} = $stock->{count} + $delta;
    return \%next;
}

sub tag_list {
    my (@tags) = @_;
    return [sort @tags];
}

sub sku_for {
    my ($n) = @_;
    return sprintf('A-%d', $n);
}

sub make_record {
    my ($id, $name) = @_;
    return { id => $id, name => lc $name };
}

sub label_pair {
    my ($sku, $count) = @_;
    return [$sku, $count];
}

1;
