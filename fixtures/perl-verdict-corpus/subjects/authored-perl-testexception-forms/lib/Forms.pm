package Forms::Error;

use strict;
use warnings;

sub new {
    my ($class, %args) = @_;
    return bless {%args}, $class;
}

sub message { $_[0]{message} }

package Forms;

use strict;
use warnings;

sub prefix_only {
    my ($n) = @_;
    die "insufficient: need $n\n" if $n > 5;
    return $n;
}

sub class_only {
    my ($n) = @_;
    die Forms::Error->new(message => "bad input: $n") if $n < 0;
    return $n;
}

sub class_then_message {
    my ($n) = @_;
    die Forms::Error->new(message => "bad range: $n") if $n > 100;
    return $n;
}

sub eval_like {
    my ($n) = @_;
    die "zero not allowed\n" if $n == 0;
    return 10 / $n;
}

sub lives_weak {
    my ($n) = @_;
    return $n * 3;
}

sub eval_ok {
    my ($n) = @_;
    return $n + 1;
}

1;
