import * as assert from 'assert';
import * as fs from 'fs';
import * as path from 'path';
import { integerSetting } from '../../src/workspaceHelpers';

suite('Integer-typed server settings', () => {
  test('the manifest declares the server integer keys as integer', () => {
    const manifest = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../../package.json'), 'utf8'));
    const properties = manifest.contributes.configuration.properties;
    for (const key of ['ripr.gitTimeoutMs', 'ripr.refreshDeadlineMs']) {
      assert.strictEqual(properties[key].type, 'integer', `${key} must be an integer setting`);
    }
  });

  test('only non-negative integers are forwarded to the server', () => {
    assert.strictEqual(integerSetting(2500), 2500);
    assert.strictEqual(integerSetting(2500.5), undefined);
    assert.strictEqual(integerSetting(-1), undefined);
    assert.strictEqual(integerSetting(Number.NaN), undefined);
    assert.strictEqual(integerSetting(undefined), undefined);
  });
});
