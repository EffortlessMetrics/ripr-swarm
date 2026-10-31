import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { runTimingInstrument } from '../probeTiming';

// #9631: the timing instrument must never turn an unsuccessful target into a
// valid timing row. POSIX shell fakes stand in for ripr; the Windows wrapper
// itself is exercised by the hang-hunt run.
suite('Probe timing instrument', () => {
  let dir: string;

  function fake(name: string, body: string): string {
    const file = path.join(dir, name);
    fs.writeFileSync(file, `#!/bin/sh\n${body}\n`, { mode: 0o755 });
    return file;
  }

  setup(function () {
    if (process.platform === 'win32') {
      this.skip();
    }
    dir = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr-probe-timing-'));
  });

  teardown(() => {
    if (dir) fs.rmSync(dir, { recursive: true, force: true });
  });

  test('a target that prints its version and exits 0 gives valid labelled rows', async () => {
    const report = await runTimingInstrument({ server: fake('ok', 'echo "ripr 0.11.0 (abc)"'), rounds: 2 });
    assert.strictEqual(report.rows.length, 4);
    assert.ok(report.rows.every((row) => row.valid && row.exit_code === 0 && row.output_ok));
    assert.deepStrictEqual(report.rows.map((row) => row.first_in_series), [true, true, false, false]);
    assert.strictEqual(report.summary.ripr_version?.valid, 2);
    assert.ok(report.identities.target?.sha256);
  });

  test('a nonzero exit is invalid even when the version text is printed', async () => {
    const report = await runTimingInstrument({ server: fake('fail', 'echo "ripr 0.11.0 (abc)"; exit 3'), rounds: 1 });
    assert.ok(report.rows.every((row) => !row.valid && row.invalid_reason === 'nonzero_exit'));
    assert.strictEqual(report.summary.ripr_version?.valid, 0);
    assert.strictEqual(report.summary.ripr_version?.median_ms, null);
  });

  test('exit 0 with the wrong output is invalid', async () => {
    const report = await runTimingInstrument({ server: fake('garbage', 'echo garbage'), rounds: 1 });
    assert.ok(report.rows.every((row) => !row.valid && row.invalid_reason === 'output_mismatch'));
  });

  test('a hung target is cut off at the deadline with a recorded cleanup outcome', async () => {
    const report = await runTimingInstrument({
      server: fake('hang', 'exec sleep 30'),
      rounds: 1,
      deadlineMs: 400,
      closeGraceMs: 1000
    });
    for (const row of report.rows) {
      assert.strictEqual(row.valid, false);
      assert.strictEqual(row.invalid_reason, 'deadline');
      assert.strictEqual(row.deadline_hit, true);
      assert.notStrictEqual(row.cleanup, 'not_needed');
      assert.ok(row.ms < 5000, `deadline row took ${row.ms} ms`);
    }
  });

  test('a missing target is a spawn error, not a timing row', async () => {
    const report = await runTimingInstrument({ server: path.join(dir, 'absent'), rounds: 1 });
    assert.ok(report.rows.every((row) => !row.valid && row.spawn_error !== null));
    assert.ok(report.identities.target?.error);
  });
});
