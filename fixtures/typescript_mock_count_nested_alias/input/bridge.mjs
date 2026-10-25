import { test as nodeTest } from "node:test";
import assert from "node:assert/strict";
globalThis.jest = { fn() { const fn = (...args) => { fn.mock.calls.push(args); }; fn.mock = { calls: [] }; return fn; } };
globalThis.expect = value => ({ toBe(expected) { assert.strictEqual(value, expected); } });
globalThis.it = (name, callback) => nodeTest(name, (_t, done) => callback(done));
await import("./test/notify.test.ts");
