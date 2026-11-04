use strict;
use warnings;
use Test::More;
use PathParts;

# Test vectors follow Perl/perl5 f722f11a2b1a34e28743e331bad6dd7da41e2086
# dist/PathTools/t/Spec.t Unix->splitpath rows (sha256
# fdd7a3e3de5974eb08fd4c45328d7c8ae38338de1dabad29c89a7b6620da9d35):
#   Unix->splitpath('d1/d2/d3/file') => ('', 'd1/d2/d3/', 'file')
#   Unix->splitpath('file')          => ('', '', 'file')

is_deeply(
    [PathParts::splitpath('d1/d2/d3/file')],
    ['', 'd1/d2/d3/', 'file'],
    'list splits volume, directory, and file'
);

is(
    scalar PathParts::splitpath('d1/d2/d3/file'),
    'file',
    'scalar context is the basename'
);

is_deeply(
    [PathParts::splitpath('d1/d2/d3/file')],
    [PathParts::splitpath('d1/d2/d3/file')],
    'list self-comparison'
);

is_deeply(
    [PathParts::splitpath('file')],
    ['', '', 'file'],
    'a bare file has an empty directory'
);

done_testing;
