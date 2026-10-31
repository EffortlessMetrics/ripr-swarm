import { importedConstant } from '../src/importedConstant';

import { edge } from './elsewhere';

test('import that does not resolve', () => {
    expect(importedConstant(edge)).toBe("gold");
});
