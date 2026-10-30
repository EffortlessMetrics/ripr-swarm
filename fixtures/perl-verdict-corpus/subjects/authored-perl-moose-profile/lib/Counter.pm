package Counter;

use strict;
use warnings;

sub new {
    my ($class, %args) = @_;
    my $self = bless { n => $args{start} // 0 }, $class;
    return $self;
}

sub inc {
    my ($self, $by) = @_;
    $self->{n} += $by;
    return $self->{n};
}

sub value {
    my ($self) = @_;
    return $self->{n};
}

1;
