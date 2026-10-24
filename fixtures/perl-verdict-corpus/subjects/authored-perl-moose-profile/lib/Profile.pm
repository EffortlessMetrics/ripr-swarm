package Profile;

use strict;
use warnings;
use Moose;

has first_name => (is => 'ro', isa => 'Str', required => 1);
has last_name  => (is => 'ro', isa => 'Str', required => 1);
has age        => (is => 'rw', isa => 'Int', default => 0);
has full_name  => (is => 'ro', isa => 'Str', lazy => 1, builder => '_build_full_name');

sub _build_full_name {
    my ($self) = @_;
    return join ' ', $self->first_name, $self->last_name;
}

sub is_adult {
    my ($self) = @_;
    return $self->age >= 18 ? 1 : 0;
}

sub birthday {
    my ($self) = @_;
    $self->age($self->age + 1);
    return $self->age;
}

__PACKAGE__->meta->make_immutable;

1;
