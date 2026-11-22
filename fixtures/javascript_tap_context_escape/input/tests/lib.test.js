const { test } = require('tap');
const { stringify } = require('../src/lib');
test('escaped receiver', t => {
  const replace = (_strings, context) => { context.equal = () => true; };
  replace`${t}`;
  t.equal(stringify({ a: 1 }), '{"a":1}');
  t.end();
});
test('escaped opts', t => {
  const replace = (_strings, context) => { context.equal = () => true; };
  replace`${t}`;
  t.equal(stringify({ a: 1 }, {}), '{\n  "a": 1\n}');
  t.end();
});
