import * as assert from 'assert';
import { validateRunOutcome } from './index';

suite('run outcome guard', () => {
  test('a before-all setup death names the hook instead of an ordinary test failure', () => {
    const reason = validateRunOutcome({
      executed: 2,
      failures: 1,
      failedBeforeHooks: ['"before all" hook for "extension is present"']
    });
    assert.ok(reason?.includes('"before all" hook for "extension is present"'), reason);
    assert.ok(reason?.includes('executed no tests'), reason);
    assert.ok(!reason?.includes('tests failed.'), reason);
  });

  test('a before-all setup death with zero executed tests is not read as not_run', () => {
    const reason = validateRunOutcome({
      executed: 0,
      failures: 1,
      failedBeforeHooks: ['"before all" hook for "extension is present"']
    });
    assert.ok(reason?.includes('"before all" hook'), reason);
    assert.ok(!reason?.includes('not_run'), reason);
  });

  test('a zero-test run with no failures is not_run', () => {
    const reason = validateRunOutcome({ executed: 0, failures: 0, failedBeforeHooks: [] });
    assert.ok(reason?.includes('executed zero tests'), reason);
  });

  test('ordinary failures keep the ordinary message', () => {
    const reason = validateRunOutcome({ executed: 5, failures: 2, failedBeforeHooks: [] });
    assert.strictEqual(reason, '2 tests failed.');
  });

  test('a clean run validates', () => {
    assert.strictEqual(
      validateRunOutcome({ executed: 5, failures: 0, failedBeforeHooks: [] }),
      undefined
    );
  });
});
