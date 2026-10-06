import * as fs from 'fs';
import * as path from 'path';
import Mocha from 'mocha';

export interface RunOutcome {
  executed: number;
  failures: number;
  failedBeforeHooks: string[];
}

/**
 * Classifies a finished mocha run and returns the rejection reason, or
 * undefined for a clean run. A suite whose "before all"/"before each" hook
 * died executed none of its tests, so that outcome is reported as a setup
 * death with the named hook instead of an ordinary test failure (#6845).
 */
export function validateRunOutcome({
  executed,
  failures,
  failedBeforeHooks,
}: RunOutcome): string | undefined {
  if (failedBeforeHooks.length > 0) {
    return [
      `${failedBeforeHooks.length} suite setup hook failed before its tests ran:`,
      ...failedBeforeHooks.map((title) => `- ${title}`),
      'The affected suite executed no tests; this is not an ordinary test failure.'
    ].join('\n');
  }
  if (executed === 0) {
    return 'test run executed zero tests; result is not_run';
  }
  if (failures > 0) {
    return `${failures} tests failed.`;
  }
  return undefined;
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
    const runner = mocha.run((failures: number) => {
      const executed = runner.stats?.tests ?? 0;
      const reason = validateRunOutcome({ executed, failures, failedBeforeHooks });
      if (reason !== undefined) {
        reject(new Error(reason));
        return;
      }
      resolve();
    });
    // mocha starts suites on a later tick (Runner.immediately(prepare)), so a
    // listener attached here observes every failure, including a before-all
    // hook death that precedes the first executed test.
    runner.on('fail', (runnable: { title?: string; type?: string }) => {
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
