/**
 * Controller-owned post-start crash recovery (#1744).
 *
 * Unexpected language-client session ends charge a bounded budget. Retries are
 * serialized through one generation-checked timer with exponential backoff.
 * Intentional stops must not charge the budget (the controller sets
 * `intentionalStopActive` around stop/reconfigure/restart paths).
 */

export const CRASH_RECOVERY_SESSION_BUDGET = 3;

/** Backoff delays after the 1st and 2nd failed sessions (ms). */
export const CRASH_RECOVERY_BACKOFF_MS: readonly number[] = [1_000, 2_000, 4_000];

/** Running time required before a session clears the crash budget (#1744). */
export const CRASH_RECOVERY_STABLE_MS = 5_000;

export type CrashRecoveryScheduler = (
  callback: () => void,
  delayMs: number
) => ReturnType<typeof setTimeout>;

export const defaultCrashRecoveryScheduler: CrashRecoveryScheduler = (callback, delayMs) =>
  setTimeout(callback, delayMs);

export function crashRecoveryDelayMs(failedSessionCount: number): number | undefined {
  if (failedSessionCount <= 0 || failedSessionCount >= CRASH_RECOVERY_SESSION_BUDGET) {
    return undefined;
  }
  const index = Math.min(failedSessionCount - 1, CRASH_RECOVERY_BACKOFF_MS.length - 1);
  return CRASH_RECOVERY_BACKOFF_MS[index];
}

export interface CrashRecoveryStatusDetail {
  readonly failedSessions: number;
  readonly circuitOpen: boolean;
  readonly pendingRetryDelayMs?: number;
}

export class CrashRecoveryController {
  private failedSessions = 0;
  private circuitOpen = false;
  private intentionalDepth = 0;
  private pendingTimer: ReturnType<typeof setTimeout> | undefined;
  private pendingRetryDelayMs: number | undefined;

  constructor(
    private readonly scheduler: CrashRecoveryScheduler = defaultCrashRecoveryScheduler
  ) {}

  snapshot(): CrashRecoveryStatusDetail {
    return {
      failedSessions: this.failedSessions,
      circuitOpen: this.circuitOpen,
      pendingRetryDelayMs: this.pendingRetryDelayMs
    };
  }

  isCircuitOpen(): boolean {
    return this.circuitOpen;
  }

  hasPendingRetry(): boolean {
    return this.pendingTimer !== undefined;
  }

  beginIntentionalStop(): void {
    this.intentionalDepth += 1;
    this.cancelPendingRetry();
  }

  endIntentionalStop(): void {
    this.intentionalDepth = Math.max(0, this.intentionalDepth - 1);
  }

  intentionalStopActive(): boolean {
    return this.intentionalDepth > 0;
  }

  /** Explicit `ripr: Restart Server` and coordinated restarts reset the circuit. */
  resetCircuit(): void {
    this.failedSessions = 0;
    this.circuitOpen = false;
    this.cancelPendingRetry();
  }

  noteSuccessfulSession(): void {
    this.failedSessions = 0;
    this.circuitOpen = false;
    this.cancelPendingRetry();
  }

  cancelPendingRetry(): void {
    if (this.pendingTimer !== undefined) {
      clearTimeout(this.pendingTimer);
      this.pendingTimer = undefined;
    }
    this.pendingRetryDelayMs = undefined;
  }

  /**
   * Record one unexpected session end and optionally schedule the next retry.
   * Returns `circuit_open` when the budget is exhausted, `retry_scheduled`
   * when a timer was armed, or `ignored` for intentional stops / duplicate pending.
   */
  handleUnexpectedSessionEnd(input: {
    startGeneration: number;
    readStartGeneration: () => number;
    onRetry: () => void | Promise<void>;
  }): 'ignored' | 'retry_scheduled' | 'circuit_open' {
    if (this.intentionalStopActive()) {
      return 'ignored';
    }
    if (this.circuitOpen) {
      return 'ignored';
    }
    if (this.pendingTimer !== undefined) {
      return 'ignored';
    }
    this.failedSessions += 1;
    if (this.failedSessions >= CRASH_RECOVERY_SESSION_BUDGET) {
      this.circuitOpen = true;
      this.cancelPendingRetry();
      return 'circuit_open';
    }
    const delayMs = crashRecoveryDelayMs(this.failedSessions);
    if (delayMs === undefined) {
      this.circuitOpen = true;
      return 'circuit_open';
    }
    const capturedGeneration = input.startGeneration;
    this.pendingRetryDelayMs = delayMs;
    this.pendingTimer = this.scheduler(() => {
      this.pendingTimer = undefined;
      this.pendingRetryDelayMs = undefined;
      if (input.readStartGeneration() !== capturedGeneration) {
        return;
      }
      void input.onRetry();
    }, delayMs);
    return 'retry_scheduled';
  }
}
