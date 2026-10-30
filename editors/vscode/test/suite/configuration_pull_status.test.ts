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
    assert.ok(!/default/i.test(`${status.summary} ${status.detail}`), 'retained last-known-good config is not described as defaults');
  });

  test('a transport failure does not diagnose an invalid setting', () => {
    const status = configurationPullFailureStatus({
      input_authority: {
        configuration_pull: {
          state: 'failed',
          failure: { kind: 'config_pull_failed', message: 'workspace/configuration request failed: Internal error: review peer could not retrieve settings' },
          recovery_route: 'retry_via_did_change_configuration'
        }
      }
    });
    assert.ok(status, 'failed pull produces a status');
    assert.strictEqual(status.kind, 'analysisFailed');
    assert.ok(status.detail?.includes('could not retrieve settings'), 'keeps the server message');
    assert.ok(!/invalid|Correct the setting/i.test(`${status.summary} ${status.nextStep}`), 'makes no bad-setting diagnosis');
    assert.ok(status.nextStep?.includes('Restart Server'), 'names a recovery action');
  });

  test('an unclassified failure is not called invalid either', () => {
    const status = configurationPullFailureStatus({ input_authority: { configuration_pull: { state: 'failed' } } });
    assert.ok(status);
    assert.ok(!/invalid|Correct the setting/i.test(`${status.summary} ${status.nextStep}`));
  });

  test('applied, absent, or missing pull state produces no status', () => {
    assert.strictEqual(configurationPullFailureStatus({ input_authority: { configuration_pull: { state: 'applied' } } }), undefined);
    assert.strictEqual(configurationPullFailureStatus({ input_authority: null }), undefined);
    assert.strictEqual(configurationPullFailureStatus({}), undefined);
  });

  test('a failed pull without a message still reports a failure', () => {
    const status = configurationPullFailureStatus({ input_authority: { configuration_pull: { state: 'failed' } } });
    assert.ok(status?.detail?.includes('could not apply the workspace configuration'));
  });
});
