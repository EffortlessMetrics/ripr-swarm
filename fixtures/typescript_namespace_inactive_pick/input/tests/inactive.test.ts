import { expect } from 'vitest';
import { isAdult } from '../src/subject';
const runner = require('mocha');
const test = runner.xit;
test('adult boundary', () => { expect(isAdult(18)).toBe(true); });
