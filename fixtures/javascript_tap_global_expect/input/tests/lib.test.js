const { test } = require('tap');
const { stringify } = require('../src/lib');
test('plain', t => { expect(stringify({ a: 1 })).toBe('{"a":1}'); t.end(); });
test('opts', t => { expect(stringify({ a: 1 }, {})).toBe('{\n  "a": 1\n}'); t.end(); });
