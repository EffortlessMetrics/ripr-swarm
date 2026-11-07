const { test } = require('tap');
const { stringify } = require('../src/lib');
const name = {name: 'computed', todo: true}; test(name, async t => { process.stderr.write('CALLBACK_EXECUTED\n'); t.equal(stringify({a: 1}), '{"a":1}'); });
