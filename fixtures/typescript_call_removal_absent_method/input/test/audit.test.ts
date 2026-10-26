import { mock, test } from 'node:test';
import assert from 'node:assert/strict';
import { emit } from '../src/audit.ts';
test('writes once', () => {
const sink = { other(event: string) { void event; } }; const write = mock.method(sink, 'write'); emit(sink, 'a'); assert.equal(write.mock.callCount(), 1);
});
