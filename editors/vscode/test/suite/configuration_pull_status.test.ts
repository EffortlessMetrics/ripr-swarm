import * as assert from 'assert';
import { configurationPullFailureStatus } from '../../src/client';

suite('Configuration pull failure status (#9917)', () => {
  const failed = {
    input_authority: {
      configuration_pull: {
        state: 'failed',
        failure: {
          kind: 'config_pull_invalid',
          message: 'workspace/configuration value for `gitTimeoutMs` has an invalid type or literal: 2500.5'
        },
        recovery_route: 'retry_via_did_change_configuration'
      }
    }
  };

  test('a failed pull surfaces the server message and the recovery step', () => {
    const status = configurationPullFailureStatus(failed);
    assert.ok(status, 'failed pull produces a status');
    assert.strictEqual(status.kind, 'analysisFailed');
    assert.ok(status.detail?.includes('`gitTimeoutMs`'), 'names the offending key');
    assert.ok(status.nextStep?.includes('Restart Server'), 'names a recovery action');
  });

  test('applied, absent, or missing pull state produces no status', () => {
    assert.strictEqual(configurationPullFailureStatus({ input_authority: { configuration_pull: { state: 'applied' } } }), undefined);
    assert.strictEqual(configurationPullFailureStatus({ input_authority: null }), undefined);
    assert.strictEqual(configurationPullFailureStatus({}), undefined);
  });

  test('a failed pull without a message still reports a failure', () => {
    const status = configurationPullFailureStatus({ input_authority: { configuration_pull: { state: 'failed' } } });
    assert.ok(status?.detail?.includes('rejected the workspace configuration'));
  });
});
