import assert from 'node:assert/strict'
import { test } from 'node:test'
import { nanoid } from '../non-secure/index.js'

await test('default id length', () => {
  assert.strictEqual(nanoid().length, 21)
})
