package PathParts;

use strict;
use warnings;

# Independently authored list-returning helper grounded in PathTools
# File::Spec::Unix::splitpath (issue #7851 names PathTools 3.86). The
# algorithm below matches the `sub splitpath` body at Perl/perl5
# f722f11a2b1a34e28743e331bad6dd7da41e2086
# dist/PathTools/lib/File/Spec/Unix.pm (sha256
# c91742b260863c8473f491fe6ebd511e56063538cc35bd0820a8abff4fd28082;
# that tree labels PathTools 3.91). Upstream license is perl_5; this
# authored copy is MIT OR Apache-2.0. The invocant is omitted because
# this is a function, not a File::Spec method.

sub splitpath {
    my ($path, $nofile) = @_;

    my ($volume, $directory, $file) = ('', '', '');

    if ($nofile) {
        $directory = $path;
    }
    else {
        $path =~ m|^ ( (?: .* / (?: \.\.?\z )? )? ) ([^/]*) |xs;
        $directory = $1;
        $file      = $2;
    }

    return ($volume, $directory, $file);
}

1;
