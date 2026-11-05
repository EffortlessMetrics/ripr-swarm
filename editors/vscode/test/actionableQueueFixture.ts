import { execFileSync } from 'child_process';
import { createHash } from 'crypto';
import * as fs from 'fs';
import * as path from 'path';
import {
  probeProcessExitCode, probeProcessStartFailure, probeWrapperReportedExitCode,
  spawnProbeProcess, terminateProbeProcessTree
} from '../src/lspCompatibility';

const DEFAULT_PRODUCER_TIMEOUT_MS = 30 * 60 * 1000;
const MAX_PRODUCER_TIMEOUT_MS = 2 * 60 * 60 * 1000;
const MAX_PRODUCER_OUTPUT_BYTES = 16 * 1024 * 1024;

export function producerTimeoutMs(raw: string | undefined): number {
  if (raw === undefined) {
    return DEFAULT_PRODUCER_TIMEOUT_MS;
  }
  if (!/^[1-9][0-9]*$/.test(raw) || Number(raw) > MAX_PRODUCER_TIMEOUT_MS) {
    throw new Error('RIPR_TEST_PRODUCER_TIMEOUT_MS must be an integer from 1 to 7200000');
  }
  return Number(raw);
}

export interface FixtureProducerOptions {
  cwd: string;
  env: NodeJS.ProcessEnv;
  timeoutMs: number;
  signal?: AbortSignal;
  onProgress?: (message: string) => void;
  maxOutputBytes?: number;
  progressIntervalMs?: number;
}

export class FixtureProducerError extends Error {
  readonly kind: string;
  readonly stdout: string;
  readonly stderr: string;
  readonly status: number | null;
  readonly signal: NodeJS.Signals | null;

  constructor(kind: string, message: string, stdout = '', stderr = '',
    status: number | null = null, signal: NodeJS.Signals | null = null) {
    super(message);
    this.name = 'FixtureProducerError';
    this.kind = kind;
    this.stdout = stdout;
    this.stderr = stderr;
    this.status = status;
    this.signal = signal;
  }
}

// Keep the deadline active until pipes close: an exited primary can leave a
// descendant holding stdout. The existing process owner contains both platforms.
export async function runFixtureProducer(
  command: string, args: readonly string[], options: FixtureProducerOptions
): Promise<string> {
  const maxBytes = options.maxOutputBytes ?? MAX_PRODUCER_OUTPUT_BYTES;
  const progressMs = options.progressIntervalMs ?? 15_000;
  for (const [name, value] of [['timeoutMs', options.timeoutMs], ['maxOutputBytes', maxBytes],
    ['progressIntervalMs', progressMs]] as const) {
    if (!Number.isSafeInteger(value) || value <= 0 || value > 2_147_483_647) {
      throw new Error(`fixture producer ${name} must be a positive bounded integer`);
    }
  }
  if (options.signal?.aborted) {
    throw new FixtureProducerError('cancelled', `fixture producer cancelled: ${String(options.signal.reason)}`);
  }
  return new Promise<string>((resolve, reject) => {
    const started = Date.now();
    const stdout: Buffer[] = [], stderr: Buffer[] = [];
    let retainedBytes = 0;
    let failure: { kind: string; message: string } | undefined;
    let child: ReturnType<typeof spawnProbeProcess>;
    try {
      child = spawnProbeProcess(command, args, false, { cwd: options.cwd, env: options.env });
    } catch (error) {
      reject(new FixtureProducerError('spawn_failure', `fixture producer could not start: ${String(error)}`));
      return;
    }
    const stop = (kind: string, message: string): void => {
      if (!failure) {
        failure = { kind, message };
        terminateProbeProcessTree(child);
      }
    };
    const onAbort = (): void => stop('cancelled', `fixture producer cancelled: ${String(options.signal?.reason)}`);
    const progress = (): void => {
      try {
        options.onProgress?.(`fixture producer running (${Math.floor((Date.now() - started) / 1000)}s; deadline ${options.timeoutMs}ms)`);
      } catch (error) {
        stop('progress_failure', `fixture producer progress failed: ${String(error)}`);
      }
    };
    const timer = setTimeout(() => stop('timeout', `fixture producer timed out after ${options.timeoutMs}ms`), options.timeoutMs);
    const heartbeat = setInterval(progress, progressMs);
    options.signal?.addEventListener('abort', onAbort, { once: true });
    const collect = (destination: Buffer[], chunk: Buffer): void => {
      const available = maxBytes - retainedBytes;
      const count = Math.min(available, chunk.length);
      if (count > 0) {
        destination.push(chunk.subarray(0, count));
        retainedBytes += count;
      }
      if (count < chunk.length) {
        stop('output_limit', `fixture producer output exceeded ${maxBytes} bytes`);
      }
    };
    child.stdout.on('data', (chunk: Buffer) => collect(stdout, chunk));
    child.stderr.on('data', (chunk: Buffer) => collect(stderr, chunk));
    child.once('error', error => stop('spawn_failure', `fixture producer could not start: ${error.message}`));
    child.stdin.on('error', error => stop('process_failure', `fixture producer stdin failed: ${error.message}`));
    child.once('close', (wrapperCode, signal) => {
      clearTimeout(timer);
      clearInterval(heartbeat);
      options.signal?.removeEventListener('abort', onAbort);
      const code = probeProcessExitCode(child, wrapperCode);
      const startFailure = probeProcessStartFailure(child);
      if (!failure && startFailure) {
        failure = { kind: 'spawn_failure', message: `fixture producer could not start: ${startFailure}` };
      }
      if (!failure && process.platform === 'win32' && probeWrapperReportedExitCode(child) === undefined) {
        failure = { kind: 'process_failure', message: 'fixture producer wrapper did not report the native exit code' };
      }
      if (!failure && code !== 0) {
        failure = { kind: 'process_failure', message: `fixture producer exited with status=${code}; signal=${signal}` };
      }
      terminateProbeProcessTree(child);
      const out = Buffer.concat(stdout).toString('utf8');
      if (failure) {
        reject(new FixtureProducerError(failure.kind, failure.message, out,
          Buffer.concat(stderr).toString('utf8'), code, signal));
      } else {
        resolve(out);
      }
    });
    child.stdin.end();
    progress();
  });
}

// Generate once before the trusted host, using the existing public producer
// composition test. Keep the original artifact bytes and compiler receipt.
export async function stageGenuineActionableQueue(repoRoot: string, exportPrefix: string): Promise<string> {
  const timeoutMs = producerTimeoutMs(process.env.RIPR_TEST_PRODUCER_TIMEOUT_MS);
  const exportDirectory = fs.mkdtempSync(`${exportPrefix}-`);
  const testName = 'tests::rust_corpus_projection::genuine_workspace_producer_preserves_current_corpus_and_native_bounded_projection';
  const args = ['test', '--locked', '-p', 'xtask', '--bin', 'xtask', '--message-format=json',
    testName, '--', '--exact', '--nocapture'];
  let transcript: string;
  const cancellation = new AbortController();
  const onInterrupt = (): void => cancellation.abort(new Error('interrupted by SIGINT'));
  const onTerminate = (): void => cancellation.abort(new Error('interrupted by SIGTERM'));
  process.once('SIGINT', onInterrupt);
  process.once('SIGTERM', onTerminate);
  try {
    console.info(`Preparing genuine actionable-queue fixture with ${testName}; logs: ${exportDirectory}`);
    transcript = await runFixtureProducer('cargo', args, {
      cwd: repoRoot, timeoutMs, signal: cancellation.signal,
      env: { ...process.env, CARGO_BUILD_JOBS: '1', RIPR_CORPUS_PROOF_EXPORT_DIR: exportDirectory },
      onProgress: message => console.info(`${message}; logs: ${exportDirectory}`)
    });
  } catch (error) {
    const result = error as { stdout?: string; stderr?: string; status?: number; signal?: string };
    fs.writeFileSync(path.join(exportDirectory, 'producer-failure.log'),
      `${String(error)}\n${result.stdout ?? ''}\n${result.stderr ?? ''}\nstatus=${result.status}; signal=${result.signal}\n`);
    throw new Error(`${String(error)}; producer logs: ${exportDirectory}`, { cause: error });
  } finally {
    process.removeListener('SIGINT', onInterrupt);
    process.removeListener('SIGTERM', onTerminate);
  }
  fs.writeFileSync(path.join(exportDirectory, 'producer-cargo.log'), transcript);
  if (!transcript.includes(`test ${testName} ... ok`)
    || !transcript.includes('1 passed; 0 failed; 0 ignored')) {
    throw new Error('genuine queue setup did not execute its one intended producer test');
  }
  const messages = transcript.split(/\r?\n/).flatMap(line => {
    try { return [JSON.parse(line) as Record<string, any>]; }
    catch { return []; }
  });
  const selected = messages.filter(message => message.reason === 'compiler-artifact'
    && message.target?.name === 'xtask' && message.profile?.test === true
    && message.executable && path.resolve(message.target.src_path) === path.join(repoRoot, 'xtask/src/main.rs'));
  if (selected.length !== 1) {
    throw new Error('genuine queue setup needs exactly one compiler-selected xtask test executable');
  }
  const roots = fs.readdirSync(exportDirectory, { withFileTypes: true })
    .filter(entry => entry.isDirectory()).map(entry => path.join(exportDirectory, entry.name));
  if (roots.length !== 1) {
    throw new Error('genuine queue setup needs exactly one exported physical fixture root');
  }
  const root = roots[0];
  const queuePath = path.join(root, 'target/ripr/reports/actionable-gaps.json');
  const queueBytes = fs.readFileSync(queuePath);
  const queue = JSON.parse(queueBytes.toString('utf8'));
  const packet = queue.packets?.[0];
  if (queue.packets?.length !== 1 || packet?.canonical_gap_id !== 'gap:ba9f4264c3029d1c'
    || packet.verify_command !== 'cargo test -p ordinary_shop above_threshold_discounts'
    || packet.verify_command_source !== 'related_test_or_observer.name'
    || packet.public_projection_eligible !== true || packet.projection_exclusion_reasons?.length !== 0) {
    throw new Error('actual producer queue did not retain the required genuine bounded subject');
  }
  const producerHead = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: repoRoot, encoding: 'utf8' }).trim();
  fs.writeFileSync(path.join(exportDirectory, 'producer-receipt.json'), `${JSON.stringify({
    producer_head: producerHead, test: testName, native_exit_code: 0, cases: 1,
    compiler_message: selected[0],
    executable_sha256: createHash('sha256').update(fs.readFileSync(selected[0].executable)).digest('hex'),
    fixture_root: root, queue: queuePath,
    queue_sha256: createHash('sha256').update(queueBytes).digest('hex')
  }, null, 2)}\n`);
  return root;
}
