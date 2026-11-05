import * as assert from 'assert';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  FixtureProducerError, producerTimeoutMs, runFixtureProducer, stageGenuineActionableQueue
} from '../actionableQueueFixture';

function running(pid: number): boolean {
  try { process.kill(pid, 0); }
  catch (error) {
    if ((error as NodeJS.ErrnoException).code === 'ESRCH') { return false; }
    throw error;
  }
  if (process.platform === 'linux') {
    try {
      const stat = fs.readFileSync(`/proc/${pid}/stat`, 'utf8');
      return !['Z', 'X'].includes(stat.slice(stat.lastIndexOf(')') + 2, stat.lastIndexOf(')') + 3));
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === 'ENOENT') {
        // Some executors expose a different procfs namespace. Missing procfs
        // evidence cannot override a successful native process-existence probe.
        try { process.kill(pid, 0); return true; }
        catch (probeError) {
          if ((probeError as NodeJS.ErrnoException).code === 'ESRCH') { return false; }
          throw probeError;
        }
      }
      throw error;
    }
  }
  return true;
}

async function waitFor(predicate: () => boolean, message: string): Promise<void> {
  const until = Date.now() + 15_000;
  while (!predicate()) {
    assert.ok(Date.now() < until, message);
    await new Promise(resolve => setTimeout(resolve, 20));
  }
}

async function within<T>(promise: Promise<T>): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([promise, new Promise<T>((_resolve, reject) => {
      timer = setTimeout(() => reject(new Error('producer did not finish after containment cleanup')), 35_000);
    })]);
  } finally { clearTimeout(timer); }
}

suite('Genuine fixture producer process boundary', function () {
  this.timeout(60_000);
  let root: string;
  const ownedPids = new Set<number>();
  const nativeBudget = process.platform === 'win32' ? 25_000 : 5000;

  setup(() => { root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr producer ')); });
  teardown(async () => {
    for (const name of ['parent.pid', 'descendant.pid']) {
      const file = path.join(root, name);
      if (fs.existsSync(file)) { ownedPids.add(Number(fs.readFileSync(file, 'utf8'))); }
    }
    for (const pid of ownedPids) {
      if (running(pid)) { process.kill(pid, 'SIGKILL'); }
      await waitFor(() => !running(pid), `owned process ${pid} remained active after cleanup`);
    }
    ownedPids.clear();
    fs.rmSync(root, { recursive: true, force: true });
  });

  function run(source: string, extra: Partial<Parameters<typeof runFixtureProducer>[2]> = {}): Promise<string> {
    fs.writeFileSync(path.join(root, 'fixture.js'), source);
    return runFixtureProducer(process.execPath, ['fixture.js'], {
      cwd: root, env: {
        ...process.env, ELECTRON_RUN_AS_NODE: '1', RIPR_FIXTURE_CONTROL: 'literal value with spaces'
      },
      timeoutMs: nativeBudget, ...extra
    });
  }

  test('runs asynchronously with the explicit child cwd/env and visible progress', async () => {
    let heartbeat = false;
    const timer = setTimeout(() => { heartbeat = true; }, 20);
    const progress: string[] = [];
    try {
      const transcript = await run(`setTimeout(() => {
        console.log(JSON.stringify({cwd: process.cwd(), value: process.env.RIPR_FIXTURE_CONTROL}));
      }, 150);`, { onProgress: value => progress.push(value), progressIntervalMs: 40 });
      const result = JSON.parse(transcript);
      assert.deepStrictEqual({ ...result, cwd: fs.realpathSync.native(result.cwd) },
        { cwd: fs.realpathSync.native(root), value: 'literal value with spaces' });
      assert.strictEqual(heartbeat, true, 'producer blocked the parent event loop');
      assert.ok(progress.length >= 2, 'a running producer needs more than a start message');
      assert.ok(progress.every(message => message.includes('fixture producer running')));
    } finally { clearTimeout(timer); }
  });

  test('retains native status and both streams for a failed process', async () => {
    await assert.rejects(run(`process.stdout.write('native stdout'); process.stderr.write('native stderr'); process.exitCode = 23;`), error => {
      assert.ok(error instanceof FixtureProducerError);
      assert.strictEqual(error.kind, 'process_failure');
      assert.strictEqual(error.status, 23);
      assert.strictEqual(error.stdout, 'native stdout');
      assert.ok(error.stderr.includes('native stderr'));
      return true;
    });
  });

  test('retains the final transcript before accepting native success', async () => {
    const expected = `${'producer output\n'.repeat(32_768)}final producer receipt\n`;
    const transcript = await run(`process.stdout.write(${JSON.stringify(expected)});`);
    assert.strictEqual(transcript, expected);
  });

  test('reports the actual missing executable as a spawn failure', async () => {
    const missing = path.join(root, 'missing-producer');
    await assert.rejects(runFixtureProducer(missing, [], { cwd: root, env: process.env, timeoutMs: nativeBudget }), error => {
      assert.ok(error instanceof FixtureProducerError);
      assert.strictEqual(error.kind, 'spawn_failure');
      assert.ok(error.message.includes('could not start'));
      return true;
    });
  });

  test('refuses an already cancelled producer before starting a child', async () => {
    const cancelled = new AbortController();
    cancelled.abort('before-start');
    await assert.rejects(run(`require('fs').writeFileSync('unexpected-start', 'started');`, { signal: cancelled.signal }), error => {
      assert.ok(error instanceof FixtureProducerError);
      assert.strictEqual(error.kind, 'cancelled');
      assert.ok(error.message.includes('before-start'));
      return true;
    });
    assert.strictEqual(fs.existsSync(path.join(root, 'unexpected-start')), false);
  });

  test('rejects output overflow and bounds the retained bytes', async () => {
    await assert.rejects(run(`process.stdout.write('x'.repeat(4096)); setInterval(() => {}, 1000);`, { maxOutputBytes: 128 }), error => {
      assert.ok(error instanceof FixtureProducerError);
      assert.strictEqual(error.kind, 'output_limit');
      assert.ok(Buffer.byteLength(error.stdout) + Buffer.byteLength(error.stderr) <= 128);
      return true;
    });
  });

  for (const mode of ['timeout-after-primary-exit', 'cancel-active-tree'] as const) {
    test(`${mode} terminates an owned descendant that ignores SIGTERM and inherits pipes`, async function () {
      this.timeout(60_000);
      fs.writeFileSync(path.join(root, 'descendant.js'), `
        process.on('SIGTERM', () => {});
        require('fs').writeFileSync('descendant.pid', String(process.pid));
        setInterval(() => {}, 1000);
      `);
      const cancelled = new AbortController();
      // Windows Node otherwise kills its non-detached child at primary exit.
      // Detaching this fixture bypasses that private Job but retains the outer
      // wrapper Job: libuv does not request CREATE_BREAKAWAY_FROM_JOB.
      const outcome = run(`
        const fs = require('fs');
        process.on('SIGTERM', () => {});
        fs.writeFileSync('parent.pid', String(process.pid));
        const child = require('child_process').spawn(process.execPath, ['descendant.js'], {
          stdio: 'inherit', detached: process.platform === 'win32'
        });
        const timer = setInterval(() => {
          if (fs.existsSync('descendant.pid')) {
            clearInterval(timer);
            ${mode === 'timeout-after-primary-exit'
              ? 'child.unref(); process.exit(0);' : 'setInterval(() => {}, 1000);'}
          }
        }, 20);
      `, { signal: cancelled.signal, timeoutMs: mode === 'timeout-after-primary-exit' ? nativeBudget : 30_000 });
      // Attach the rejection handler before waiting for the child handshake.
      let settled = false;
      const observed = outcome.then(value => ({ value }), error => ({ error })).then(result => {
        settled = true;
        return result;
      });
      await waitFor(() => fs.existsSync(path.join(root, 'descendant.pid')), 'descendant did not establish the cleanup subject');
      const parent = Number(fs.readFileSync(path.join(root, 'parent.pid'), 'utf8'));
      const descendant = Number(fs.readFileSync(path.join(root, 'descendant.pid'), 'utf8'));
      ownedPids.add(parent); ownedPids.add(descendant);
      assert.ok(running(descendant), 'the descendant must actually be alive before the control');
      if (process.platform !== 'win32') {
        process.kill(descendant, 'SIGTERM');
        await new Promise(resolve => setTimeout(resolve, 50));
        assert.ok(running(descendant), 'the descendant did not resist graceful termination');
      }
      if (mode === 'timeout-after-primary-exit') {
        await waitFor(() => !running(parent), 'the primary did not exit before the pipe-holding control');
        assert.strictEqual(settled, false, 'the producer settled before exercising inherited pipes');
        assert.ok(running(descendant), 'the descendant must still hold pipes after primary exit');
      }
      if (mode === 'cancel-active-tree') { cancelled.abort('explicit-stop'); }
      const result = await within(observed);
      assert.ok('error' in result && result.error instanceof FixtureProducerError, JSON.stringify(result));
      assert.strictEqual(result.error.kind, mode === 'cancel-active-tree' ? 'cancelled' : 'timeout');
      await waitFor(() => !running(parent) && !running(descendant), 'the primary or descendant remained active after producer completion');
    });
  }

  test('validates the deadline instead of silently disabling or overflowing it', () => {
    assert.strictEqual(producerTimeoutMs(undefined), 1_800_000);
    assert.strictEqual(producerTimeoutMs('7200000'), 7_200_000);
    for (const raw of ['', '0', '-1', '1.5', 'NaN', 'Infinity', '7200001', '1e3', ' 1000']) {
      assert.throws(() => producerTimeoutMs(raw), /RIPR_TEST_PRODUCER_TIMEOUT_MS/);
    }
  });
});

suite('Genuine fixture producer adapter', () => {
  let root: string;
  let oldPath: string | undefined;
  let oldTimeout: string | undefined;
  let oldCase: string | undefined;
  let oldElectron: string | undefined;

  setup(function () {
    if (process.platform === 'win32') { this.skip(); return; }
    root = fs.mkdtempSync(path.join(os.tmpdir(), 'ripr queue adapter '));
    oldPath = process.env.PATH;
    oldTimeout = process.env.RIPR_TEST_PRODUCER_TIMEOUT_MS;
    oldCase = process.env.RIPR_FIXTURE_CASE;
    oldElectron = process.env.ELECTRON_RUN_AS_NODE;
    const cargo = path.join(root, 'cargo');
    const interpreter = path.join(root, 'node runtime');
    const script = path.join(root, 'cargo fixture.js');
    const quote = (value: string) => "'" + value.replace(/'/g, "'\\''") + "'";
    // A shebang cannot quote an interpreter path containing spaces. Exercise
    // the same quoted exec boundary for both the mock Cargo and its interpreter.
    fs.writeFileSync(interpreter, `#!/bin/sh\nexec ${quote(process.execPath)} "$@"\n`, { mode: 0o700 });
    fs.writeFileSync(cargo, `#!/bin/sh\nexec ${quote(interpreter)} ${quote(script)} "$@"\n`, { mode: 0o700 });
    fs.writeFileSync(script, `
const fs = require('fs'), path = require('path');
const output = process.env.RIPR_CORPUS_PROOF_EXPORT_DIR;
if (process.env.RIPR_FIXTURE_CASE === 'timeout') {
  process.stderr.write('native producer waiting\\n');
  setInterval(() => {}, 1000);
} else {
  const root = path.join(output, 'fixture');
  const queue = path.join(root, 'target/ripr/reports/actionable-gaps.json');
  fs.mkdirSync(path.dirname(queue), {recursive:true});
  fs.writeFileSync(queue, JSON.stringify({packets:[{
    canonical_gap_id:'gap:ba9f4264c3029d1c',
    verify_command:'cargo test -p ordinary_shop above_threshold_discounts',
    verify_command_source:'related_test_or_observer.name',
    public_projection_eligible:true, projection_exclusion_reasons:[]
  }]}));
  const executable = path.join(output, 'xtask-test');
  fs.writeFileSync(executable, 'controlled executable bytes');
  console.log(JSON.stringify({reason:'compiler-artifact',target:{name:'xtask',src_path:path.join(process.cwd(),'xtask/src/main.rs')},profile:{test:true},executable}));
  if (process.env.RIPR_FIXTURE_CASE !== 'zero-tests') {
    console.log('test tests::rust_corpus_projection::genuine_workspace_producer_preserves_current_corpus_and_native_bounded_projection ... ok');
    console.log('1 passed; 0 failed; 0 ignored');
  }
}
`);
    process.env.PATH = `${root}${path.delimiter}${oldPath}`;
    process.env.ELECTRON_RUN_AS_NODE = '1';
    process.env.RIPR_TEST_PRODUCER_TIMEOUT_MS = '5000';
    delete process.env.RIPR_FIXTURE_CASE;
  });
  teardown(() => {
    if (process.platform === 'win32') { return; }
    for (const [key, value] of [
      ['PATH', oldPath], ['RIPR_TEST_PRODUCER_TIMEOUT_MS', oldTimeout],
      ['RIPR_FIXTURE_CASE', oldCase], ['ELECTRON_RUN_AS_NODE', oldElectron]
    ]) {
      if (value === undefined) { delete process.env[key!]; }
      else { process.env[key!] = value; }
    }
    fs.rmSync(root, { recursive: true, force: true });
  });

  function stage(): Promise<string> {
    return stageGenuineActionableQueue(path.resolve(__dirname, '../../../../..'), path.join(root, 'export'));
  }

  test('preserves test, executable and queue validation before publishing the receipt', async () => {
    const fixture = await stage();
    const receipt = JSON.parse(fs.readFileSync(path.join(path.dirname(fixture), 'producer-receipt.json'), 'utf8'));
    assert.strictEqual(receipt.native_exit_code, 0);
    assert.strictEqual(receipt.cases, 1);
    assert.strictEqual(receipt.queue, path.join(fixture, 'target/ripr/reports/actionable-gaps.json'));
    assert.strictEqual(receipt.executable_sha256, '430786dda544af57cee67ff74709ace9dd922e08e0a6f36d56921b7b82088672');
    assert.match(receipt.queue_sha256, /^[a-f0-9]{64}$/);
  });

  test('zero executed producer tests cannot publish an accepted receipt', async () => {
    process.env.RIPR_FIXTURE_CASE = 'zero-tests';
    await assert.rejects(stage(), /did not execute its one intended producer test/);
    const exported = fs.readdirSync(root).find(name => name.startsWith('export-'))!;
    assert.strictEqual(fs.existsSync(path.join(root, exported, 'producer-receipt.json')), false);
  });

  test('timeout retains the underlying producer failure and restores signal handlers', async function () {
    this.timeout(10_000);
    const beforeInt = process.listeners('SIGINT'), beforeTerm = process.listeners('SIGTERM');
    process.env.RIPR_FIXTURE_CASE = 'timeout';
    process.env.RIPR_TEST_PRODUCER_TIMEOUT_MS = '1000';
    await assert.rejects(stage(), /fixture producer timed out after 1000ms/);
    const exported = fs.readdirSync(root).find(name => name.startsWith('export-'))!;
    const failure = fs.readFileSync(path.join(root, exported, 'producer-failure.log'), 'utf8');
    assert.ok(failure.includes('native producer waiting'));
    assert.ok(failure.includes('timed out after 1000ms'));
    assert.strictEqual(fs.existsSync(path.join(root, exported, 'producer-receipt.json')), false);
    assert.deepStrictEqual(process.listeners('SIGINT'), beforeInt);
    assert.deepStrictEqual(process.listeners('SIGTERM'), beforeTerm);
  });
});
