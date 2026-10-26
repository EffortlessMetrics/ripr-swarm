import * as assert from 'assert';
import {
  REQUIRED_TITLE_SEPARATOR,
  executedFromStats,
  missingRequiredTitles,
  parseRequiredTitles,
  validateRunOutcome,
} from './index';

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

  test('multiple setup deaths pluralize the hook count', () => {
    const reason = validateRunOutcome({
      executed: 0,
      failures: 2,
      failedBeforeHooks: [
        '"before all" hook for "extension is present"',
        '"before all" hook for "settings roundtrip"'
      ]
    });
    assert.ok(reason?.includes('2 suite setup hooks failed before their remaining tests ran'), reason);
  });

  test('a zero-test run with no failures is not_run', () => {
    const reason = validateRunOutcome({ executed: 0, failures: 0, failedBeforeHooks: [] });
    assert.ok(reason?.includes('executed zero tests'), reason);
  });

  test('ordinary failures keep the ordinary message', () => {
    const reason = validateRunOutcome({ executed: 5, failures: 2, failedBeforeHooks: [] });
    assert.strictEqual(reason, '2 tests failed.');
  });

  test('skipped tests do not count as executed', () => {
    assert.strictEqual(executedFromStats({ passes: 0, failures: 0 }), 0);
    assert.strictEqual(executedFromStats({ passes: 3, failures: 1 }), 4);
    assert.strictEqual(executedFromStats(undefined), 0);
    // A failed hook is in mocha's failure count but is not an executed test.
    assert.strictEqual(executedFromStats({ passes: 0, failures: 1 }, 1), 0);
    assert.strictEqual(executedFromStats({ passes: 2, failures: 2 }, 1), 3);
    const reason = validateRunOutcome({
      executed: executedFromStats({ passes: 0, failures: 0 }),
      failures: 0,
      failedBeforeHooks: []
    });
    assert.ok(reason?.includes('executed zero tests'), reason);
  });

  test('a clean run validates', () => {
    assert.strictEqual(
      validateRunOutcome({ executed: 5, failures: 0, failedBeforeHooks: [] }),
      undefined
    );
  });
});

suite('required journey guard', () => {
  const journey = 'Extension Smoke real server surfaces seam diagnostic, hover provider, and agent actions';

  test('titles containing commas survive the env round trip', () => {
    const value = ['a, b', journey].join(REQUIRED_TITLE_SEPARATOR);
    assert.deepStrictEqual(parseRequiredTitles(value), ['a, b', journey]);
    assert.deepStrictEqual(parseRequiredTitles(undefined), []);
    assert.deepStrictEqual(parseRequiredTitles(' | '), []);
  });

  test('a required test that was skipped or never registered is missing', () => {
    assert.deepStrictEqual(missingRequiredTitles([journey], ['some other test']), [journey]);
    assert.deepStrictEqual(missingRequiredTitles([journey], []), [journey]);
  });

  test('a required test that passed is not missing', () => {
    assert.deepStrictEqual(missingRequiredTitles([journey], [journey, 'other']), []);
  });

  test('a lane whose smoke tests pass but whose required journey skipped is not_run', () => {
    const reason = validateRunOutcome({
      executed: 40,
      failures: 0,
      failedBeforeHooks: [],
      missingRequired: [journey],
    });
    assert.ok(reason?.includes('not_run'), reason);
    assert.ok(reason?.includes(journey), reason);
  });

  test('ordinary failures are reported before missing journeys', () => {
    const reason = validateRunOutcome({
      executed: 40,
      failures: 2,
      failedBeforeHooks: [],
      missingRequired: [journey],
    });
    assert.strictEqual(reason, '2 tests failed.');
  });

  test('no required journeys leaves a clean run clean', () => {
    assert.strictEqual(
      validateRunOutcome({ executed: 3, failures: 0, failedBeforeHooks: [], missingRequired: [] }),
      undefined
    );
  });
});
