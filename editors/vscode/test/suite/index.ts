import * as fs from 'fs';
import * as path from 'path';
import Mocha from 'mocha';

export interface RunOutcome {
  executed: number;
  failures: number;
  failedBeforeHooks: string[];
  /** Required tests (RIPR_TEST_REQUIRE_RAN) that did not pass in this run. */
  missingRequired?: string[];
}

/** Separator for RIPR_TEST_REQUIRE_RAN; test titles contain commas. */
export const REQUIRED_TITLE_SEPARATOR = '|';

export function parseRequiredTitles(value: string | undefined): string[] {
  return (value ?? '')
    .split(REQUIRED_TITLE_SEPARATOR)
    .map((title) => title.trim())
    .filter((title) => title.length > 0);
}

/**
 * Required tests that did not pass. A `this.skip()` test, a failed test and a
 * test that was never registered all land here: a lane that names a journey as
 * its proof must have actually run it, not merely run something else.
 */
export function missingRequiredTitles(required: string[], passed: Iterable<string>): string[] {
  const passedTitles = new Set(passed);
  return required.filter((title) => !passedTitles.has(title));
}

/**
 * Classifies a finished mocha run and returns the rejection reason, or
 * undefined for a clean run. A failed "before all"/"before each" hook is
 * named as a setup death with what did and did not execute, instead of
 * reading as an ordinary failure count (#6845).
 */
export function validateRunOutcome({
  executed,
  failures,
  failedBeforeHooks,
  missingRequired = [],
}: RunOutcome): string | undefined {
  if (failedBeforeHooks.length > 0) {
    const lines = [
      `${failedBeforeHooks.length} suite setup hook${failedBeforeHooks.length === 1 ? '' : 's'} failed before ${failedBeforeHooks.length === 1 ? 'its' : 'their'} remaining tests ran:`,
      ...failedBeforeHooks.map((title) => `- ${title}`)
    ];
    const otherFailures = Math.max(0, failures - failedBeforeHooks.length);
    if (otherFailures > 0) {
      lines.push(`${otherFailures} other test(s) failed in this run.`);
    }
    if (executed > 0) {
      lines.push(`${executed} test(s) did execute in this run, earlier or in other suites.`);
    }
    return lines.join('\n');
  }
  if (executed === 0) {
    return 'test run executed zero tests; result is not_run';
  }
  if (failures > 0) {
    return `${failures} tests failed.`;
  }
  if (missingRequired.length > 0) {
    return [
      `${missingRequired.length} required test${missingRequired.length === 1 ? '' : 's'} did not pass (skipped, absent or failed); result is not_run for:`,
      ...missingRequired.map((title) => `- ${title}`)
    ].join('\n');
  }
  return undefined;
}

/**
 * Tests that actually ran to a verdict. mocha's `stats.tests` also counts
 * skipped (`this.skip()`) tests, so a lane whose every test skips would
 * otherwise read as executed and pass; only passes and failures count.
 */
export function executedFromStats(
  stats?: { passes?: number; failures?: number },
  hookFailures = 0
): number {
  // mocha counts a failed hook in `stats.failures`; a hook is not a test.
  return Math.max(0, (stats?.passes ?? 0) + (stats?.failures ?? 0) - hookFailures);
}

function beforeHookTitle(runnable: { title?: string; type?: string }): string | undefined {
  if (
    runnable.type === 'hook' &&
    typeof runnable.title === 'string' &&
    /^"before (all|each)" hook/.test(runnable.title)
  ) {
    return runnable.title;
  }
  return undefined;
}

export function run(): Promise<void> {
  const mocha = new Mocha({
    ui: 'tdd',
    color: true,
    timeout: 10000,
  });
  const grep = process.env.RIPR_TEST_GREP;
  if (grep) {
    mocha.grep(grep);
  }

  const testsRoot = path.resolve(__dirname);
  const files = findTestFiles(testsRoot);
  files.forEach((f) => mocha.addFile(path.resolve(testsRoot, f)));

  return new Promise((resolve, reject) => {
    const failedBeforeHooks: string[] = [];
    const passedTitles: string[] = [];
    const requiredTitles = parseRequiredTitles(process.env.RIPR_TEST_REQUIRE_RAN);
    let hookFailures = 0;
    const runner = mocha.run((failures: number) => {
      const executed = executedFromStats(runner.stats, hookFailures);
      const missingRequired = missingRequiredTitles(requiredTitles, passedTitles);
      const reason = validateRunOutcome({ executed, failures, failedBeforeHooks, missingRequired });
      if (reason !== undefined) {
        reject(new Error(reason));
        return;
      }
      resolve();
    });
    // mocha starts suites on a later tick (Runner.immediately(prepare)), so a
    // listener attached here observes every failure, including a before-all
    // hook death that precedes the first executed test.
    runner.on('pass', (test: { fullTitle?: () => string }) => {
      if (typeof test.fullTitle === 'function') {
        passedTitles.push(test.fullTitle());
      }
    });
    runner.on('fail', (runnable: { title?: string; type?: string }) => {
      if (runnable.type === 'hook') {
        hookFailures += 1;
      }
      const title = beforeHookTitle(runnable);
      if (title !== undefined) {
        failedBeforeHooks.push(title);
      }
    });
  });
}

function findTestFiles(dir: string, relative = ''): string[] {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  const files: string[] = [];
  for (const entry of entries) {
    const name = entry.name;
    const rel = path.join(relative, name);
    if (entry.isDirectory()) {
      files.push(...findTestFiles(path.join(dir, name), rel));
    } else if (name.endsWith('.test.js')) {
      files.push(rel);
    }
  }
  return files;
}
