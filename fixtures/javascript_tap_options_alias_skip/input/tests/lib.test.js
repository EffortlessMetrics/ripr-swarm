const { test } = require('tap');
const { stringify } = require('../src/lib');
const plain = {name: 'plain', skip: true}; const opts = {name: 'opts', skip: true};
test(plain, async t => {
  process.stderr.write('CALLBACK_EXECUTED\n');
  t.equal(stringify({ a: 1 }), '{"a":1}');
});
test(opts, async t => {
  process.stderr.write('CALLBACK_EXECUTED\n');
  t.equal(stringify({ a: 1 }, {}), '{\n  "a": 1\n}');
});
