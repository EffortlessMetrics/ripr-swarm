import * as assert from 'assert';
import * as cp from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { ArmName, instrumentFailure, runTimingInstrument } from '../probeTiming';

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

  test('the failure gate trips when any measured arm has no valid trial, not only the direct target', () => {
    const arm = (valid: number) => ({
      valid,
      invalid: 5 - valid,
      min_ms: null,
      median_ms: null,
      max_ms: null,
      first_round_valid_ms: [],
      later_rounds_median_ms: null
    });
    const posix: ArmName[] = ['ripr_version', 'extension_probe'];
    const win: ArmName[] = ['ripr_version', 'powershell_empty', 'extension_probe'];
    assert.strictEqual(instrumentFailure({ ripr_version: arm(5), extension_probe: arm(5) }, posix), null);
    assert.strictEqual(
      instrumentFailure({ ripr_version: arm(5), extension_probe: arm(0) }, posix),
      'no valid trial for: extension_probe'
    );
    assert.strictEqual(
      instrumentFailure({ ripr_version: arm(0), powershell_empty: arm(0), extension_probe: arm(5) }, win),
      'no valid trial for: ripr_version, powershell_empty'
    );
  });

  test('an arm missing from the summary fails the gate', () => {
    const arm = { valid: 5, invalid: 0, min_ms: 1, median_ms: 1, max_ms: 1, first_round_valid_ms: [1], later_rounds_median_ms: 1 };
    assert.strictEqual(
      instrumentFailure({ ripr_version: arm }, ['ripr_version', 'extension_probe']),
      'no valid trial for: extension_probe'
    );
    assert.strictEqual(
      instrumentFailure({ ripr_version: arm }, ['ripr_version', 'powershell_empty', 'extension_probe']),
      'no valid trial for: powershell_empty, extension_probe'
    );
  });

  test('the CLI entry point exits 1 for an unsuccessful target and 0 for a good one', () => {
    const cli = path.join(__dirname, '..', 'probeTiming.js');
    const out = path.join(dir, 'report.json');
    const bad = cp.spawnSync(process.execPath, [cli, fake('bad', 'exit 4'), out], { encoding: 'utf8' });
    assert.strictEqual(bad.status, 1, bad.stderr);
    assert.match(bad.stderr, /instrument invalid: no valid trial for: ripr_version, extension_probe/);
    const good = cp.spawnSync(process.execPath, [cli, fake('good', 'echo "ripr 0.11.0 (abc)"'), out], { encoding: 'utf8' });
    assert.strictEqual(good.status, 0, good.stderr);
    assert.strictEqual(JSON.parse(fs.readFileSync(out, 'utf8')).schema, 2);
  });
});
