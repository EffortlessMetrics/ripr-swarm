use strict;
use warnings;
use Test::More;
use Test::Exception;
use Forms;

throws_ok { Forms::prefix_only(9) } qr/^insufficient/, 'the prefix names the failure';

throws_ok { Forms::class_only(-1) } 'Forms::Error', 'a negative input raises the error class';

throws_ok { Forms::class_then_message(101) } 'Forms::Error', 'an out of range input raises the error class';
is($@->message, 'bad range: 101', 'the message names the input');

eval { Forms::eval_like(0) };
like($@, qr/zero not allowed/, 'zero is refused');

lives_and { ok(Forms::lives_weak(2) > 0) } 'tripling stays positive';

eval { Forms::eval_ok(2) };
ok(!$@, 'adding one does not die');

done_testing();
