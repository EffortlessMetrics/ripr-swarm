package Hostile;

use strict;
use warnings;
use utf8;
use constant RATE => 0.1;

=pod

Decoys the reader must skip:

sub tax { return $_[0] * RATE_DECOY }

=cut

# sub tax { return $_[0] * 9 }

our $AUTOLOAD;

sub new {
    my ($class) = @_;
    return bless { width => 4 }, $class;
}

sub tax {
    my ($amount) = @_;
    return $amount * RATE;
}

sub greet {
    my ($name) = @_;
    return <<"END_TPL";
sub fake_total { return 0 }
Hello, $name!
END_TPL
}

sub AUTOLOAD {
    my ($self) = @_;
    (my $field = $AUTOLOAD) =~ s/.*:://;
    return if $field eq 'DESTROY';
    die "no field $field" unless exists $self->{$field};
    return $self->{$field} * 2;
}

my %MULT = (double => 2, triple => 3);
for my $op (sort keys %MULT) {
    no strict 'refs';
    my $by = $MULT{$op};
    *{__PACKAGE__ . "::$op"} = sub { return $_[0] * $by };
}

eval "sub quad { \$_[0] * 4 }";

sub café_price {
    my ($montant) = @_;
    return $montant + 5;
}

# line 500 "elsewhere.pm"
sub pad {
    my ($s) = @_;
    return sprintf('%-6s|', $s);
}

1;

__END__

sub never_called { return 1 }
