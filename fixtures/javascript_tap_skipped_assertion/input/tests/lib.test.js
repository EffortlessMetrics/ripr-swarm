const { test } = require('tap');
const { stringify } = require('../src/lib');
test('skipped assertion', t => {
  t.equal(stringify({ a: 1 }), '{"a":1}', { skip: true });
  t.end();
});
