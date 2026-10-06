import * as assert from 'assert';
import { validateRunOutcome } from './index';

suite('run outcome guard', () => {
  test('a before-all setup death names the hook and what still executed', () => {
    const reason = validateRunOutcome({
      executed: 2,
      failures: 1,
      failedBeforeHooks: ['"before all" hook for "extension is present"']
    });
    assert.ok(reason?.includes('"before all" hook for "extension is present"'), reason);
    assert.ok(reason?.includes('2 test(s) did execute in this run'), reason);
    assert.ok(!reason?.includes('tests failed.'), reason);
  });

  test('a before-each setup death does not claim the suite ran no tests', () => {
    const reason = validateRunOutcome({
      executed: 3,
      failures: 1,
      failedBeforeHooks: ['"before each" hook for "later test"']
    });
    assert.ok(reason?.includes('"before each" hook for "later test"'), reason);
    assert.ok(reason?.includes('3 test(s) did execute in this run'), reason);
    assert.ok(!reason?.includes('executed no tests'), reason);
  });

  test('a before-all setup death with zero executed tests is not read as not_run', () => {
    const reason = validateRunOutcome({
      executed: 0,
      failures: 1,
      failedBeforeHooks: ['"before all" hook for "extension is present"']
    });
    assert.ok(reason?.includes('"before all" hook'), reason);
    assert.ok(!reason?.includes('not_run'), reason);
    assert.ok(!reason?.includes('test(s) did execute'), reason);
  });

  test('other ordinary failures are counted beside a setup death', () => {
    const reason = validateRunOutcome({
      executed: 4,
      failures: 3,
      failedBeforeHooks: ['"before all" hook for "extension is present"']
    });
    assert.ok(reason?.includes('2 other test(s) failed in this run'), reason);
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
