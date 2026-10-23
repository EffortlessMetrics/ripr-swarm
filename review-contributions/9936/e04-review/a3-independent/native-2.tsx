export {};
declare function require(path: string): any;
declare function Foo(props: any): any;
declare global { namespace JSX { type Element=any; interface IntrinsicElements { p: any; } } }
var test = require('tape');
const { triple } = require('../src/k.js');

test('triples', function (t) {
  t.equal(triple(2), 6);
  t.end();
});
const n = 1, s = ''; const a = <Foo a={1} />; const q = n < /a`/.source.length; test = null;