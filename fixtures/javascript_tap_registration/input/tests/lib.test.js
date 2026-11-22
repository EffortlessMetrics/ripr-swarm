const { test } = require('tap');
const { stringify } = require('../src/lib');
test('plain', function (t) {
  t.equal(stringify({ a: 1 }), '{"a":1}');
  t.end();
});
test('opts', function (t) {
  t.equal(stringify({ a: 1 }, {}), '{\n  "a": 1\n}');
  t.end();
});
