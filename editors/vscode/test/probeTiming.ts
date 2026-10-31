// #9631 timing instrument (schema 2) for the Windows probe wrapper.
//
// Each trial uses a monotonic clock, has a hard deadline with a recorded cleanup
// outcome, verifies target output and exit or start-failure markers, and is
// labelled with its round and position so order effects stay visible. A row is
// `valid` only when the target actually succeeded, so an unsuccessful target can
// no longer produce the same timing row as success. Diagnostic only: this is not
// Job-containment proof (the Job-contract tests own that) and its first trial is
// the first launch after the run began, not an isolated machine-cold start.
import * as cp from 'child_process';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  probeProcessExitCode,
  probeProcessStartFailure,
  probeWrapperReportedExitCode,
  spawnProbeProcess,
  terminateProbeProcessTree
} from '../src/lspCompatibility';

export type ArmName = 'ripr_version' | 'powershell_empty' | 'extension_probe';

export interface TimingOptions {
  readonly server: string;
  readonly rounds?: number;
  readonly deadlineMs?: number;
  readonly closeGraceMs?: number;
}

export interface TimingRow {
  readonly ordinal: number;
  readonly round: number;
  readonly position: number;
  readonly first_in_series: boolean;
  readonly arm: ArmName;
  readonly ms: number;
  readonly exit_code: number | null;
  readonly signal: string | null;
  readonly deadline_hit: boolean;
  readonly cleanup: string;
  readonly spawn_error: string | null;
  readonly start_failure: string | null;
  readonly wrapper_exit_marker: number | null;
  readonly output_ok: boolean;
  readonly invalid_reason: string | null;
  readonly valid: boolean;
  readonly stdout_head: string;
}

export interface ArmSummary {
  readonly valid: number;
  readonly invalid: number;
  readonly min_ms: number | null;
  readonly median_ms: number | null;
  readonly max_ms: number | null;
  readonly first_round_valid_ms: number[];
  readonly later_rounds_median_ms: number | null;
}

export interface TimingReport {
  readonly schema: 2;
  readonly note: string;
  readonly clock: string;
  readonly deadline_ms: number;
  readonly rounds: number;
  readonly platform: { os: string; release: string; node: string; cpus: number };
  readonly candidate_sha: string | null;
  readonly identities: Record<string, FileIdentity | null>;
  readonly summary: Partial<Record<ArmName, ArmSummary>>;
  readonly rows: TimingRow[];
}

interface FileIdentity {
  readonly path: string;
  readonly bytes?: number;
  readonly sha256?: string;
  readonly error?: string;
}

interface Verdict {
  exit_code: number | null;
  output_ok: boolean;
  wrapper_exit_marker?: number;
  start_failure?: string;
  invalid_reason?: string;
}

interface TrialContext {
  child: cp.ChildProcess | null;
  code: number | null;
  out: string;
  spawnError: string | null;
  deadlineHit: boolean;
}

interface Arm {
  start(server: string): cp.ChildProcessWithoutNullStreams;
  check(ctx: TrialContext): Verdict;
}

const TAIL = 4096;
const VERSION_RE = /^ripr \d+\.\d+\.\d+/;
const isWin = process.platform === 'win32';

function identity(file: string): FileIdentity {
  try {
    const bytes = fs.readFileSync(file);
    return { path: file, bytes: bytes.length, sha256: crypto.createHash('sha256').update(bytes).digest('hex') };
  } catch (error) {
    return { path: file, error: String((error as Error).message) };
  }
}

function plainCheck(expectVersion: boolean): Arm['check'] {
  return ({ code, out, spawnError, deadlineHit }) => {
    const outputOk = expectVersion ? VERSION_RE.test(out.trim()) : true;
    const verdict: Verdict = { exit_code: code, output_ok: outputOk };
    if (spawnError) verdict.invalid_reason = 'spawn_error';
    else if (deadlineHit) verdict.invalid_reason = 'deadline';
    else if (code !== 0) verdict.invalid_reason = 'nonzero_exit';
    else if (!outputOk) verdict.invalid_reason = 'output_mismatch';
    return verdict;
  };
}

const extensionCheck: Arm['check'] = ({ child, code, out, spawnError, deadlineHit }) => {
  const exit = child ? probeProcessExitCode(child, code) : code;
  const marker = child ? probeWrapperReportedExitCode(child) : undefined;
  const startFailure = child ? probeProcessStartFailure(child) : undefined;
  const outputOk = VERSION_RE.test(out.trim());
  const verdict: Verdict = { exit_code: exit, output_ok: outputOk, wrapper_exit_marker: marker, start_failure: startFailure };
  if (spawnError) verdict.invalid_reason = 'spawn_error';
  else if (deadlineHit) verdict.invalid_reason = 'deadline';
  else if (startFailure) verdict.invalid_reason = 'target_start_failure';
  else if (isWin && marker === undefined) verdict.invalid_reason = 'missing_exit_marker';
  else if (exit !== 0) verdict.invalid_reason = 'nonzero_exit';
  else if (!outputOk) verdict.invalid_reason = 'output_mismatch';
  return verdict;
};

const stdio: ['pipe', 'pipe', 'pipe'] = ['pipe', 'pipe', 'pipe'];
const ARMS: Record<ArmName, Arm> = {
  ripr_version: {
    start: (server) => cp.spawn(server, ['--version'], { shell: false, windowsHide: true, stdio }),
    check: plainCheck(true)
  },
  powershell_empty: {
    start: () => cp.spawn('powershell.exe', ['-NoProfile', '-NonInteractive', '-Command', 'exit 0'], { shell: false, windowsHide: true, stdio }),
    check: plainCheck(false)
  },
  extension_probe: {
    start: (server) => spawnProbeProcess(server, ['--version'], false),
    check: extensionCheck
  }
};

type Measured = Omit<TimingRow, 'ordinal' | 'round' | 'position' | 'first_in_series'>;

function trial(arm: ArmName, server: string, deadlineMs: number, closeGraceMs: number): Promise<Measured> {
  return new Promise((resolve) => {
    const t0 = process.hrtime.bigint();
    let out = '';
    let spawnError: string | null = null;
    let deadlineHit = false;
    let cleanup = 'not_needed';
    let settled = false;
    let child: cp.ChildProcessWithoutNullStreams | null = null;
    let deadlineTimer: NodeJS.Timeout | undefined;
    let graceTimer: NodeJS.Timeout | undefined;
    const finish = (code: number | null, signal: string | null, closeObserved: boolean): void => {
      if (settled) return;
      settled = true;
      clearTimeout(deadlineTimer);
      clearTimeout(graceTimer);
      const ms = Number((process.hrtime.bigint() - t0) / 1000n) / 1000;
      if (deadlineHit) cleanup = closeObserved ? 'closed_after_terminate' : 'close_not_observed_after_terminate';
      const verdict = ARMS[arm].check({ child, code, out, spawnError, deadlineHit });
      resolve({
        arm,
        ms: Math.round(ms * 10) / 10,
        exit_code: verdict.exit_code,
        signal,
        deadline_hit: deadlineHit,
        cleanup,
        spawn_error: spawnError,
        start_failure: verdict.start_failure ?? null,
        wrapper_exit_marker: verdict.wrapper_exit_marker ?? null,
        output_ok: verdict.output_ok,
        invalid_reason: verdict.invalid_reason ?? null,
        valid: verdict.invalid_reason === undefined,
        stdout_head: out.slice(0, 120)
      });
    };
    try {
      child = ARMS[arm].start(server);
    } catch (error) {
      spawnError = String((error as Error).message);
      finish(null, null, false);
      return;
    }
    const started = child;
    started.on('error', (error) => {
      spawnError = String(error.message);
      finish(null, null, false);
    });
    started.stdout.on('data', (chunk: Buffer) => {
      out = (out + chunk.toString('utf8')).slice(-TAIL);
    });
    started.stderr.resume();
    started.once('close', (code, signal) => finish(code, signal, true));
    started.stdin.end();
    deadlineTimer = setTimeout(() => {
      deadlineHit = true;
      try {
        terminateProbeProcessTree(started);
        cleanup = 'terminate_requested';
      } catch (error) {
        cleanup = `terminate_failed: ${(error as Error).message}`;
      }
      graceTimer = setTimeout(() => finish(null, null, false), closeGraceMs);
    }, deadlineMs);
  });
}

function median(values: number[]): number | null {
  if (values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const mid = sorted.length >> 1;
  return sorted.length % 2 ? sorted[mid] : Math.round(((sorted[mid - 1] + sorted[mid]) / 2) * 10) / 10;
}

/** Arms a valid report must contain on this platform. */
export function requiredArms(platform: NodeJS.Platform = process.platform): ArmName[] {
  return platform === 'win32' ? ['ripr_version', 'powershell_empty', 'extension_probe'] : ['ripr_version', 'extension_probe'];
}

/**
 * Why the report cannot be trusted as a measurement, or null. Every required arm
 * must be present with at least one valid trial: a wrapped probe that never
 * succeeded, or an arm missing from the summary, must not pass just because the
 * direct target did.
 */
export function instrumentFailure(
  summary: Partial<Record<ArmName, ArmSummary>>,
  required: readonly ArmName[] = requiredArms()
): string | null {
  const bad = required.filter((arm) => !summary[arm] || summary[arm]?.valid === 0);
  return bad.length > 0 ? `no valid trial for: ${bad.join(', ')}` : null;
}

export async function runTimingInstrument(options: TimingOptions): Promise<TimingReport> {
  const rounds = options.rounds ?? 5;
  const deadlineMs = options.deadlineMs ?? 60000;
  const closeGraceMs = options.closeGraceMs ?? 10000;
  const armNames = requiredArms();
  const rows: TimingRow[] = [];
  let ordinal = 0;
  for (let round = 1; round <= rounds; round++) {
    // Rotate the starting arm each round so no arm is always first in a round.
    const shift = (round - 1) % armNames.length;
    const order = armNames.slice(shift).concat(armNames.slice(0, shift));
    for (let position = 0; position < order.length; position++) {
      const measured = await trial(order[position], options.server, deadlineMs, closeGraceMs);
      rows.push({ ordinal: ordinal++, round, position, first_in_series: round === 1, ...measured });
    }
  }
  const summary: Partial<Record<ArmName, ArmSummary>> = {};
  for (const arm of armNames) {
    const own = rows.filter((row) => row.arm === arm);
    const valid = own.filter((row) => row.valid).map((row) => row.ms);
    summary[arm] = {
      valid: valid.length,
      invalid: own.length - valid.length,
      min_ms: valid.length ? Math.min(...valid) : null,
      median_ms: median(valid),
      max_ms: valid.length ? Math.max(...valid) : null,
      first_round_valid_ms: own.filter((row) => row.valid && row.first_in_series).map((row) => row.ms),
      later_rounds_median_ms: median(own.filter((row) => row.valid && !row.first_in_series).map((row) => row.ms))
    };
  }
  const wrapperModule = path.join(__dirname, '..', 'src', 'lspCompatibility.js');
  return {
    schema: 2,
    note: 'Ordered trials on one runner: first_in_series is the first launch after this run began, not an isolated machine-cold start; summaries use valid rows only.',
    clock: 'process.hrtime.bigint (monotonic)',
    deadline_ms: deadlineMs,
    rounds,
    platform: { os: process.platform, release: os.release(), node: process.version, cpus: os.cpus().length },
    candidate_sha: process.env.CANDIDATE_SHA ?? null,
    identities: {
      target: identity(options.server),
      probe_wrapper_module: identity(wrapperModule),
      powershell: isWin && process.env.SystemRoot
        ? identity(path.join(process.env.SystemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe'))
        : null
    },
    summary,
    rows
  };
}

if (require.main === module) {
  const [, , server, outPath] = process.argv;
  if (!server || !outPath) {
    console.error('usage: node probeTiming.js <ripr-server> <output.json>');
    process.exit(2);
  }
  runTimingInstrument({ server }).then((report) => {
    fs.writeFileSync(outPath, JSON.stringify(report, null, 2));
    console.log(JSON.stringify({ summary: report.summary, identities: report.identities }, null, 2));
    const failure = instrumentFailure(report.summary);
    if (failure) {
      console.error(`instrument invalid: ${failure}`);
      process.exit(1);
    }
  });
}
